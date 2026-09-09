//! Host-synced JDBC plugin settings (`AppSettings.pluginSettings.jdbc`).
//!
//! Mirrors the Redis pattern (`SETTINGS_KEY` + `set_settings_*`):
//! the host calls [`apply_plugin_settings`] on boot and on every
//! `save_settings`. Environment variables override file/UI values for CI.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};

use crate::agent_process::AgentLaunchConfig;

/// Key under `AppSettings.pluginSettings` (must match frontend `pluginId: 'jdbc'`).
pub const SETTINGS_KEY: &str = "jdbc";

const ENV_JAVA: &str = "DATAZEN_JDBC_JAVA";
const ENV_AGENT_JAR: &str = "DATAZEN_JDBC_AGENT_JAR";
const ENV_IDLE: &str = "DATAZEN_JDBC_IDLE_TIMEOUT";

static LAUNCH: OnceLock<Mutex<AgentLaunchConfig>> = OnceLock::new();
/// Bumped whenever launch config changes so a running agent can restart.
static CONFIG_EPOCH: AtomicU64 = AtomicU64::new(1);

fn launch_lock() -> &'static Mutex<AgentLaunchConfig> {
    LAUNCH.get_or_init(|| Mutex::new(AgentLaunchConfig::default()))
}

/// Monotonic epoch; AgentProcessManager compares after settings apply.
pub fn config_epoch() -> u64 {
    CONFIG_EPOCH.load(Ordering::SeqCst)
}

/// Snapshot used when spawning the agent (env overrides applied).
pub fn resolved_launch_config() -> AgentLaunchConfig {
    let mut cfg = launch_lock()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .clone();
    apply_env_overrides(&mut cfg);
    cfg.agent_jar = resolve_agent_jar_path(&cfg.agent_jar);
    cfg
}

/// Install settings from `pluginSettings.jdbc` JSON (or `{}` for defaults).
pub fn apply_plugin_settings(value: &serde_json::Value) {
    let mut next = AgentLaunchConfig::default();

    if let Some(s) = value.get("javaPath").and_then(|v| v.as_str()) {
        let trimmed = s.trim();
        if !trimmed.is_empty() {
            next.java_path = Some(PathBuf::from(trimmed));
        }
    }
    if let Some(s) = value.get("agentJarPath").and_then(|v| v.as_str()) {
        let trimmed = s.trim();
        if !trimmed.is_empty() {
            next.agent_jar = PathBuf::from(trimmed);
        }
    }
    if let Some(n) = value
        .get("idleTimeoutSecs")
        .and_then(|v| v.as_u64().or_else(|| v.as_f64().map(|f| f as u64)))
    {
        next.idle_timeout_secs = n.max(60);
    }

    apply_env_overrides(&mut next);

    let mut guard = launch_lock().lock().unwrap_or_else(|e| e.into_inner());
    let changed = guard.java_path != next.java_path
        || guard.agent_jar != next.agent_jar
        || guard.idle_timeout_secs != next.idle_timeout_secs;
    *guard = next;
    drop(guard);
    if changed {
        CONFIG_EPOCH.fetch_add(1, Ordering::SeqCst);
        tracing::info!(
            target: "jdbc_agent",
            epoch = config_epoch(),
            "JDBC agent launch config updated from plugin settings"
        );
    }
}

fn apply_env_overrides(cfg: &mut AgentLaunchConfig) {
    if let Ok(j) = std::env::var(ENV_JAVA) {
        let t = j.trim();
        if !t.is_empty() {
            cfg.java_path = Some(PathBuf::from(t));
        }
    }
    if let Ok(j) = std::env::var(ENV_AGENT_JAR) {
        let t = j.trim();
        if !t.is_empty() {
            cfg.agent_jar = PathBuf::from(t);
        }
    }
    if let Ok(s) = std::env::var(ENV_IDLE) {
        if let Ok(n) = s.trim().parse::<u64>() {
            cfg.idle_timeout_secs = n.max(60);
        }
    }
}

/// Resolve a possibly relative agent jar path.
///
/// Search order when the configured path is relative or missing as a file:
/// 1. As given (absolute or cwd-relative)
/// 2. Next to the current executable
/// 3. `./datazen-jdbc-agent/target/datazen-jdbc-agent.jar` (dev monorepo)
/// 4. `./datazen-jdbc-agent.jar`
pub fn resolve_agent_jar_path(configured: &Path) -> PathBuf {
    if configured.is_file() {
        return configured.to_path_buf();
    }

    let name = configured
        .file_name()
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("datazen-jdbc-agent.jar"));

    let mut candidates: Vec<PathBuf> = Vec::new();
    candidates.push(configured.to_path_buf());

    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            candidates.push(dir.join(&name));
            // macOS app bundle: Contents/MacOS → Resources
            if let Some(contents) = dir.parent() {
                candidates.push(contents.join("Resources").join(&name));
            }
        }
    }
    if let Ok(cwd) = std::env::current_dir() {
        candidates.push(cwd.join(&name));
        candidates.push(cwd.join("datazen-jdbc-agent").join("target").join(&name));
        candidates.push(cwd.join("datazen-jdbc-agent").join(&name));
    }

    for c in &candidates {
        if c.is_file() {
            tracing::debug!(path = %c.display(), "resolved JDBC agent jar");
            return c.clone();
        }
    }

    // Keep the configured path so the spawn error message stays actionable.
    configured.to_path_buf()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn apply_reads_java_and_jar() {
        apply_plugin_settings(&serde_json::json!({
            "javaPath": "/usr/bin/java",
            "agentJarPath": "/opt/datazen-jdbc-agent.jar",
            "idleTimeoutSecs": 120,
        }));
        let cfg = resolved_launch_config();
        assert_eq!(cfg.java_path.as_deref(), Some(Path::new("/usr/bin/java")));
        assert_eq!(cfg.agent_jar, PathBuf::from("/opt/datazen-jdbc-agent.jar"));
        assert_eq!(cfg.idle_timeout_secs, 120);
    }

    #[test]
    fn empty_java_means_path_lookup() {
        apply_plugin_settings(&serde_json::json!({
            "javaPath": "  ",
            "agentJarPath": "datazen-jdbc-agent.jar",
        }));
        let cfg = resolved_launch_config();
        assert!(cfg.java_path.is_none());
    }

    #[test]
    fn idle_clamped_to_min_60() {
        apply_plugin_settings(&serde_json::json!({ "idleTimeoutSecs": 10 }));
        assert_eq!(resolved_launch_config().idle_timeout_secs, 60);
    }
}
