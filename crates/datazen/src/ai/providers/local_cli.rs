//! Local CLI agent providers.
//!
//! The eight desktop presets (Claude Code, Codex, Cursor, OpenCode, Grok,
//! Qoder, CodeBuddy, Pi) all use the same adapter. The adapter starts the
//! configured executable with a clean environment and isolated working
//! directory, writes the prompt to stdin (never argv), forwards stdout as
//! content chunks and sanitized stderr as reasoning chunks, and kills the
//! process group on cancellation or timeout.
//!
//! This module deliberately has no Tauri dependency so the desktop and any
//! future host can share the same process contract.

use std::collections::HashMap;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use serde::Serialize;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, Command};
use tokio::sync::{mpsc, RwLock};
use tokio_util::sync::CancellationToken;

use super::local_cli_presets::*;
use crate::ai::domain::traits::AiProvider;
use crate::ai::domain::types::{
    AiError, AiProviderConfig, AiProviderType, CompletionRequest, CompletionResponse, MessageRole,
    StreamChunk, TokenUsage,
};

/// Environment variables allowed into a local CLI child process.
///
/// This is an allow-list, not a deny-list: a new credential variable added by a
/// shell, proxy, CI, or vendor CLI is invisible unless it is explicitly listed
/// here. `HOME`/`USERPROFILE` are included because the CLIs keep their own
/// login state in the user profile; the file contents are not copied into argv
/// or logs by this adapter.
pub const SAFE_ENV_VARS: &[&str] = &[
    "PATH",
    "HOME",
    "USERPROFILE",
    "APPDATA",
    "LOCALAPPDATA",
    "XDG_CONFIG_HOME",
    "XDG_DATA_HOME",
    "XDG_CACHE_HOME",
    "TMPDIR",
    "TMP",
    "TEMP",
    "LANG",
    "LC_ALL",
    "LC_CTYPE",
    "TERM",
    "SHELL",
    "SystemRoot",
    "WINDIR",
    "COMSPEC",
    "PATHEXT",
];

/// A local CLI preset. Commands are static, user-overridable names; the prompt
/// is always sent over stdin so it never appears in the process table.
#[derive(Debug, Clone, Copy)]
pub struct LocalCliPreset {
    /// Stable IPC/serde identifier, e.g. `claude_code`.
    pub id: &'static str,
    pub provider_type: AiProviderType,
    pub display_name: &'static str,
    /// Default command used when PATH detection runs.
    pub command: &'static str,
    /// Additional binary names to try (for CLIs that renamed their entrypoint).
    pub executable_names: &'static [&'static str],
    /// Non-prompt arguments. These are fixed per preset and contain no user data.
    pub args: &'static [&'static str],
    pub default_model: &'static str,
    /// Human-readable installation hint used in availability errors.
    pub install_hint: &'static str,
}

/// Availability result shown by Settings and returned by validation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalCliAvailability {
    pub available: bool,
    /// Safe display name (basename only), never the full configured path.
    pub executable: Option<String>,
    pub reason: String,
}

/// Return all eight presets in product order.
pub fn local_cli_presets() -> &'static [LocalCliPreset] {
    LOCAL_CLI_PRESETS
}

/// Alias kept short for call sites that already live in the local-CLI module.
pub fn presets() -> &'static [LocalCliPreset] {
    local_cli_presets()
}

impl LocalCliPreset {
    pub fn all() -> &'static [LocalCliPreset] {
        local_cli_presets()
    }

    pub fn by_type(provider_type: AiProviderType) -> Option<&'static LocalCliPreset> {
        preset_for_type(provider_type)
    }

    pub fn executable(&self) -> &'static str {
        self.command
    }
}

/// Resolve a preset by provider type.
pub fn preset_for_type(provider_type: AiProviderType) -> Option<&'static LocalCliPreset> {
    LOCAL_CLI_PRESETS
        .iter()
        .find(|preset| preset.provider_type == provider_type)
}

/// Discover a local CLI using the process PATH. `configured_path` may be an
/// absolute/relative executable path or an alternate command name.
pub fn detect_local_cli(
    provider_type: AiProviderType,
    configured_path: Option<&str>,
) -> Option<LocalCliAvailability> {
    preset_for_type(provider_type).map(|preset| detect_preset(preset, configured_path))
}

/// Detect one preset. Kept separate so tests can exercise all eight without a
/// provider instance.
pub fn detect_preset(
    preset: &LocalCliPreset,
    configured_path: Option<&str>,
) -> LocalCliAvailability {
    let path_env = std::env::var_os("PATH");
    match resolve_executable(preset, configured_path, path_env.as_deref()) {
        Ok(path) => LocalCliAvailability {
            available: true,
            executable: path
                .file_name()
                .and_then(|name| name.to_str())
                .map(ToOwned::to_owned),
            reason: format!("{} is available", preset.display_name),
        },
        Err(reason) => LocalCliAvailability {
            available: false,
            executable: None,
            reason,
        },
    }
}

