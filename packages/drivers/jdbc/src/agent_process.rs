//! Lifecycle manager for the external JDBC Java Agent process.
//!
//! Phase 1: spawn, stdio JSON-RPC, hello handshake, restart-once, shutdown.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, Command};
use tokio::sync::{Mutex, oneshot};
use tokio::time::timeout;
use tracing::{debug, error, info, warn};

use crate::protocol::{
    methods, validate_hello_result, JsonRpcRequest, JsonRpcResponse, RpcError, PROTOCOL_VERSION,
};

/// Default RPC round-trip timeout.
const RPC_TIMEOUT: Duration = Duration::from_secs(30);
/// Timeout for spawn + hello only.
const HELLO_TIMEOUT: Duration = Duration::from_secs(15);

/// Where to find `java` and the agent jar.
#[derive(Debug, Clone)]
pub struct AgentLaunchConfig {
    /// Override for `java` executable; None -> JAVA_HOME / PATH.
    pub java_path: Option<PathBuf>,
    /// Path to `datazen-jdbc-agent-*.jar`.
    pub agent_jar: PathBuf,
    /// Idle timeout before shutdown when no sessions remain (seconds).
    pub idle_timeout_secs: u64,
    /// Optional host version string sent in `agent.hello`.
    pub host_version: String,
}

