use std::collections::HashMap;
use std::process::Stdio;
use std::sync::Arc;
use std::sync::atomic::{AtomicU8, Ordering};
use std::time::Instant;

use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, ChildStdout, Command};
use tokio::sync::{Mutex, OwnedSemaphorePermit, Semaphore};

use crate::audit::{Audit, Event};
use crate::config::Config;

const MAX_REPLY: u64 = 64 << 20;

pub struct Worker {
    child: Child,
    input: ChildStdin,
    output: BufReader<ChildStdout>,
    failed: bool,
    // Fields drop in declaration order: kill the child, then release its permit,
    // then report the actual freed capacity (even after an in-flight DELETE).
    _slot: OwnedSemaphorePermit,
    lease: Lease,
}

#[cfg(all(test, unix))]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;
    use std::time::Duration;

    struct Fixture {
        state: State,
        directory: std::path::PathBuf,
    }

    impl Fixture {
        fn new() -> Self {
            let directory = std::env::temp_dir().join(format!("solidcraft-log-test-{}", uuid::Uuid::new_v4()));
            std::fs::create_dir(&directory).unwrap();
            let cli = directory.join("test-cad");
            // A real child that keeps stdin/stdout open without answering requests.
            std::fs::write(&cli, "#!/bin/sh\nexec sleep 30\n").unwrap();
            std::fs::set_permissions(&cli, std::fs::Permissions::from_mode(0o700)).unwrap();
            let state = State::new(Config {
                listen: "127.0.0.1:0".into(),
                cli: cli.to_string_lossy().into_owned(),
                web: directory.clone(),
                files: directory.clone(),
                token: "private-test-bearer-never-log".into(),
                origins: vec!["https://private-test-origin.invalid".into()],
                max_sessions: 1,
                idle: Duration::from_secs(60),
                timeout: Duration::from_millis(10),
            });
            Self { state, directory }
        }

        fn rows(&self) -> Vec<String> {
            self.state.audit.rows.lock().unwrap().clone()
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.directory);
        }
    }

    #[tokio::test]
    async fn deleting_in_flight_session_logs_release_only_when_slot_is_really_free() {
        let f = Fixture::new();
        let (id, worker, log_id) = f.state.create("2025-06-18".into()).await.unwrap();
        assert_ne!(id, log_id);
        assert!(f.state.remove(&id, EndReason::Client).await);
        assert!(f.state.create("2025-06-18".into()).await.is_err());
        let held = f.rows().join("\n");
        assert!(held.contains("session.closing") && held.contains("process_limit"));
        assert!(!held.contains("session.released"));
        drop(worker);
        let released = f.rows().join("\n");
        assert!(released.contains("session.released") && released.contains("\"slots_used\":0"));
        assert!(released.contains("client_delete") && released.contains(&log_id));
        assert!(!released.contains(&id) && !released.contains(&f.state.config.token));
        assert!(f.state.create("2025-06-18".into()).await.is_ok());
    }

    #[tokio::test]
    async fn expiry_keeps_busy_worker_and_logs_idle_release_without_its_http_id() {
        let mut f = Fixture::new();
        f.state.config.idle = Duration::ZERO;
        let (id, worker, _) = f.state.create("2025-06-18".into()).await.unwrap();
        let guard = worker.lock().await;
        f.state.expire().await;
        assert!(f.state.sessions.lock().await.contains_key(&id));
        drop(guard);
        f.state.expire().await;
        assert!(f.state.sessions.lock().await.is_empty());
        drop(worker);
        let rows = f.rows().join("\n");
        assert!(rows.contains("idle_timeout") && rows.contains("session.released"));
        assert!(!rows.contains(&id));
    }

    #[tokio::test]
    async fn timeout_records_reason_and_releases_failed_worker_without_request_payload() {
        let f = Fixture::new();
        let (id, worker, log_id) = f.state.create("2025-06-18".into()).await.unwrap();
        let result = f
            .state
            .exchange(worker, json!({"jsonrpc":"2.0","id":1,"method":"private-request-marker","params":{"secret":f.state.config.token}}), log_id)
            .await;
        assert!(result.is_err());
        assert!(f.state.remove(&id, EndReason::Failure).await);
        let rows = f.rows().join("\n");
        assert!(rows.contains("cad.timed_out") && rows.contains("operation_timeout"));
        assert!(rows.contains("session.released") && rows.contains("\"slots_used\":0"));
        for private in [&id, &f.state.config.token, &f.state.config.cli, "private-request-marker"] {
            assert!(!rows.contains(private));
        }
    }

    #[tokio::test]
    async fn failed_start_logs_error_without_leaking_cli_path_or_holding_capacity() {
        let mut f = Fixture::new();
        f.state.config.cli = f.directory.join("private-missing-cli").to_string_lossy().into_owned();
        assert!(f.state.create("2025-06-18".into()).await.is_err());
        assert_eq!(f.state.slots.available_permits(), 1);
        let rows = f.rows().join("\n");
        assert!(rows.contains("[error]") && rows.contains("cad.start_failed"));
        assert!(!rows.contains(&f.state.config.cli) && !rows.contains("session.released"));
    }

    #[tokio::test]
    async fn concurrent_request_logs_busy_without_ending_the_working_session() {
        let f = Fixture::new();
        let (id, worker, log_id) = f.state.create("2025-06-18".into()).await.unwrap();
        let mut held = worker.lock().await;
        let result = f.state.exchange(Arc::clone(&worker), json!({"jsonrpc":"2.0","id":1,"method":"ping"}), log_id.clone()).await;
        assert!(result.is_err() && !held.failed);
        assert!(held.child.try_wait().unwrap().is_none());
        let rows = f.rows().join("\n");
        assert!(rows.contains("session.busy") && rows.contains(&log_id));
        assert!(!rows.contains("cad.exchange_failed") && !rows.contains(&id));
        drop(held);
        f.state.remove(&id, EndReason::Client).await;
        drop(worker);
    }

    #[tokio::test]
    async fn closed_cad_pipe_logs_failure_and_its_actual_capacity_release() {
        let f = Fixture::new();
        std::fs::write(&f.state.config.cli, "#!/bin/sh\nexit 0\n").unwrap();
        let (id, worker, log_id) = f.state.create("2025-06-18".into()).await.unwrap();
        let result = f.state.exchange(worker, json!({"jsonrpc":"2.0","id":1,"method":"ping"}), log_id).await;
        assert!(result.is_err());
        f.state.remove(&id, EndReason::Failure).await;
        let rows = f.rows().join("\n");
        assert!(rows.contains("cad.exchange_failed") && rows.contains("cad_failure"));
        assert!(rows.contains("slots=0/1") && !rows.contains(&id));
    }
}