fn resolve_executable(
    preset: &LocalCliPreset,
    configured_path: Option<&str>,
    path_env: Option<&std::ffi::OsStr>,
) -> Result<PathBuf, String> {
    let configured = configured_path.map(str::trim).filter(|v| !v.is_empty());
    if let Some(configured) = configured {
        if looks_like_path(configured) {
            let path = PathBuf::from(configured);
            return executable_at(path).ok_or_else(|| {
                format!(
                    "Configured {} executable was not found or is not executable",
                    preset.display_name
                )
            });
        }
        return find_on_path(configured, path_env).ok_or_else(|| {
            format!(
                "Configured {} executable was not found in PATH",
                preset.display_name
            )
        });
    }

    for name in preset
        .executable_names
        .iter()
        .copied()
        .chain(std::iter::once(preset.command))
    {
        if let Some(path) = find_on_path(name, path_env) {
            return Ok(path);
        }
    }

    Err(format!(
        "{} executable '{}' was not found in PATH. {}",
        preset.display_name, preset.command, preset.install_hint
    ))
}

fn looks_like_path(value: &str) -> bool {
    value.contains('/') || value.contains('\\') || Path::new(value).is_absolute()
}

fn executable_at(path: PathBuf) -> Option<PathBuf> {
    let metadata = std::fs::metadata(&path).ok()?;
    if !metadata.is_file() {
        return None;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if metadata.permissions().mode() & 0o111 == 0 {
            return None;
        }
    }
    Some(path)
}

fn find_on_path(name: &str, path_env: Option<&std::ffi::OsStr>) -> Option<PathBuf> {
    let path_env = path_env?;
    for dir in std::env::split_paths(path_env) {
        if dir.as_os_str().is_empty() {
            continue;
        }
        for candidate_name in executable_candidates(name) {
            if let Some(path) = executable_at(dir.join(candidate_name)) {
                return Some(path);
            }
        }
    }
    None
}

fn executable_candidates(name: &str) -> Vec<OsString> {
    #[allow(unused_mut)]
    let mut candidates = vec![OsString::from(name)];
    #[cfg(windows)]
    {
        let has_extension = Path::new(name).extension().is_some();
        if !has_extension {
            for ext in ["exe", "cmd", "bat", "com"] {
                candidates.push(OsString::from(format!("{name}.{ext}")));
            }
        }
    }
    candidates
}

/// Env pairs that can be passed to a child. Only present variables are
/// returned; no credential/proxy variables are ever copied.
pub fn isolated_env() -> Vec<(OsString, OsString)> {
    SAFE_ENV_VARS
        .iter()
        .filter_map(|name| std::env::var_os(name).map(|value| (OsString::from(name), value)))
        .collect()
}

struct LocalCliState {
    executable: PathBuf,
    workspace: Option<PathBuf>,
    timeout: Duration,
}

/// Shared adapter for all eight local CLI presets.
pub struct LocalCliProvider {
    preset: &'static LocalCliPreset,
    state: RwLock<Option<LocalCliState>>,
    cancellations: Arc<RwLock<HashMap<String, CancellationToken>>>,
}