impl Default for AgentLaunchConfig {
    fn default() -> Self {
        Self {
            java_path: None,
            agent_jar: PathBuf::from("datazen-jdbc-agent.jar"),
            idle_timeout_secs: 600,
            host_version: env!("CARGO_PKG_VERSION").to_string(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentState {
    Stopped,
    Starting,
    Running,
    Failed,
}

struct LiveAgent {
    child: Child,
    stdin: ChildStdin,
    /// Pending RPC waiters keyed by request id.
    pending: HashMap<u64, oneshot::Sender<Result<serde_json::Value, RpcError>>>,
}

/// Manages a single shared Java Agent child process for the whole app.
pub struct AgentProcessManager {
    config: AgentLaunchConfig,
    state: Mutex<AgentState>,
    live: Mutex<Option<LiveAgent>>,
    next_id: AtomicU64,
    /// After a crash, allow exactly one automatic restart attempt.
    allow_restart: AtomicBool,
}

impl AgentProcessManager {
    pub fn new(config: AgentLaunchConfig) -> Arc<Self> {
        Arc::new(Self {
            config,
            state: Mutex::new(AgentState::Stopped),
            live: Mutex::new(None),
            next_id: AtomicU64::new(1),
            allow_restart: AtomicBool::new(true),
        })
    }

    pub async fn state(&self) -> AgentState {
        *self.state.lock().await
    }

    pub fn config(&self) -> &AgentLaunchConfig {
        &self.config
    }

    fn alloc_id(&self) -> u64 {
        self.next_id.fetch_add(1, Ordering::Relaxed)
    }

    /// Resolve `java` executable path.
    pub fn resolve_java(config: &AgentLaunchConfig) -> Result<PathBuf, String> {
        if let Some(ref p) = config.java_path {
            if p.as_os_str().is_empty() {
                return Err("java_path is empty".into());
            }
            return Ok(p.clone());
        }
        if let Ok(home) = std::env::var("JAVA_HOME") {
            let candidate = Path::new(&home).join("bin").join(if cfg!(windows) {
                "java.exe"
            } else {
                "java"
            });
            if candidate.is_file() {
                return Ok(candidate);
            }
        }
        // Fall back to PATH lookup.
        Ok(PathBuf::from(if cfg!(windows) { "java.exe" } else { "java" }))
    }

    /// Ensure the agent is running and has completed `agent.hello`.
    pub async fn ensure_running(self: &Arc<Self>) -> Result<(), String> {
        {
            let st = *self.state.lock().await;
            if st == AgentState::Running {
                // Cheap liveness: if we still hold a live handle, OK.
                let live = self.live.lock().await;
                if live.is_some() {
                    return Ok(());
                }
            }
        }
        self.start_and_hello().await
    }

    async fn start_and_hello(self: &Arc<Self>) -> Result<(), String> {
        {
            let mut st = self.state.lock().await;
            if *st == AgentState::Starting {
                return Err("JDBC Agent is already starting".into());
            }
            *st = AgentState::Starting;
        }

        let result = self.spawn_inner().await;
        match result {
            Ok(()) => {
                *self.state.lock().await = AgentState::Running;
                self.allow_restart.store(true, Ordering::SeqCst);
                Ok(())
            }
            Err(e) => {
                *self.state.lock().await = AgentState::Failed;
                let _ = self.kill_inner().await;
                Err(e)
            }
        }
    }

    async fn spawn_inner(self: &Arc<Self>) -> Result<(), String> {
        let java = Self::resolve_java(&self.config)?;
        let jar = &self.config.agent_jar;
        if !jar.is_file() {
            return Err(format!(
                "JDBC Agent jar not found: {} (place datazen-jdbc-agent.jar or set path in Settings)",
                jar.display()
            ));
        }

        info!(java = %java.display(), jar = %jar.display(), "starting JDBC Agent");

        let mut child = Command::new(&java)
            .arg("-jar")
            .arg(jar)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            .spawn()
            .map_err(|e| format!("failed to spawn JDBC Agent: {e}"))?;

        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| "agent stdin missing".to_string())?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| "agent stdout missing".to_string())?;
        let stderr = child.stderr.take();

        // Drain stderr to tracing so it never blocks the pipe.
        if let Some(stderr) = stderr {
            tokio::spawn(async move {
                let mut lines = BufReader::new(stderr).lines();
                while let Ok(Some(line)) = lines.next_line().await {
                    warn!(target: "jdbc_agent", "{line}");
                }
            });
        }

        let pending: HashMap<u64, oneshot::Sender<Result<serde_json::Value, RpcError>>> =
            HashMap::new();

        {
            let mut live = self.live.lock().await;
            *live = Some(LiveAgent {
                child,
                stdin,
                pending,
            });
        }

        // Reader task: demux stdout lines to pending oneshots.
        let mgr = Arc::clone(self);
        tokio::spawn(async move {
            let mut lines = BufReader::new(stdout).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                let trimmed = line.trim();
                if trimmed.is_empty() {
                    continue;
                }
                if !trimmed.starts_with('{') {
                    warn!(target: "jdbc_agent", "skip non-json stdout: {trimmed}");
                    continue;
                }
                match JsonRpcResponse::from_line(trimmed) {
                    Ok(resp) => {
                        if let Some(id) = resp.id_u64() {
                            let slot = {
                                let mut live = mgr.live.lock().await;
                                live.as_mut().and_then(|l| l.pending.remove(&id))
                            };
                            if let Some(tx) = slot {
                                let payload = if let Some(err) = resp.error {
                                    Err(RpcError::Remote {
                                        code: err.code,
                                        message: err.message,
                                        data: err.data,
                                    })
                                } else if let Some(result) = resp.result {
                                    Ok(result)
                                } else {
                                    Err(RpcError::Protocol(
                                        "response missing result and error".into(),
                                    ))
                                };
                                let _ = tx.send(payload);
                            } else {
                                debug!(id, "no pending waiter for agent response");
                            }
                        }
                    }
                    Err(e) => {
                        warn!(target: "jdbc_agent", "bad JSON from agent: {e}; line={trimmed}");
                    }
                }
            }
            // EOF: fail all pending and mark dead.
            warn!(target: "jdbc_agent", "agent stdout closed");
            let mut live = mgr.live.lock().await;
            if let Some(mut l) = live.take() {
                for (_, tx) in l.pending.drain() {
                    let _ = tx.send(Err(RpcError::Protocol("agent process exited".into())));
                }
                let _ = l.child.kill().await;
            }
            *mgr.state.lock().await = AgentState::Failed;
        });

        // Hello handshake.
        let hello_params = serde_json::json!({
            "hostVersion": self.config.host_version,
            "protocolVersion": PROTOCOL_VERSION,
        });
        let result = timeout(
            HELLO_TIMEOUT,
            self.rpc_unlocked(methods::AGENT_HELLO, Some(hello_params)),
        )
        .await
        .map_err(|_| "agent.hello timed out".to_string())?
        .map_err(|e| format!("agent.hello failed: {e}"))?;

        let ver = validate_hello_result(&result).map_err(|e| e.to_string())?;
        info!(agent_version = %ver, "JDBC Agent ready");
        Ok(())
    }

    /// Send a JSON-RPC request and wait for the matching response id.
    pub async fn rpc(
        self: &Arc<Self>,
        method: &str,
        params: serde_json::Value,
    ) -> Result<serde_json::Value, String> {
        self.ensure_running().await?;
        match self.rpc_unlocked(method, Some(params)).await {
            Ok(v) => Ok(v),
            Err(e) => {
                // One automatic restart if the process died.
                if self.maybe_restart_after_error(&e).await {
                    self.ensure_running().await?;
                    return self
                        .rpc_unlocked(method, Some(params))
                        .await
                        .map_err(|e| e.to_string());
                }
                Err(e.to_string())
            }
        }
    }

    async fn maybe_restart_after_error(&self, err: &RpcError) -> bool {
        let msg = err.to_string();
        let dead = msg.contains("exited") || msg.contains("not running") || msg.contains("broken");
        if !dead {
            return false;
        }
        if !self.allow_restart.swap(false, Ordering::SeqCst) {
            return false;
        }
        warn!(target: "jdbc_agent", "attempting one automatic agent restart");
        let _ = self.kill_inner().await;
        *self.state.lock().await = AgentState::Stopped;
        true
    }

    async fn rpc_unlocked(
        &self,
        method: &str,
        params: Option<serde_json::Value>,
    ) -> Result<serde_json::Value, RpcError> {
        let id = self.alloc_id();
        let req = JsonRpcRequest::new(id, method, params);
        let line = req.to_line()?;

        let (tx, rx) = oneshot::channel();
        {
            let mut live = self.live.lock().await;
            let live = live
                .as_mut()
                .ok_or_else(|| RpcError::Protocol("agent not running".into()))?;
            live.pending.insert(id, tx);
            live.stdin
                .write_all(line.as_bytes())
                .await
                .map_err(|e| RpcError::Protocol(format!("write stdin: {e}")))?;
            live.stdin
                .write_all(b"\n")
                .await
                .map_err(|e| RpcError::Protocol(format!("write newline: {e}")))?;
            live.stdin
                .flush()
                .await
                .map_err(|e| RpcError::Protocol(format!("flush stdin: {e}")))?;
        }

        match timeout(RPC_TIMEOUT, rx).await {
            Ok(Ok(result)) => result,
            Ok(Err(_)) => Err(RpcError::Protocol("RPC waiter dropped".into())),
            Err(_) => {
                // Timed out: remove pending slot.
                let mut live = self.live.lock().await;
                if let Some(l) = live.as_mut() {
                    l.pending.remove(&id);
                }
                Err(RpcError::Protocol(format!(
                    "RPC timed out after {}s (method={method})",
                    RPC_TIMEOUT.as_secs()
                )))
            }
        }
    }

    async fn kill_inner(&self) -> Result<(), String> {
        let mut live = self.live.lock().await;
        if let Some(mut l) = live.take() {
            for (_, tx) in l.pending.drain() {
                let _ = tx.send(Err(RpcError::Protocol("agent shutting down".into())));
            }
            // Best-effort graceful shutdown RPC is skipped if stdin already broken.
            let _ = l.child.kill().await;
            let _ = l.child.wait().await;
        }
        Ok(())
    }

    /// Graceful shutdown: try `agent.shutdown`, then kill.
    pub async fn shutdown(&self) -> Result<(), String> {
        {
            let live = self.live.lock().await;
            if live.is_some() {
                drop(live);
                let _ = timeout(
                    Duration::from_secs(3),
                    self.rpc_unlocked(methods::AGENT_SHUTDOWN, None),
                )
                .await;
            }
        }
        self.kill_inner().await?;
        *self.state.lock().await = AgentState::Stopped;
        Ok(())
    }
}

impl Drop for AgentProcessManager {
    fn drop(&mut self) {
        // Best-effort: if Tokio runtime is still alive, spawn kill; otherwise rely on kill_on_drop.
        if let Ok(live) = self.live.try_lock() {
            if live.is_some() {
                error!(target: "jdbc_agent", "AgentProcessManager dropped while agent still live; child kill_on_drop should reap");
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command as StdCommand;

    #[tokio::test]
    async fn default_state_is_stopped() {
        let mgr = AgentProcessManager::new(AgentLaunchConfig::default());
        assert_eq!(mgr.state().await, AgentState::Stopped);
    }

    #[tokio::test]
    async fn missing_jar_fails_clearly() {
        let mgr = AgentProcessManager::new(AgentLaunchConfig {
            agent_jar: PathBuf::from("/nonexistent/datazen-jdbc-agent.jar"),
            ..Default::default()
        });
        let err = mgr.ensure_running().await.unwrap_err();
        assert!(err.contains("not found"), "{err}");
        assert_eq!(mgr.state().await, AgentState::Failed);
    }

    /// Integration: spawn a tiny Python mock agent if `python3` exists.
    #[tokio::test]
    async fn mock_agent_hello_and_rpc() {
        let python = which_python();
        let Some(python) = python else {
            eprintln!("skip mock_agent_hello_and_rpc: no python3");
            return;
        };

        let dir = tempfile_dir();
        let script = dir.join("mock_agent.py");
        std::fs::write(
            &script,
            r#"#!/usr/bin/env python3
import sys, json
for line in sys.stdin:
    line = line.strip()
    if not line:
        continue
    req = json.loads(line)
    rid = req.get("id")
    method = req.get("method")
    if method == "agent.hello":
        print(json.dumps({"jsonrpc":"2.0","id":rid,"result":{
            "agentVersion":"0.1.0","protocolVersion":1,
            "capabilities":["jdbc"]}}), flush=True)
    elif method == "agent.shutdown":
        print(json.dumps({"jsonrpc":"2.0","id":rid,"result":{"ok":True}}), flush=True)
        break
    elif method == "ping":
        print(json.dumps({"jsonrpc":"2.0","id":rid,"result":{"pong":True}}), flush=True)
    else:
        print(json.dumps({"jsonrpc":"2.0","id":rid,"error":{
            "code":-32601,"message":"unknown","data":{"category":"internal"}}}), flush=True)
"#,
        )
        .unwrap();

        // Use python as "java" and script path encoded via a wrapper jar path trick:
        // We override by pointing java_path to a shell wrapper.
        let wrapper = dir.join("fake_java.sh");
        #[cfg(unix)]
        {
            std::fs::write(
                &wrapper,
                format!(
                    "#!/bin/sh\nexec '{}' '{}'\n",
                    python.display(),
                    script.display()
                ),
            )
            .unwrap();
            StdCommand::new("chmod").arg("+x").arg(&wrapper).status().ok();
        }
        #[cfg(not(unix))]
        {
            eprintln!("skip mock_agent on non-unix");
            return;
        }

        // agent_jar must exist as a file; create dummy.
        let dummy_jar = dir.join("dummy.jar");
        std::fs::write(&dummy_jar, b"dummy").unwrap();

        let mgr = AgentProcessManager::new(AgentLaunchConfig {
            java_path: Some(wrapper),
            agent_jar: dummy_jar,
            idle_timeout_secs: 60,
            host_version: "test".into(),
        });

        // spawn uses `java -jar jar` — our wrapper ignores args and runs python.
        // Command is: fake_java.sh -jar dummy.jar — shell script still execs python.
        mgr.ensure_running().await.expect("hello");
        assert_eq!(mgr.state().await, AgentState::Running);

        let pong = mgr
            .rpc("ping", serde_json::json!({}))
            .await
            .expect("ping");
        assert_eq!(pong["pong"], true);

        mgr.shutdown().await.ok();
        assert_eq!(mgr.state().await, AgentState::Stopped);
    }

    fn which_python() -> Option<PathBuf> {
        for name in ["python3", "python"] {
            if StdCommand::new(name).arg("--version").output().is_ok() {
                return Some(PathBuf::from(name));
            }
        }
        None
    }

    fn tempfile_dir() -> PathBuf {
        let dir = std::env::temp_dir().join(format!("datazen-jdbc-test-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        dir
    }
}