#[derive(Clone, Copy)]
pub enum EndReason {
    Shutdown = 0,
    Client = 1,
    Idle = 2,
    Failure = 3,
    Timeout = 4,
}

impl EndReason {
    fn name(value: u8) -> &'static str {
        match value {
            1 => "client_delete",
            2 => "idle_timeout",
            3 => "cad_failure",
            4 => "operation_timeout",
            _ => "shutdown",
        }
    }
}

struct Lease {
    id: String,
    reason: Arc<AtomicU8>,
    audit: Arc<Audit>,
    slots: Arc<Semaphore>,
    total: usize,
    started: bool,
}

impl Drop for Lease {
    fn drop(&mut self) {
        if self.started {
            self.audit.emit(Event::Released, json!({"session":self.id,"reason":EndReason::name(self.reason.load(Ordering::Relaxed)),"slots_used":self.total.saturating_sub(self.slots.available_permits()),"slots_total":self.total}));
        }
    }
}

impl Worker {
    fn start(config: &Config, slot: OwnedSemaphorePermit, mut lease: Lease) -> Result<Self, String> {
        let mut child = Command::new(&config.cli)
            .arg("mcp")
            .current_dir(&config.files)
            .env_remove("SOLVECRAFT_MCP_TOKEN")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .kill_on_drop(true)
            .spawn()
            .map_err(|e| format!("start CAD process: {e}"))?;
        let input = child.stdin.take().ok_or("CAD stdin unavailable")?;
        let output = BufReader::new(child.stdout.take().ok_or("CAD stdout unavailable")?);
        lease.started = true;
        Ok(Self { _slot: slot, child, input, output, failed: false, lease })
    }

    async fn exchange(&mut self, message: &Value) -> Result<Option<Value>, String> {
        if self.failed {
            return Err("CAD session has ended; initialize a new session and reopen your saved design".into());
        }
        let mut bytes = serde_json::to_vec(message).map_err(|e| e.to_string())?;
        bytes.push(b'\n');
        self.input.write_all(&bytes).await.map_err(|e| e.to_string())?;
        self.input.flush().await.map_err(|e| e.to_string())?;
        if message.get("id").is_none() || message.get("method").is_none() {
            return Ok(None);
        }
        let mut line = Vec::new();
        (&mut self.output).take(MAX_REPLY).read_until(b'\n', &mut line).await.map_err(|e| e.to_string())?;
        if line.last() != Some(&b'\n') {
            return Err("CAD process closed or exceeded the response limit".into());
        }
        let reply: Value = serde_json::from_slice(&line).map_err(|e| e.to_string())?;
        if reply.get("id") != message.get("id") {
            return Err("CAD response ID did not match the request".into());
        }
        Ok(Some(reply))
    }
}

pub struct Entry {
    pub worker: Arc<Mutex<Worker>>,
    pub touched: Instant,
    pub version: String,
    pub log_id: String,
    reason: Arc<AtomicU8>,
}

pub struct State {
    slots: Arc<Semaphore>,
    pub config: Config,
    pub sessions: Mutex<HashMap<String, Entry>>,
    pub audit: Arc<Audit>,
}

impl State {
    pub fn new(config: Config) -> Self {
        Self { slots: Arc::new(Semaphore::new(config.max_sessions)), config, sessions: Mutex::new(HashMap::new()), audit: Arc::new(Audit::default()) }
    }