impl LocalCliProvider {
    pub fn new(preset: &'static LocalCliPreset) -> Self {
        Self {
            preset,
            state: RwLock::new(None),
            cancellations: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub fn for_type(provider_type: AiProviderType) -> Option<Self> {
        preset_for_type(provider_type).map(Self::new)
    }

    fn extra_string(config: &AiProviderConfig, keys: &[&str]) -> Option<String> {
        keys.iter()
            .find_map(|key| config.extra.get(*key))
            .and_then(serde_json::Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned)
    }

    fn executable_override(config: &AiProviderConfig) -> Option<String> {
        Self::extra_string(
            config,
            &[
                "executablePath",
                "executable_path",
                "executable",
                "cliPath",
                "cli_path",
                "command",
                "program",
                "path",
            ],
        )
    }

    fn workspace_override(config: &AiProviderConfig) -> Option<String> {
        Self::extra_string(
            config,
            &[
                "workspaceDir",
                "workspace_dir",
                "workingDirectory",
                "working_directory",
                "workdir",
                "working_dir",
                "dir",
                "cwd",
            ],
        )
    }

    fn timeout(config: &AiProviderConfig) -> Duration {
        let secs = ["timeoutSecs", "timeout_secs", "timeout_seconds", "timeout"]
            .iter()
            .find_map(|key| config.extra.get(*key))
            .and_then(|value| {
                value
                    .as_u64()
                    .or_else(|| value.as_f64().map(|secs| secs.ceil() as u64))
            })
            .unwrap_or(300)
            .clamp(1, 3600);
        Duration::from_secs(secs)
    }

    fn resolve_state(&self, config: &AiProviderConfig) -> Result<LocalCliState, AiError> {
        if config.provider_type != self.preset.provider_type {
            return Err(AiError::Validation(format!(
                "{} adapter cannot initialize {:?}",
                self.preset.display_name, config.provider_type
            )));
        }
        let availability = detect_preset(self.preset, Self::executable_override(config).as_deref());
        if !availability.available {
            return Err(AiError::ProviderNotAvailable(availability.reason));
        }
        let path_env = std::env::var_os("PATH");
        let executable = resolve_executable(
            self.preset,
            Self::executable_override(config).as_deref(),
            path_env.as_deref(),
        )
        .map_err(AiError::ProviderNotAvailable)?;
        Ok(LocalCliState {
            executable,
            workspace: Self::workspace_override(config).map(PathBuf::from),
            timeout: Self::timeout(config),
        })
    }

    async fn cancel_entry(&self, request_id: &str) {
        let mut cancellations = self.cancellations.write().await;
        if let Some(token) = cancellations.remove(request_id) {
            token.cancel();
        } else {
            // Cancel arrived before the process registered. Leave a cancelled
            // tombstone; the runner consumes it instead of losing the signal.
            let token = CancellationToken::new();
            token.cancel();
            cancellations.insert(request_id.to_string(), token);
        }
    }
}

#[async_trait]
impl AiProvider for LocalCliProvider {
    fn provider_type(&self) -> AiProviderType {
        self.preset.provider_type
    }

    fn display_name(&self) -> &str {
        self.preset.display_name
    }

    fn supports_streaming(&self) -> bool {
        true
    }

    fn supports_tools(&self) -> bool {
        // The CLI's native tool loop is opaque to the host. Exposing the same
        // database tools here would create a second, unauthorized execution
        // path, so local agents are text-only for now.
        false
    }

    async fn validate_config(&self, config: &AiProviderConfig) -> Result<(), AiError> {
        self.resolve_state(config).map(|_| ())
    }

    async fn initialize(&self, config: &AiProviderConfig) -> Result<(), AiError> {
        let state = self.resolve_state(config)?;
        *self.state.write().await = Some(state);
        Ok(())
    }

    async fn complete(&self, request: &CompletionRequest) -> Result<CompletionResponse, AiError> {
        let state = self.state.read().await;
        let state = state.as_ref().ok_or_else(|| {
            AiError::NotConfigured(format!("{} is not initialized", self.preset.display_name))
        })?;
        let output = self.run_with_cancel(request, state, None).await?;
        Ok(CompletionResponse {
            request_id: request.request_id.clone(),
            content: output.content,
            reasoning: (!output.reasoning.is_empty()).then_some(output.reasoning),
            usage: TokenUsage::default(),
            model: request.model.clone(),
            finish_reason: Some("stop".into()),
            tool_calls: None,
            response_id: None,
        })
    }

    async fn stream_complete(
        &self,
        request: &CompletionRequest,
        sender: mpsc::Sender<Result<StreamChunk, AiError>>,
    ) -> Result<(), AiError> {
        let state = self.state.read().await;
        let state = match state.as_ref() {
            Some(state) => state,
            None => {
                return Err(AiError::NotConfigured(format!(
                    "{} is not initialized",
                    self.preset.display_name
                )))
            }
        };

        match self
            .run_with_cancel(request, state, Some(sender.clone()))
            .await
        {
            Ok(output) => {
                let _ = sender
                    .send(Ok(StreamChunk {
                        content: String::new(),
                        reasoning: None,
                        done: true,
                        cancelled: false,
                        usage: Some(TokenUsage::default()),
                        tool_calls: None,
                        response_id: None,
                        egress_summary: None,
                        memory_used: None,
                    }))
                    .await;
                let _ = output;
                Ok(())
            }
            Err(error) => {
                // Match the API protocol adapters and the test provider: emit
                // the terminal error chunk for the streaming UI, then return the
                // same error to the command caller.
                let _ = sender.send(Err(clone_ai_error(&error))).await;
                Err(error)
            }
        }
    }

    async fn cancel(&self, request_id: &str) -> Result<(), AiError> {
        self.cancel_entry(request_id).await;
        Ok(())
    }

    async fn reset(&self) {
        *self.state.write().await = None;
        let tokens = std::mem::take(&mut *self.cancellations.write().await);
        for token in tokens.into_values() {
            token.cancel();
        }
    }
}

struct RunOutput {
    content: String,
    reasoning: String,
}

impl LocalCliProvider {
    async fn run_with_cancel(
        &self,
        request: &CompletionRequest,
        state: &LocalCliState,
        sender: Option<mpsc::Sender<Result<StreamChunk, AiError>>>,
    ) -> Result<RunOutput, AiError> {
        let mut cancellations = self.cancellations.write().await;
        let token = cancellations
            .remove(&request.request_id)
            .unwrap_or_default();
        cancellations.insert(request.request_id.clone(), token.clone());
        drop(cancellations);
        let result = self.run_inner(request, state, sender, token).await;
        self.cancellations.write().await.remove(&request.request_id);
        result
    }

    async fn run_inner(
        &self,
        request: &CompletionRequest,
        state: &LocalCliState,
        sender: Option<mpsc::Sender<Result<StreamChunk, AiError>>>,
        token: CancellationToken,
    ) -> Result<RunOutput, AiError> {
        let mut workspace =
            WorkspaceGuard::prepare(&request.request_id, state.workspace.as_deref())?;
        let prompt = build_prompt(request);
        let mut command = Command::new(&state.executable);
        command.args(self.preset.args);
        command.stdin(Stdio::piped());
        command.stdout(Stdio::piped());
        command.stderr(Stdio::piped());
        command.current_dir(workspace.path());
        command.env_clear();
        for (key, value) in isolated_env() {
            command.env(key, value);
        }
        command.kill_on_drop(true);
        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt;
            command.as_std_mut().process_group(0);
        }

        let mut child = command.spawn().map_err(|error| {
            AiError::ProviderNotAvailable(format!(
                "{} could not be started: {error}",
                self.preset.display_name
            ))
        })?;

        let Some(mut stdin) = child.stdin.take() else {
            kill_process_tree(&mut child).await;
            return Err(AiError::RequestFailed(format!(
                "{} did not provide stdin",
                self.preset.display_name
            )));
        };
        let prompt_bytes = prompt.into_bytes();
        let stdin_task = tokio::spawn(async move {
            let _ = stdin.write_all(&prompt_bytes).await;
            let _ = stdin.shutdown().await;
        });

        let request_cancel = request.cancel_token.clone();

        let stdout = child.stdout.take();
        let stderr = child.stderr.take();
        let stdout_task = tokio::spawn(read_stdout(stdout, sender.clone()));
        let stderr_task = tokio::spawn(read_stderr(stderr, sender.clone()));

        let result = tokio::select! {
            status = child.wait() => WaitResult::Exited(status),
            _ = wait_for_cancel(request_cancel) => WaitResult::Cancelled,
            _ = token.cancelled() => WaitResult::Cancelled,
            _ = tokio::time::sleep(state.timeout) => WaitResult::Timeout,
        };

        match result {
            WaitResult::Exited(Ok(status)) => {
                let _ = stdin_task.await;
                let content = stdout_task.await.unwrap_or_default();
                let stderr = stderr_task.await.unwrap_or_default();
                workspace.cleanup();
                if !status.success() {
                    let detail = if stderr.trim().is_empty() {
                        "no error output".to_string()
                    } else {
                        stderr.trim().to_string()
                    };
                    return Err(AiError::RequestFailed(format!(
                        "{} exited with {}: {}",
                        self.preset.display_name, status, detail
                    )));
                }
                Ok(RunOutput {
                    content,
                    reasoning: stderr,
                })
            }
            WaitResult::Exited(Err(error)) => {
                kill_process_tree(&mut child).await;
                abort_reader_tasks(stdin_task, stdout_task, stderr_task).await;
                workspace.cleanup();
                Err(AiError::Internal(format!(
                    "{} process wait failed: {error}",
                    self.preset.display_name
                )))
            }
            WaitResult::Cancelled => {
                kill_process_tree(&mut child).await;
                abort_reader_tasks(stdin_task, stdout_task, stderr_task).await;
                workspace.cleanup();
                Err(AiError::Cancelled(request.request_id.clone()))
            }
            WaitResult::Timeout => {
                kill_process_tree(&mut child).await;
                abort_reader_tasks(stdin_task, stdout_task, stderr_task).await;
                workspace.cleanup();
                Err(AiError::Timeout(state.timeout.as_secs()))
            }
        }
    }
}

fn clone_ai_error(error: &AiError) -> AiError {
    match error {
        AiError::NotConfigured(value) => AiError::NotConfigured(value.clone()),
        AiError::InvalidApiKey => AiError::InvalidApiKey,
        AiError::RequestFailed(value) => AiError::RequestFailed(value.clone()),
        AiError::RateLimited { retry_after_secs } => AiError::RateLimited {
            retry_after_secs: *retry_after_secs,
        },
        AiError::ModelNotFound(value) => AiError::ModelNotFound(value.clone()),
        AiError::ContextLengthExceeded { used, limit } => AiError::ContextLengthExceeded {
            used: *used,
            limit: *limit,
        },
        AiError::ProviderNotAvailable(value) => AiError::ProviderNotAvailable(value.clone()),
        AiError::NotSupported(value) => AiError::NotSupported(value.clone()),
        AiError::Cancelled(value) => AiError::Cancelled(value.clone()),
        AiError::Validation(value) => AiError::Validation(value.clone()),
        AiError::Timeout(secs) => AiError::Timeout(*secs),
        AiError::Internal(value) => AiError::Internal(value.clone()),
    }
}

enum WaitResult {
    Exited(std::io::Result<std::process::ExitStatus>),
    Cancelled,
    Timeout,
}

async fn abort_reader_tasks(
    stdin_task: tokio::task::JoinHandle<()>,
    stdout_task: tokio::task::JoinHandle<String>,
    stderr_task: tokio::task::JoinHandle<String>,
) {
    // After a kill the direct child may have left a descendant holding the
    // pipes open. Do not let those pipes turn a cancellation into a timeout:
    // abort the readers (they are the remaining sender clones) and reap them.
    stdin_task.abort();
    stdout_task.abort();
    stderr_task.abort();
    let _ = stdin_task.await;
    let _ = stdout_task.await;
    let _ = stderr_task.await;
}

async fn wait_for_cancel(token: Option<CancellationToken>) {
    match token {
        Some(token) => token.cancelled().await,
        None => std::future::pending::<()>().await,
    }
}

async fn read_stdout(
    stdout: Option<tokio::process::ChildStdout>,
    sender: Option<mpsc::Sender<Result<StreamChunk, AiError>>>,
) -> String {
    let Some(stdout) = stdout else {
        return String::new();
    };
    let mut reader = stdout;
    let mut buffer = [0u8; 4096];
    let mut output = String::new();
    loop {
        match reader.read(&mut buffer).await {
            Ok(0) => break,
            Ok(read) => {
                let text = String::from_utf8_lossy(&buffer[..read]).into_owned();
                output.push_str(&text);
                if let Some(sender) = sender.as_ref() {
                    let _ = sender
                        .send(Ok(StreamChunk {
                            content: text,
                            reasoning: None,
                            done: false,
                            cancelled: false,
                            usage: None,
                            tool_calls: None,
                            response_id: None,
                            egress_summary: None,
                            memory_used: None,
                        }))
                        .await;
                }
            }
            Err(_) => break,
        }
    }
    output
}

async fn read_stderr(
    stderr: Option<tokio::process::ChildStderr>,
    sender: Option<mpsc::Sender<Result<StreamChunk, AiError>>>,
) -> String {
    let Some(stderr) = stderr else {
        return String::new();
    };
    let mut reader = BufReader::new(stderr);
    let mut output = String::new();
    let mut line = String::new();
    loop {
        line.clear();
        match reader.read_line(&mut line).await {
            Ok(0) => break,
            Ok(_) => {
                let sanitized = sanitize_stderr(&line);
                if sanitized.trim().is_empty() {
                    continue;
                }
                output.push_str(&sanitized);
                if let Some(sender) = sender.as_ref() {
                    let _ = sender
                        .send(Ok(StreamChunk {
                            content: String::new(),
                            reasoning: Some(sanitized),
                            done: false,
                            cancelled: false,
                            usage: None,
                            tool_calls: None,
                            response_id: None,
                            egress_summary: None,
                            memory_used: None,
                        }))
                        .await;
                }
            }
            Err(_) => break,
        }
    }
    output
}

/// Remove credentials and absolute paths from child stderr before it reaches
/// an IPC payload, log, or the UI. The existing cross-form redactors are the
/// only implementation; this adapter does not keep a weaker local copy.
fn sanitize_stderr(input: &str) -> String {
    let secrets = datazen_infra::redaction::redact_secrets_for_log(input);
    let secrets = credential_assignment_re()
        .replace_all(&secrets, "${1}***")
        .into_owned();
    let secrets = secret_token_re().replace_all(&secrets, "***").into_owned();
    let paths = datazen_infra::redaction::redact_paths_for_ipc(&secrets);
    paths.chars().take(4096).collect()
}

fn credential_assignment_re() -> &'static regex::Regex {
    use std::sync::OnceLock;
    static RE: OnceLock<regex::Regex> = OnceLock::new();
    RE.get_or_init(|| {
        regex::Regex::new(
            r"(?i)((?:api[_-]?key|access[_-]?token|refresh[_-]?token|token|secret|password|passwd|credential|authorization)\s*[:=]\s*)([^\s,;]+)",
        )
        .expect("credential assignment regex")
    })
}

