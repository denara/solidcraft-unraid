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

fn tool_error(message: &Value, error: &str) -> Value {
    json!({"jsonrpc":"2.0","id":message.get("id").cloned().unwrap_or(Value::Null),"result":{"isError":true,"content":[{"type":"text","text":error}]}})
}

pub struct Worker {
    child: Child,
    input: ChildStdin,
    output: BufReader<ChildStdout>,
    failed: bool,
    model: Arc<std::sync::Mutex<crate::workspace_model::ModelStatus>>,
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
                browser_auth: directory.join(".workspace-sessions.json"),
                token: "private-test-bearer-never-log".into(),
                token_source: crate::config::TokenSource::Environment,
                log_color: false,
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
        let observation = Arc::clone(&f.state.sessions.lock().await.get(&id).unwrap().observer);
        let _activity = crate::workspace_observer::CadActivity::acquire(observation).unwrap();
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
    async fn observer_protocol_lock_queues_cad_instead_of_reporting_a_concurrent_agent() {
        let fixture = Fixture::new();
        let (_id, worker, log_id) = fixture.state.create("2025-06-18".into()).await.unwrap();
        // A short internal start/poll owns the pipe without being a real CAD
        // request. The authenticated request must wait for that exchange.
        let held = worker.lock().await;
        let (result, ()) =
            tokio::join!(fixture.state.exchange(Arc::clone(&worker), json!({"jsonrpc":"2.0","id":1,"method":"ping"}), log_id), async {
                tokio::time::sleep(Duration::from_millis(5)).await;
                drop(held);
            });
        // The deliberately silent fixture eventually hits its regular CAD
        // timeout; it must never be rejected merely because observation ran.
        assert!(result.unwrap_err().contains("timed out"));
        assert!(!fixture.rows().iter().any(|row| row.contains("session.busy")));
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

    #[tokio::test]
    async fn tool_notifications_do_not_execute_preflight_or_return_null_id_errors() {
        let fixture = Fixture::new();
        let (_id, worker, log_id) = fixture.state.create("2025-06-18".into()).await.unwrap();
        let notification = json!({"jsonrpc":"2.0","method":"tools/call","params":{"name":"execute","arguments":{"command":"motion.export_animation","params":{"path":"frames"}}}});
        assert!(fixture.state.exchange(Arc::clone(&worker), notification, log_id).await.unwrap().is_none());
        assert!(!worker.lock().await.failed);
        assert!(!fixture.rows().iter().any(|row| row.contains("cad.timed_out") || row.contains("cad.exchange_failed")));
    }

    #[tokio::test]
    async fn project_selection_is_required_immutable_and_does_not_reach_cad() {
        let fixture = Fixture::new();
        let (_id, worker, log_id) = fixture.state.create("2025-06-18".into()).await.unwrap();
        let call = |tool: &str, args: Value| json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":tool,"arguments":args}});
        let denied = fixture.state.exchange(Arc::clone(&worker), call("new_design", json!({})), log_id.clone()).await.unwrap().unwrap();
        assert_eq!(denied["result"]["isError"], true);
        assert!(denied.to_string().contains("project required"));
        assert!(!worker.lock().await.failed);
        let selected =
            fixture.state.exchange(Arc::clone(&worker), call("project_select", json!({"name":"Demo"})), log_id.clone()).await.unwrap().unwrap();
        assert_eq!(selected["result"]["structuredContent"]["project"], "Demo");
        assert!(fixture.directory.join("Demo").is_dir());
        let repeated =
            fixture.state.exchange(Arc::clone(&worker), call("project_select", json!({"name":"Demo"})), log_id.clone()).await.unwrap().unwrap();
        assert_ne!(repeated["result"]["isError"], true);
        let changed =
            fixture.state.exchange(Arc::clone(&worker), call("project_select", json!({"name":"Other"})), log_id.clone()).await.unwrap().unwrap();
        assert_eq!(changed["result"]["isError"], true);
        assert!(!fixture.directory.join("Other").exists());
        let invalid = fixture
            .state
            .exchange(Arc::clone(&worker), call("save", json!({"path":"../outside.solidcraft"})), log_id.clone())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(invalid["result"]["isError"], true);
        let list = fixture.state.exchange(Arc::clone(&worker), call("project_list", json!({})), log_id).await.unwrap().unwrap();
        assert_eq!(list["result"]["structuredContent"]["projects"], json!(["Demo"]));
        assert_eq!(worker.lock().await.model.lock().unwrap().snapshot()["project"], "Demo");
        assert!(!fixture.rows().iter().any(|row| row.contains("cad.timed_out")));
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
    fn start(
        config: &Config,
        slot: OwnedSemaphorePermit,
        mut lease: Lease,
        model: Arc<std::sync::Mutex<crate::workspace_model::ModelStatus>>,
    ) -> Result<Self, String> {
        let mut child = Command::new(&config.cli)
            .arg("mcp")
            .arg("--headless")
            .arg("--allow-path")
            .arg(&config.files)
            .arg("--quiet")
            .current_dir(&config.files)
            .env_remove("SOLVECRAFT_MCP_TOKEN")
            .env("SOLIDCRAFT_LINK_ROOT", &config.files)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .kill_on_drop(true)
            .spawn()
            .map_err(|e| format!("start CAD process: {e}"))?;
        let input = child.stdin.take().ok_or("CAD stdin unavailable")?;
        let output = BufReader::new(child.stdout.take().ok_or("CAD stdout unavailable")?);
        lease.started = true;
        Ok(Self { _slot: slot, child, input, output, failed: false, model, lease })
    }

    pub(crate) async fn exchange(&mut self, message: &Value) -> Result<Option<Value>, String> {
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
    pub model: Arc<std::sync::Mutex<crate::workspace_model::ModelStatus>>,
    pub observer: Arc<crate::workspace_observer::Observation>,
    reason: Arc<AtomicU8>,
}

impl Drop for Entry {
    fn drop(&mut self) {
        self.observer.end();
    }
}

pub struct State {
    slots: Arc<Semaphore>,
    pub config: Config,
    pub sessions: Mutex<HashMap<String, Entry>>,
    pub audit: Arc<Audit>,
    pub browser: crate::workspace::BrowserAuth,
    pub file_gate: Arc<Mutex<()>>,
    pub observer: crate::workspace_observer::Hub,
}

impl State {
    pub fn new(config: Config) -> Self {
        let audit = Arc::new(Audit::new(config.log_color));
        let browser = crate::workspace::BrowserAuth::new(&config);
        Self {
            slots: Arc::new(Semaphore::new(config.max_sessions)),
            config,
            sessions: Mutex::new(HashMap::new()),
            audit,
            browser,
            file_gate: Arc::new(Mutex::new(())),
            observer: Default::default(),
        }
    }

    pub fn slots_used(&self) -> usize {
        self.config.max_sessions.saturating_sub(self.slots.available_permits())
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
        let model = Arc::new(std::sync::Mutex::new(crate::workspace_model::ModelStatus::default()));
        let worker = Arc::new(Mutex::new(Worker::start(&self.config, slot, lease, Arc::clone(&model)).inspect_err(|_| {
            self.audit.emit(Event::StartFailed, json!({"reason":"process_start_failed"}));
        })?));
        sessions.insert(
            id.clone(),
            Entry {
                worker: Arc::clone(&worker),
                touched: Instant::now(),
                version,
                log_id: log_id.clone(),
                reason,
                model,
                observer: Arc::new(crate::workspace_observer::Observation::default()),
            },
        );
        self.audit.emit(Event::Opened, json!({"session":log_id,"active_sessions":sessions.len(),"slots_used":self.config.max_sessions.saturating_sub(self.slots.available_permits()),"slots_total":self.config.max_sessions}));
        Ok((id, worker, log_id))
    }

    pub async fn exchange(&self, worker: Arc<Mutex<Worker>>, mut message: Value, log_id: String) -> Result<Option<Value>, String> {
        let observer = self.sessions.lock().await.values().find(|entry| entry.log_id == log_id).map(|entry| Arc::clone(&entry.observer));
        let activity = match observer {
            Some(observer) => Some(crate::workspace_observer::CadActivity::acquire(observer).ok_or_else(|| {
                self.audit.emit(Event::Busy, json!({"session":log_id,"reason":"concurrent_request"}));
                "CAD session is busy; send one request at a time".to_string()
            })?),
            None => None,
        };
        let timeout = self.config.timeout;
        let audit = Arc::clone(&self.audit);
        let file_gate = Arc::clone(&self.file_gate);
        let transfers = Arc::clone(&self.browser.actions.transfers);
        let root = self.config.files.clone();
        // A disconnected HTTP client must not abandon a read and leave a late reply in
        // the pipe. The spawned task finishes the transaction even if its caller leaves.
        tokio::spawn(async move {
            let _activity = activity;
            // Observer protocol requests are short start/poll exchanges. A real CAD
            // request queues behind them rather than being rejected as session busy.
            let mut worker = if _activity.is_some() {
                worker.lock().await
            } else { worker.try_lock().map_err(|_| {
                audit.emit(Event::Busy, json!({"session":log_id,"reason":"concurrent_request"}));
                "CAD session is busy; send one request at a time".to_string()
            })? };
            let is_tool = message.get("method").and_then(Value::as_str) == Some("tools/call");
            if is_tool && message.get("id").is_none() {
                return Ok(None);
            }
            if is_tool {
                let tool = message.pointer("/params/name").and_then(Value::as_str).unwrap_or_default();
                if matches!(tool, "project_list" | "project_select") {
                    let previous = worker.model.lock().map_err(|_| "model metadata unavailable")?.project.clone();
                    let args = message.pointer("/params/arguments").cloned().unwrap_or_else(|| json!({}));
                    if let Err(error) = crate::projects::arguments(tool, &args) {
                        return Ok(Some(tool_error(&message, &error)));
                    }
                    let selected = if tool == "project_select" {
                        match args.get("name").and_then(Value::as_str) {
                            Some(name) if previous.as_deref().is_none_or(|current| current == name) => Some(name.to_owned()),
                            Some(_) => return Ok(Some(tool_error(&message, "this session already has a project; initialize a new session to select another"))),
                            None => return Ok(Some(tool_error(&message, "project_select requires a project name"))),
                        }
                    } else { None };
                    let project_root = root.clone();
                    let select_name = selected.clone();
                    let _files = file_gate.lock().await;
                    let result = tokio::task::spawn_blocking(move || {
                        if let Some(name) = select_name {
                            crate::projects::select(&project_root, &name)?;
                            Ok(json!({"project":name,"directory":name,"paths":"CAD paths are relative to this project; workspace_list_files and expected_revisions keys are relative to the shared workspace"}))
                        } else {
                            crate::projects::list(&project_root)
                        }
                    }).await.map_err(|_| "project directory unavailable")?;
                    return Ok(Some(match result {
                        Ok(value) => {
                            if let Some(name) = selected {
                                worker.model.lock().map_err(|_| "model metadata unavailable")?.project = Some(name);
                            }
                            json!({"jsonrpc":"2.0","id":message.get("id"),"result":{"structuredContent":value,"content":[{"type":"text","text":value.to_string()}]}})
                        }
                        Err(error) => tool_error(&message, &error),
                    }));
                }
                let project = worker.model.lock().map_err(|_| "model metadata unavailable")?.project.clone();
                if tool != "list_commands" {
                    let Some(project) = project else {
                        return Ok(Some(tool_error(&message, "project required: call project_select with a project name before CAD tools")));
                    };
                    if let Err(error) = crate::projects::prepare(&root, &project, &mut message) {
                        return Ok(Some(tool_error(&message, &error)));
                    }
                }
            }
            let expected = message
                .pointer_mut("/params/arguments")
                .and_then(Value::as_object_mut)
                .and_then(|a| a.remove("expected_revisions"))
                .unwrap_or(Value::Null);
            let previous = worker.model.lock().map_err(|_| "model metadata unavailable")?.clone();
            let operations = crate::workspace_model::operations(&root, &message, previous.current_file.as_deref());
            let operations = match operations {
                Ok(ops) => ops,
                Err(error) => return Ok(Some(tool_error(&message, &error))),
            };
            if let Some(project) = previous.project.as_deref()
                && let Err(error) = crate::projects::check_writes(project, &operations)
            {
                return Ok(Some(tool_error(&message, &error)));
            }
            let _files = if operations.is_empty() { None } else { Some(file_gate.lock().await) };
            for operation in &operations {
                if let crate::workspace_model::FileOperation::Write(name) | crate::workspace_model::FileOperation::Save(name) = operation
                    && let Err(error) = crate::workspace_actions::check_transfers(&transfers, name) {
                    return Ok(Some(tool_error(&message, error)));
                }
            }
            let check_root = root.clone();
            let check_ops = operations.clone();
            if let Err(error) =
                tokio::task::spawn_blocking(move || crate::workspace_model::check_revisions(&check_root, &previous, &check_ops, &expected))
                    .await
                    .map_err(|_| "revision check unavailable")?
            {
                return Ok(Some(tool_error(&message, &error)));
            }
            let transaction = async {
                let reply = worker.exchange(&message).await?;
                if is_tool || message.get("method").and_then(Value::as_str) == Some("initialize") {
                    let probe =
                        json!({"jsonrpc":"2.0","id":format!("workspace-{}",uuid::Uuid::new_v4()),"method":"solidcraft/workspaceStatus","params":{}});
                    let status = worker.exchange(&probe).await?;
                    if let Some(status) = status.and_then(|value| value.get("result").cloned()) {
                        let success = reply
                            .as_ref()
                            .is_some_and(|r| r.get("error").is_none() && r.pointer("/result/isError").and_then(Value::as_bool) != Some(true));
                        let refreshed = success && crate::workspace_model::refreshes_current(&operations, &status, &root);
                        let mut next_model = worker.model.lock().map_err(|_| "model metadata unavailable")?.clone();
                        let status_root = root.clone();
                        let next_model = tokio::task::spawn_blocking(move || {
                            next_model.update(&status_root, &status, refreshed);
                            next_model
                        })
                        .await
                        .map_err(|_| "model metadata unavailable")?;
                        *worker.model.lock().map_err(|_| "model metadata unavailable")? = next_model;
                    }
                }
                Ok(reply)
            };
            let result = match tokio::time::timeout(timeout, transaction).await {
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
