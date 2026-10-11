use std::collections::HashMap;
use std::process::Stdio;
use std::sync::Arc;
use std::time::Instant;

use serde_json::Value;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, ChildStdout, Command};
use tokio::sync::{Mutex, OwnedSemaphorePermit, Semaphore};

use crate::config::Config;

const MAX_REPLY: u64 = 64 << 20;

pub struct Worker {
    _slot: OwnedSemaphorePermit,
    child: Child,
    input: ChildStdin,
    output: BufReader<ChildStdout>,
    failed: bool,
}

impl Worker {
    fn start(config: &Config, slot: OwnedSemaphorePermit) -> Result<Self, String> {
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
        Ok(Self { _slot: slot, child, input, output, failed: false })
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
}

pub struct State {
    slots: Arc<Semaphore>,
    pub config: Config,
    pub sessions: Mutex<HashMap<String, Entry>>,
}

impl State {
    pub fn new(config: Config) -> Self {
        Self { slots: Arc::new(Semaphore::new(config.max_sessions)), config, sessions: Mutex::new(HashMap::new()) }
    }

    pub async fn expire(&self) {
        self.sessions.lock().await.retain(|_, entry| entry.touched.elapsed() < self.config.idle || entry.worker.try_lock().is_err());
    }

    pub async fn create(&self, version: String) -> Result<(String, Arc<Mutex<Worker>>), String> {
        self.expire().await;
        let mut sessions = self.sessions.lock().await;
        if sessions.len() >= self.config.max_sessions {
            return Err("session limit reached; close an unused session or wait for its expiry".into());
        }
        // A deleted session with an in-flight operation still occupies a process slot
        // until that operation ends. Repeated DELETE/initialize cannot exceed the limit.
        let slot = Arc::clone(&self.slots).try_acquire_owned().map_err(|_| "CAD process limit reached")?;
        let id = uuid::Uuid::new_v4().to_string();
        let worker = Arc::new(Mutex::new(Worker::start(&self.config, slot)?));
        sessions.insert(id.clone(), Entry { worker: Arc::clone(&worker), touched: Instant::now(), version });
        Ok((id, worker))
    }

    pub async fn exchange(&self, worker: Arc<Mutex<Worker>>, message: Value) -> Result<Option<Value>, String> {
        let timeout = self.config.timeout;
        // A disconnected HTTP client must not abandon a read and leave a late reply in
        // the pipe. The spawned task finishes the transaction even if its caller leaves.
        tokio::spawn(async move {
            let mut worker = worker.try_lock().map_err(|_| "CAD session is busy; send one request at a time".to_string())?;
            let result = match tokio::time::timeout(timeout, worker.exchange(&message)).await {
                Ok(result) => result,
                Err(_) => Err("CAD operation timed out; this session has ended (the operation is not retried)".into()),
            };
            if result.is_err() {
                worker.failed = true;
                let _ = worker.child.kill().await;
            }
            result
        })
        .await
        .map_err(|e| e.to_string())?
    }
}