    pub async fn expire(&self) {
        self.sessions.lock().await.retain(|_, entry| {
            if entry.touched.elapsed() < self.config.idle || entry.worker.try_lock().is_err() {
                true
            } else {
                let _ = entry.reason.compare_exchange(EndReason::Shutdown as u8, EndReason::Idle as u8, Ordering::Relaxed, Ordering::Relaxed);
                false
            }
        });
    }

    pub async fn remove(&self, id: &str, reason: EndReason) -> bool {
        let mut sessions = self.sessions.lock().await;
        let Some(entry) = sessions.remove(id) else { return false };
        // A timeout/failure may already have recorded a more specific end reason.
        let _ = entry.reason.compare_exchange(EndReason::Shutdown as u8, reason as u8, Ordering::Relaxed, Ordering::Relaxed);
        if Arc::strong_count(&entry.worker) > 1 {
            self.audit.emit(Event::Closing, json!({"session":entry.log_id,"reason":EndReason::name(entry.reason.load(Ordering::Relaxed)),"active_sessions":sessions.len(),"slots_used":self.config.max_sessions.saturating_sub(self.slots.available_permits()),"slots_total":self.config.max_sessions}));
        }
        true
    }

    pub async fn create(&self, version: String) -> Result<(String, Arc<Mutex<Worker>>, String), String> {
        self.expire().await;
        let mut sessions = self.sessions.lock().await;
        if sessions.len() >= self.config.max_sessions {
            self.audit
                .emit(Event::Capacity, json!({"reason":"session_limit","slots_total":self.config.max_sessions,"active_sessions":sessions.len()}));
            return Err("session limit reached; close an unused session or wait for its expiry".into());
        }
        // A deleted session with an in-flight operation still occupies a process slot
        // until that operation ends. Repeated DELETE/initialize cannot exceed the limit.
        let slot = Arc::clone(&self.slots).try_acquire_owned().map_err(|_| {
            self.audit
                .emit(Event::Capacity, json!({"reason":"process_limit","slots_total":self.config.max_sessions,"active_sessions":sessions.len()}));
            "CAD process limit reached"
        })?;
        let id = uuid::Uuid::new_v4().to_string();
        // This ID is for logs only, independent of the HTTP session credential.
        let log_id = format!("s-{}", uuid::Uuid::new_v4().simple());
        let reason = Arc::new(AtomicU8::new(EndReason::Shutdown as u8));
        let lease = Lease {
            id: log_id.clone(),
            reason: Arc::clone(&reason),
            audit: Arc::clone(&self.audit),
            slots: Arc::clone(&self.slots),
            total: self.config.max_sessions,
            started: false,
        };
        let worker = Arc::new(Mutex::new(Worker::start(&self.config, slot, lease).inspect_err(|_| {
            self.audit.emit(Event::StartFailed, json!({"reason":"process_start_failed"}));
        })?));
        sessions.insert(id.clone(), Entry { worker: Arc::clone(&worker), touched: Instant::now(), version, log_id: log_id.clone(), reason });
        self.audit.emit(Event::Opened, json!({"session":log_id,"active_sessions":sessions.len(),"slots_used":self.config.max_sessions.saturating_sub(self.slots.available_permits()),"slots_total":self.config.max_sessions}));
        Ok((id, worker, log_id))
    }

    pub async fn exchange(&self, worker: Arc<Mutex<Worker>>, message: Value, log_id: String) -> Result<Option<Value>, String> {
        let timeout = self.config.timeout;
        let audit = Arc::clone(&self.audit);
        // A disconnected HTTP client must not abandon a read and leave a late reply in
        // the pipe. The spawned task finishes the transaction even if its caller leaves.
        tokio::spawn(async move {
            let mut worker = worker.try_lock().map_err(|_| {
                audit.emit(Event::Busy, json!({"session":log_id,"reason":"concurrent_request"}));
                "CAD session is busy; send one request at a time".to_string()
            })?;
            let result = match tokio::time::timeout(timeout, worker.exchange(&message)).await {
                Ok(result) => result,
                Err(_) => {
                    worker.lease.reason.store(EndReason::Timeout as u8, Ordering::Relaxed);
                    audit.emit(Event::TimedOut, json!({"session":worker.lease.id,"timeout_seconds":timeout.as_secs()}));
                    Err("CAD operation timed out; this session has ended (the operation is not retried)".into())
                }
            };
            if result.is_err() {
                if worker.lease.reason.load(Ordering::Relaxed) != EndReason::Timeout as u8 {
                    worker.lease.reason.store(EndReason::Failure as u8, Ordering::Relaxed);
                    audit.emit(Event::CadFailed, json!({"session":worker.lease.id,"reason":"process_or_protocol_failure"}));
                }
                worker.failed = true;
                let _ = worker.child.kill().await;
            }
            result
        })
        .await
        .map_err(|e| e.to_string())?
    }
}