fn secret_token_re() -> &'static regex::Regex {
    use std::sync::OnceLock;
    static RE: OnceLock<regex::Regex> = OnceLock::new();
    RE.get_or_init(|| {
        regex::Regex::new(
            r"(?i)\b(?:sk-[a-z0-9_-]{8,}|ghp_[a-z0-9]{20,}|github_pat_[a-z0-9_]{20,}|xox[baprs]-[a-z0-9-]{10,}|AKIA[0-9A-Z]{16})\b",
        )
        .expect("secret token regex")
    })
}

fn build_prompt(request: &CompletionRequest) -> String {
    let mut prompt = String::new();
    for message in &request.messages {
        let role = match message.role {
            MessageRole::System => "System",
            MessageRole::User => "User",
            MessageRole::Assistant => "Assistant",
            MessageRole::Tool => "Tool",
        };
        prompt.push_str(role);
        prompt.push_str(": ");
        prompt.push_str(&message.content);
        prompt.push_str("\n\n");
    }
    prompt
}

struct WorkspaceGuard {
    path: PathBuf,
    cleanup: bool,
}

impl WorkspaceGuard {
    fn prepare(request_id: &str, configured: Option<&Path>) -> Result<Self, AiError> {
        if let Some(configured) = configured {
            std::fs::create_dir_all(configured).map_err(|error| {
                AiError::RequestFailed(format!(
                    "Could not prepare the configured workspace: {error}"
                ))
            })?;
            return Ok(Self {
                path: configured.to_path_buf(),
                cleanup: false,
            });
        }
        let safe_id: String = request_id
            .chars()
            .filter(|ch| ch.is_ascii_alphanumeric() || *ch == '-')
            .take(32)
            .collect();
        let path = std::env::temp_dir().join(format!(
            "datazen-cli-{}-{}-{}",
            std::process::id(),
            safe_id,
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&path).map_err(|_| {
            AiError::RequestFailed("Could not prepare an isolated workspace".to_string())
        })?;
        Ok(Self {
            path,
            cleanup: true,
        })
    }

    fn path(&self) -> &Path {
        &self.path
    }

    fn cleanup(&mut self) {
        if self.cleanup {
            let _ = std::fs::remove_dir_all(&self.path);
            self.cleanup = false;
        }
    }
}

impl Drop for WorkspaceGuard {
    fn drop(&mut self) {
        self.cleanup();
    }
}

async fn kill_process_tree(child: &mut Child) {
    #[cfg(unix)]
    if let Some(pid) = child.id() {
        // The child was spawned as a process-group leader. Killing the group
        // also terminates helper processes started by the CLI (MCP servers,
        // language servers, shell wrappers).
        unsafe {
            libc::kill(-(pid as i32), libc::SIGKILL);
        }
    }
    #[cfg(windows)]
    if let Some(pid) = child.id() {
        let _ = std::process::Command::new("taskkill")
            .args(["/PID", &pid.to_string(), "/T", "/F"])
            .output();
    }
    let _ = child.kill().await;
    let _ = child.wait().await;
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use crate::ai::domain::types::{ChatMessage, CompletionRequest, MessageRole};
    use std::os::unix::fs::PermissionsExt;
    use std::sync::Arc;
    use std::time::Instant;

    fn script(dir: &tempfile::TempDir, body: &str) -> PathBuf {
        let path = dir.path().join("fake-agent");
        std::fs::write(&path, format!("#!/bin/sh\n{body}\n")).unwrap();
        let mut permissions = std::fs::metadata(&path).unwrap().permissions();
        permissions.set_mode(0o755);
        std::fs::set_permissions(&path, permissions).unwrap();
        path
    }

    fn request(id: &str) -> CompletionRequest {
        CompletionRequest {
            request_id: id.into(),
            model: "local".into(),
            messages: vec![
                ChatMessage {
                    role: MessageRole::System,
                    content: "system prompt".into(),
                    reasoning: None,
                    tool_calls: None,
                    tool_call_id: None,
                },
                ChatMessage {
                    role: MessageRole::User,
                    content: "hello from user".into(),
                    reasoning: None,
                    tool_calls: None,
                    tool_call_id: None,
                },
            ],
            temperature: None,
            stop: None,
            tools: None,
            previous_response_id: None,
            cancel_token: None,
        }
    }

    async fn provider_for(path: &Path, timeout_secs: u64) -> LocalCliProvider {
        let provider = LocalCliProvider::new(local_cli_presets().first().unwrap());
        let config = AiProviderConfig {
            provider_type: provider.provider_type(),
            api_key: None,
            endpoint: None,
            model: String::new(),
            max_tokens: 1,
            extra: serde_json::json!({
                "executablePath": path,
                "timeoutSecs": timeout_secs,
            }),
        };
        provider.initialize(&config).await.unwrap();
        provider
    }

    #[tokio::test]
    async fn streams_stdout_in_order_and_prompt_through_stdin_not_argv() {
        let dir = tempfile::tempdir().unwrap();
        let path = script(&dir, r#"printf 'first\n'; sleep 0.05; printf 'second\n'"#);
        let provider = provider_for(&path, 10).await;
        let (tx, mut rx) = mpsc::channel(16);
        let handle =
            tokio::spawn(async move { provider.stream_complete(&request("req-stream"), tx).await });
        let mut content = String::new();
        let mut saw_done = false;
        while let Some(item) = rx.recv().await {
            let chunk = item.unwrap();
            if chunk.done {
                saw_done = true;
            }
            content.push_str(&chunk.content);
        }
        handle.await.unwrap().unwrap();
        assert_eq!(content, "first\nsecond\n");
        assert!(saw_done);
    }

    #[tokio::test]
    async fn prompt_is_written_to_stdin_and_not_arguments() {
        let dir = tempfile::tempdir().unwrap();
        let path = script(
            &dir,
            r#"printf 'args:' ; printf ' %s' "$@" ; printf '\n'; cat"#,
        );
        let provider = provider_for(&path, 10).await;
        let response = provider.complete(&request("req-stdin")).await.unwrap();
        assert!(response.content.contains("args: -p --output-format text"));
        assert!(response.content.contains("System: system prompt"));
        assert!(response.content.contains("User: hello from user"));
    }

    #[tokio::test]
    async fn stderr_secrets_and_absolute_paths_are_removed_from_stream_reasoning() {
        let dir = tempfile::tempdir().unwrap();
        let path = script(
            &dir,
            r#"printf 'progress\n' >&2; printf 'token=supersecret /Users/alice/private/db\n' >&2; printf 'ok\n'"#,
        );
        let provider = provider_for(&path, 10).await;
        let (tx, mut rx) = mpsc::channel(16);
        let handle = tokio::spawn(async move {
            provider
                .stream_complete(&request("req-stderr-stream"), tx)
                .await
        });
        let mut reasoning = String::new();
        let mut content = String::new();
        while let Some(item) = rx.recv().await {
            let chunk = item.unwrap();
            content.push_str(&chunk.content);
            reasoning.push_str(chunk.reasoning.as_deref().unwrap_or(""));
        }
        handle.await.unwrap().unwrap();
        assert_eq!(content, "ok\n");
        assert!(reasoning.contains("progress"), "{reasoning}");
        assert!(!reasoning.contains("supersecret"), "{reasoning}");
        assert!(!reasoning.contains("/Users/alice"), "{reasoning}");
        assert!(reasoning.contains("***"), "{reasoning}");
    }

    #[tokio::test]
    async fn sensitive_environment_is_not_inherited() {
        std::env::set_var("DATAZEN_TEST_SECRET_TOKEN", "leak-me");
        std::env::set_var("HTTPS_PROXY", "http://user:password@proxy.example");
        let dir = tempfile::tempdir().unwrap();
        let path = script(
            &dir,
            r#"printf 'secret=%s\n' "${DATAZEN_TEST_SECRET_TOKEN:-unset}"; printf 'proxy=%s\n' "${HTTPS_PROXY:-unset}"; printf 'path=%s\n' "${PATH:+set}""#,
        );
        let provider = provider_for(&path, 10).await;
        let response = provider.complete(&request("req-env")).await.unwrap();
        assert!(
            response.content.contains("secret=unset"),
            "{}",
            response.content
        );
        assert!(
            response.content.contains("proxy=unset"),
            "{}",
            response.content
        );
        assert!(
            response.content.contains("path=set"),
            "{}",
            response.content
        );
        assert!(!SAFE_ENV_VARS.contains(&"HTTPS_PROXY"));
        assert!(!SAFE_ENV_VARS
            .iter()
            .any(|name| name.to_ascii_uppercase().contains("TOKEN")));
    }

    #[tokio::test]
    async fn default_workspace_is_an_isolated_temporary_directory() {
        let dir = tempfile::tempdir().unwrap();
        let path = script(&dir, "pwd");
        let provider = provider_for(&path, 10).await;
        let response = provider.complete(&request("req-cwd")).await.unwrap();
        let cwd = std::env::current_dir().unwrap();
        assert_ne!(PathBuf::from(response.content.trim()), cwd);
        assert!(
            response.content.contains("datazen-cli-"),
            "{}",
            response.content
        );
    }

    #[tokio::test]
    async fn nonzero_exit_maps_to_a_readable_sanitized_error() {
        let dir = tempfile::tempdir().unwrap();
        let path = script(
            &dir,
            r#"printf 'boom token=supersecret /Users/alice/private\n' >&2; exit 7"#,
        );
        let provider = provider_for(&path, 10).await;
        let error = provider.complete(&request("req-fail")).await.unwrap_err();
        let message = error.to_string();
        assert!(message.contains("exited with"), "{message}");
        assert!(message.contains("boom"), "{message}");
        assert!(!message.contains("supersecret"), "{message}");
        assert!(!message.contains("/Users/alice"), "{message}");
    }

    #[tokio::test]
    async fn stream_failure_emits_a_terminal_error_chunk_and_returns_err() {
        let dir = tempfile::tempdir().unwrap();
        let path = script(
            &dir,
            r#"printf 'failed token=supersecret
' >&2; exit 9"#,
        );
        let provider = provider_for(&path, 10).await;
        let (tx, mut rx) = mpsc::channel(16);
        let handle = tokio::spawn(async move {
            provider
                .stream_complete(&request("req-stream-fail"), tx)
                .await
        });
        let mut saw_error = false;
        while let Some(item) = rx.recv().await {
            if let Err(error) = item {
                let message = error.to_string();
                assert!(message.contains("exited with"), "{message}");
                assert!(!message.contains("supersecret"), "{message}");
                saw_error = true;
            }
        }
        let error = handle.await.unwrap().unwrap_err();
        assert!(error.to_string().contains("exited with"), "{error}");
        assert!(saw_error);
    }

    #[tokio::test]
    async fn missing_executable_and_invalid_configured_path_are_readable() {
        let dir = tempfile::tempdir().unwrap();
        let missing = dir.path().join("does-not-exist");
        let provider = LocalCliProvider::new(local_cli_presets().first().unwrap());
        let config = AiProviderConfig {
            provider_type: provider.provider_type(),
            api_key: None,
            endpoint: None,
            model: String::new(),
            max_tokens: 1,
            extra: serde_json::json!({ "executablePath": missing }),
        };
        let error = provider.initialize(&config).await.unwrap_err();
        assert!(matches!(error, AiError::ProviderNotAvailable(_)));
        assert!(error.to_string().contains("was not found"), "{error}");

        let not_executable = dir.path().join("plain-file");
        std::fs::write(&not_executable, "not a program").unwrap();
        let availability = detect_preset(
            local_cli_presets().first().unwrap(),
            not_executable.to_str(),
        );
        assert!(!availability.available);
        assert!(availability
            .reason
            .contains("not found or is not executable"));
    }

    #[tokio::test]
    async fn timeout_kills_the_child_and_returns_timeout() {
        let dir = tempfile::tempdir().unwrap();
        let path = script(&dir, "sleep 30");
        let provider = provider_for(&path, 1).await;
        let started = Instant::now();
        let error = provider
            .complete(&request("req-timeout"))
            .await
            .unwrap_err();
        assert!(matches!(error, AiError::Timeout(1)), "{error}");
        assert!(started.elapsed() < Duration::from_secs(5));
    }

    #[tokio::test]
    async fn cancel_kills_the_process_tree_and_returns_cancelled() {
        let dir = tempfile::tempdir().unwrap();
        let pid_file = dir.path().join("child.pid");
        let script_body = format!(
            "sleep 30 & child=$!; printf '%s\\n' \"$child\" > {}; printf 'started\\n'; wait",
            pid_file.display()
        );
        let path = script(&dir, &script_body);
        let provider = Arc::new(provider_for(&path, 30).await);
        let (tx, mut rx) = mpsc::channel(16);
        let runner = provider.clone();
        let handle =
            tokio::spawn(async move { runner.stream_complete(&request("req-cancel"), tx).await });
        let first = rx.recv().await.expect("first chunk").unwrap();
        assert_eq!(first.content, "started\n");
        provider.cancel("req-cancel").await.unwrap();

        let mut saw_cancelled = false;
        while let Some(item) = rx.recv().await {
            if let Err(error) = item {
                saw_cancelled |= matches!(error, AiError::Cancelled(_));
            }
        }
        assert!(handle.await.unwrap().is_err());
        assert!(saw_cancelled);

        let pid: i32 = std::fs::read_to_string(&pid_file)
            .unwrap()
            .trim()
            .parse()
            .unwrap();
        let mut gone = false;
        for _ in 0..40 {
            let rc = unsafe { libc::kill(pid, 0) };
            if rc != 0 {
                gone = true;
                break;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        assert!(gone, "grandchild {pid} survived cancellation");
    }

    #[tokio::test]
    async fn request_cancel_token_also_kills_the_process() {
        let dir = tempfile::tempdir().unwrap();
        let path = script(&dir, "sleep 30");
        let provider = Arc::new(provider_for(&path, 30).await);
        let token = CancellationToken::new();
        let mut req = request("req-token-cancel");
        req.cancel_token = Some(token.clone());
        let (tx, mut rx) = mpsc::channel(16);
        let runner = provider.clone();
        let handle = tokio::spawn(async move { runner.stream_complete(&req, tx).await });
        tokio::time::sleep(Duration::from_millis(100)).await;
        token.cancel();
        let mut saw_cancelled = false;
        while let Some(item) = rx.recv().await {
            if let Err(error) = item {
                saw_cancelled |= matches!(error, AiError::Cancelled(_));
            }
        }
        assert!(handle.await.unwrap().is_err());
        assert!(saw_cancelled);
    }

    #[tokio::test]
    async fn presets_detect_available_custom_executable() {
        let dir = tempfile::tempdir().unwrap();
        let path = script(&dir, "printf ok");
        for preset in local_cli_presets() {
            let availability = detect_preset(preset, path.to_str());
            assert!(
                availability.available,
                "{}: {}",
                preset.id, availability.reason
            );
            assert_eq!(availability.executable.as_deref(), Some("fake-agent"));
            assert!(!availability.reason.contains(dir.path().to_str().unwrap()));
        }
    }

    #[tokio::test]
    async fn cancel_before_the_stream_registers_is_not_lost() {
        let dir = tempfile::tempdir().unwrap();
        let path = script(&dir, "sleep 30");
        let provider = Arc::new(provider_for(&path, 30).await);
        provider.cancel("req-early-cancel").await.unwrap();
        let started = Instant::now();
        let (tx, mut rx) = mpsc::channel(16);
        let runner = provider.clone();
        let handle = tokio::spawn(async move {
            runner
                .stream_complete(&request("req-early-cancel"), tx)
                .await
        });
        let mut saw_cancelled = false;
        while let Some(item) = rx.recv().await {
            if let Err(error) = item {
                saw_cancelled |= matches!(error, AiError::Cancelled(_));
            }
        }
        assert!(handle.await.unwrap().is_err());
        assert!(saw_cancelled);
        assert!(started.elapsed() < Duration::from_secs(3));
    }

    #[tokio::test]
    async fn cancelled_stream_does_not_wait_for_timeout() {
        let dir = tempfile::tempdir().unwrap();
        let path = script(&dir, "printf 'ready\\n'; sleep 30");
        let provider = Arc::new(provider_for(&path, 30).await);
        let (tx, mut rx) = mpsc::channel(16);
        let runner = provider.clone();
        let handle = tokio::spawn(async move {
            runner
                .stream_complete(&request("req-fast-cancel"), tx)
                .await
        });
        let _ = rx.recv().await;
        let started = Instant::now();
        provider.cancel("req-fast-cancel").await.unwrap();
        while rx.recv().await.is_some() {}
        assert!(handle.await.unwrap().is_err());
        assert!(started.elapsed() < Duration::from_secs(3));
    }
}
