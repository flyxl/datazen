//! Structured error type for IPC command handlers.
//!
//! `CommandError` categorises failures while serialising to a plain string
//! so the frontend receives the same format as the previous `Result<T, String>`.

use crate::store::DesktopRepositoryError;
use datazen_driver_api::DriverError;
use datazen_feature_core::services::FilterRejection;
use datazen_modules::ai::AiError;

#[derive(Debug)]
pub enum CommandError {
    Store(DesktopRepositoryError),
    Driver(DriverError),
    Ai(AiError),
    Io(std::io::Error),
    Json(serde_json::Error),
    NotFound(String),
    NotConfigured(String),
    Validation(String),
    /// 连接额度已满：消息里带上限与当前用量，是可处置的用户提示而不是内部故障。
    ResourceBusy(String),
    DataSyncNotStarted(String),
    DataSyncPartiallyApplied(u64),
    DataSyncOutcomeUnknown(String),
    Internal(String),
    RestoreOutcome(Box<datazen_platform_api::dto::job::JobDetails>),
}

impl std::fmt::Display for CommandError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::RestoreOutcome(details) if details.job.effect_outcome == Some(datazen_platform_api::dto::execution::EffectOutcome::NotStarted)
                && details.domain_results.iter().any(|result| result.result_code == "restoreUnsupported") => write!(f, "Restore Unsupported: no verified retained-resource binding; no database write was submitted; inspect Job {}; automatic retry is forbidden", details.job.job_id.as_str()),
            Self::RestoreOutcome(details) => write!(f, "Restore ended {:?}, effects {:?}; inspect Job {} before creating a new restore; automatic retry is forbidden", details.job.state, details.job.effect_outcome, details.job.job_id.as_str()),
            Self::DataSyncPartiallyApplied(count) => write!(f, "{count} rows committed before execution stopped; compare current data before continuing"),
            Self::Store(e) => write!(f, "{e}"),
            Self::Driver(e) => write!(f, "{e}"),
            Self::Ai(e) => write!(f, "{e}"),
            Self::Io(e) => write!(f, "{e}"),
            Self::Json(e) => write!(f, "{e}"),
            Self::NotFound(msg)
            | Self::NotConfigured(msg)
            | Self::Validation(msg)
            | Self::ResourceBusy(msg)
            | Self::DataSyncNotStarted(msg)
            | Self::DataSyncOutcomeUnknown(msg)
            | Self::Internal(msg) => {
                write!(f, "{msg}")
            }
        }
    }
}

impl serde::Serialize for CommandError {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        if let Self::RestoreOutcome(details) = self {
            return serde::Serialize::serialize(
                &serde_json::json!({
                    "code": "restoreOutcome",
                    "message": self.to_string(),
                    "details": details,
                    "automaticRetryAllowed": false,
                    "unsupported": details.job.effect_outcome == Some(datazen_platform_api::dto::execution::EffectOutcome::NotStarted)
                        && details.domain_results.iter().any(|result| result.result_code == "restoreUnsupported"),
                }),
                serializer,
            );
        }
        // Redact secrets so IPC payloads never leak credentials (M9/B8).
        let redacted = datazen_infra::redaction::redact_secrets_for_log(&self.to_string());
        // …and the absolute paths: an IPC payload is not a log line, and it
        // travels further than the machine it was produced on (AGENTS.md:
        // "全局 IPC 错误 Payload 自动脱敏凭据与绝对路径"). The file name
        // survives; the directories do not.
        let redacted = datazen_infra::redaction::redact_paths_for_ipc(&redacted);
        serializer.serialize_str(&redacted)
    }
}

impl From<DesktopRepositoryError> for CommandError {
    fn from(e: DesktopRepositoryError) -> Self {
        Self::Store(e)
    }
}

impl From<DriverError> for CommandError {
    fn from(e: DriverError) -> Self {
        match e {
            DriverError::NotSupported(msg) => Self::Validation(msg),
            other => Self::Driver(other),
        }
    }
}

/// A filter the shared layer refused becomes a validation error.
///
/// The shared layer owns the judgement and the message; the host only says
/// which kind of failure it is. The message keeps its `grid.filter.*` prefix,
/// so the frontend can tell a filter problem from any other failure.
impl From<FilterRejection> for CommandError {
    fn from(rejection: FilterRejection) -> Self {
        Self::Validation(rejection.contract_message())
    }
}

impl From<AiError> for CommandError {
    fn from(e: AiError) -> Self {
        Self::Ai(e)
    }
}

impl From<std::io::Error> for CommandError {
    fn from(e: std::io::Error) -> Self {
        Self::Io(e)
    }
}

impl From<serde_json::Error> for CommandError {
    fn from(e: serde_json::Error) -> Self {
        Self::Json(e)
    }
}

impl From<crate::schema_diff::object_selection::ObjectSelectionError> for CommandError {
    fn from(e: crate::schema_diff::object_selection::ObjectSelectionError) -> Self {
        CommandError::Validation(e.to_string())
    }
}

impl From<crate::data_sync::DataSyncError> for CommandError {
    fn from(e: crate::data_sync::DataSyncError) -> Self {
        match e {
            crate::data_sync::DataSyncError::NotStarted(message) => {
                Self::DataSyncNotStarted(message)
            }
            crate::data_sync::DataSyncError::OutcomeUnknown(message) => {
                Self::DataSyncOutcomeUnknown(message)
            }
            other => Self::Validation(other.to_string()),
        }
    }
}

impl From<crate::data_transfer::TransferError> for CommandError {
    fn from(e: crate::data_transfer::TransferError) -> Self {
        match e {
            crate::data_transfer::TransferError::NotImplemented(msg) => Self::Validation(msg),
            other => Self::Validation(other.to_string()),
        }
    }
}

impl From<datazen_feature_core::dashboard::repository::DashboardRepositoryError> for CommandError {
    fn from(e: datazen_feature_core::dashboard::repository::DashboardRepositoryError) -> Self {
        match e {
            datazen_feature_core::dashboard::repository::DashboardRepositoryError::NotFound(id) => {
                Self::NotFound(id)
            }
            datazen_feature_core::dashboard::repository::DashboardRepositoryError::Parse(msg) => {
                Self::Validation(msg)
            }
            datazen_feature_core::dashboard::repository::DashboardRepositoryError::Db(msg) => {
                Self::Internal(msg)
            }
        }
    }
}

impl From<datazen_feature_core::dashboard::runs::DashboardRunsError> for CommandError {
    fn from(e: datazen_feature_core::dashboard::runs::DashboardRunsError) -> Self {
        match e {
            datazen_feature_core::dashboard::runs::DashboardRunsError::NotFound(id) => {
                Self::NotFound(id)
            }
            datazen_feature_core::dashboard::runs::DashboardRunsError::Parse(msg) => {
                Self::Validation(msg)
            }
            datazen_feature_core::dashboard::runs::DashboardRunsError::Db(msg) => {
                Self::Internal(msg)
            }
        }
    }
}

impl From<crate::dashboard::export::DashboardExportError> for CommandError {
    fn from(e: crate::dashboard::export::DashboardExportError) -> Self {
        match e {
            crate::dashboard::export::DashboardExportError::Validation(msg) => {
                Self::Validation(msg)
            }
            crate::dashboard::export::DashboardExportError::Io(e) => Self::Io(e),
            crate::dashboard::export::DashboardExportError::Store(e) => Self::from(e),
            crate::dashboard::export::DashboardExportError::Database(e) => {
                Self::Internal(e.to_string())
            }
        }
    }
}

impl From<crate::dashboard::execute::DashboardExecuteError> for CommandError {
    fn from(e: crate::dashboard::execute::DashboardExecuteError) -> Self {
        match e {
            crate::dashboard::execute::DashboardExecuteError::Workflow(msg) => {
                Self::Validation(msg)
            }
            crate::dashboard::execute::DashboardExecuteError::Runs(e) => Self::from(e),
            crate::dashboard::execute::DashboardExecuteError::Store(e) => Self::from(e),
        }
    }
}

impl From<crate::dashboard::create::CreateWidgetError> for CommandError {
    fn from(e: crate::dashboard::create::CreateWidgetError) -> Self {
        match e {
            crate::dashboard::create::CreateWidgetError::Store(e) => Self::from(e),
            crate::dashboard::create::CreateWidgetError::Validation(msg) => Self::Validation(msg),
            crate::dashboard::create::CreateWidgetError::Workflow(msg) => Self::Validation(msg),
        }
    }
}

impl From<String> for CommandError {
    fn from(s: String) -> Self {
        Self::Internal(s)
    }
}

impl From<&str> for CommandError {
    fn from(s: &str) -> Self {
        Self::Internal(s.to_string())
    }
}

/// Extension trait that converts any `Result<T, E>` into `Result<T, CommandError>`
/// while logging the error with `tracing::error!`.
pub trait CmdExt<T> {
    fn cmd_err(self, cmd: &str) -> Result<T, CommandError>;
}

impl<T, E: Into<CommandError>> CmdExt<T> for Result<T, E> {
    fn cmd_err(self, cmd: &str) -> Result<T, CommandError> {
        self.map_err(|e| {
            let err: CommandError = e.into();
            let redacted = datazen_infra::redaction::redact_secrets_for_log(&err.to_string());
            tracing::error!(cmd, error = %redacted);
            err
        })
    }
}

/// Legacy path-based IPC is only available in webdriver/E2E builds.
pub(crate) fn require_webdriver_path_ipc(disabled_msg: &'static str) -> Result<(), CommandError> {
    if !cfg!(feature = "webdriver") {
        return Err(CommandError::Validation(disabled_msg.into()));
    }
    Ok(())
}

/// Error text used when a merged command receives `override_path` in a
/// production build (decision 3 example wording, kept verbatim).
pub(crate) const OVERRIDE_DISABLED_MSG: &str = "path override disabled in production";

/// Webdriver-only escape hatch shared by all decision-3 merged commands
/// (`backup_database`, `restore_sql_file`, connections/app-data import-export):
/// `Some(path)` bypasses the native dialog, but is rejected in production
/// builds — the dialog's OS-level confirmation stays the only user-facing file
/// selector there. `None` never touches the gate.
pub(crate) fn resolve_override_path(
    override_path: Option<String>,
    disabled_msg: &'static str,
) -> Result<Option<std::path::PathBuf>, CommandError> {
    match override_path {
        Some(p) => {
            require_webdriver_path_ipc(disabled_msg)?;
            Ok(Some(std::path::PathBuf::from(p)))
        }
        None => Ok(None),
    }
}

/// Ensure `target` resolves to a path under `base` after canonicalization.
/// Returns the canonical target path on success.
pub(crate) fn assert_under_dir(
    base: &std::path::Path,
    target: &std::path::Path,
    cmd: &str,
) -> Result<std::path::PathBuf, CommandError> {
    let canonical_base = base.canonicalize().cmd_err(cmd)?;
    let canonical_target = target.canonicalize().cmd_err(cmd)?;
    if !canonical_target.starts_with(&canonical_base) {
        return Err(CommandError::Validation(
            "Path traversal not allowed".into(),
        ));
    }
    Ok(canonical_target)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn restore_outcomes_preserve_effects_and_forbid_automatic_retry() {
        for (state, effect, code, unsupported) in [
            ("failed", "notStarted", "restoreUnsupported", true),
            ("failed", "notStarted", "restoreInvalidInput", false),
            ("failed", "unknown", "restoreExecuted", false),
            ("failed", "partiallyApplied", "restoreExecuted", false),
            ("failed", "rolledBack", "restoreExecuted", false),
            ("cancelled", "completed", "restoreExecuted", false),
            ("cancelled", "notStarted", "restoreInvalidInput", false),
        ] {
            let details = serde_json::from_value(serde_json::json!({
                "job": {"jobId":"restore-test", "kind":"databaseRestoreApply", "state":state,
                    "stage":null, "executionIds":[], "artifactIds":[], "createdAt":"2026-10-17T00:00:00.000Z", "updatedAt":"2026-10-17T00:00:00.000Z",
                    "effectOutcome":effect},
                "stateVersion":1,
                "domainResults":[{"stageId":"restoreArtifact", "resultCode":code,
                    "outcomeCode":effect, "counters":[], "items":[], "artifactIds":[]}]
            }))
            .unwrap();
            let error = CommandError::RestoreOutcome(Box::new(details));
            let value = serde_json::to_value(&error).unwrap();
            assert_eq!(value["unsupported"], unsupported);
            assert_eq!(value["automaticRetryAllowed"], false);
            assert_eq!(value["details"]["job"]["state"], state);
            assert_eq!(value["details"]["job"]["effectOutcome"], effect);
            assert!(value["message"]
                .as_str()
                .unwrap()
                .contains("automatic retry is forbidden"));
        }
    }

    #[test]
    fn display_and_serialize_variants() {
        let cases: Vec<CommandError> = vec![
            CommandError::Store(DesktopRepositoryError::ReadError("x".into())),
            CommandError::Driver(DriverError::QueryFailed("q".into())),
            CommandError::Ai(AiError::NotConfigured("ai".into())),
            CommandError::Io(std::io::Error::new(std::io::ErrorKind::NotFound, "file")),
            CommandError::Json(serde_json::from_str::<serde_json::Value>("{").unwrap_err()),
            CommandError::NotFound("n".into()),
            CommandError::NotConfigured("nc".into()),
            CommandError::Validation("v".into()),
            CommandError::Internal("i".into()),
        ];
        for err in cases {
            let s = err.to_string();
            assert!(!s.is_empty());
            let json = serde_json::to_string(&err).unwrap();
            assert!(json.starts_with('"') && json.ends_with('"'));
        }
    }

    #[test]
    fn from_conversions_work() {
        let _: CommandError = DriverError::ConnectionFailed("x".into()).into();
        let _: CommandError = AiError::RequestFailed("fail".into()).into();
        let _: CommandError = std::io::Error::new(std::io::ErrorKind::Other, "io").into();
        let _: CommandError = "msg".into();
        let _: CommandError = "msg".to_string().into();
        let err: CommandError = crate::data_sync::refuse_overwrite_copy().into();
        assert!(crate::data_sync::is_overwrite_copy_retired_message(
            &err.to_string()
        ));
    }

    #[test]
    fn cmd_ext_logs_and_wraps() {
        let ok: Result<i32, DriverError> = Ok(7);
        assert_eq!(ok.cmd_err("test_cmd").unwrap(), 7);

        let err: Result<i32, DriverError> = Err(DriverError::ConnectionFailed("gone".into()));
        assert!(err.cmd_err("test_cmd").is_err());
    }

    #[test]
    fn cmd_ext_redacts_secrets_in_ipc_payload() {
        let err: Result<i32, DriverError> = Err(DriverError::ConnectionFailed(
            "mysql://root:s3cret@127.0.0.1:3306/db".into(),
        ));
        let mapped = err.cmd_err("test_connection").unwrap_err();
        // Display keeps the original message (useful for internal logging).
        assert!(mapped.to_string().contains("s3cret"));
        // IPC payload (Serialize) must NOT leak the password.
        let ipc_json = serde_json::to_string(&mapped).unwrap();
        assert!(
            !ipc_json.contains("s3cret"),
            "IPC must not contain plaintext password: {ipc_json}"
        );
        assert!(
            ipc_json.contains("***@"),
            "IPC must contain redacted placeholder: {ipc_json}"
        );
        // Error category must be preserved for frontend classification.
        assert!(
            ipc_json.contains("Connection failed"),
            "IPC must preserve error category: {ipc_json}"
        );
    }

    /// FKU-D5: the IPC payload must not carry an absolute path either.
    ///
    /// The credential redaction has always been part of the payload; the path
    /// was not, and it only stayed invisible because the front end dropped the
    /// whole message. Now that the message reaches the UI, the path has to be
    /// masked at the point where the payload is produced.
    #[test]
    fn cmd_ext_redacts_absolute_paths_in_ipc_payload() {
        let err: CommandError = CommandError::Validation(
            "cannot open /Users/alice/code/datazen/app.db for reading".into(),
        );
        // The log-side text keeps the path: a log line is read on the machine
        // that produced it, and the path is what the reader needs.
        assert!(err.to_string().contains("/Users/alice"));
        // The IPC payload does not.
        let ipc_json = serde_json::to_string(&err).unwrap();
        assert!(
            !ipc_json.contains("/Users/alice"),
            "IPC must not contain an absolute path: {ipc_json}"
        );
        assert!(
            ipc_json.contains("app.db"),
            "IPC must keep the file name: {ipc_json}"
        );
    }

    #[test]
    fn resolve_override_path_gates_without_webdriver_feature() {
        // Decision 3: the merged commands' `override_path` replaces the old
        // raw-path IPCs and must stay webdriver-only. `cfg!` keeps both arms
        // compiled, so this also pins the production error message.
        let gated = resolve_override_path(Some("/tmp/override.sql".into()), OVERRIDE_DISABLED_MSG);
        if cfg!(feature = "webdriver") {
            assert_eq!(
                gated.unwrap().map(|p| p.display().to_string()).as_deref(),
                Some("/tmp/override.sql")
            );
        } else {
            let err = gated.unwrap_err();
            assert!(matches!(err, CommandError::Validation(ref msg) if msg.contains("disabled")));
            assert!(err.to_string().contains("path override disabled"));
        }
        // No override must never touch the gate (production dialog flow).
        assert_eq!(
            resolve_override_path(None, OVERRIDE_DISABLED_MSG).unwrap(),
            None
        );
    }
}

impl From<datazen_feature_core::error::FeatureError> for CommandError {
    fn from(error: datazen_feature_core::error::FeatureError) -> Self {
        use datazen_feature_core::error::FeatureError;
        match error {
            FeatureError::Ai(error) => Self::Ai(error),
            FeatureError::Validation(message) => Self::Validation(message),
            FeatureError::NotFound(message) => Self::NotFound(message),
            FeatureError::NotConfigured(message) => Self::NotConfigured(message),
            FeatureError::Internal(message) => Self::Internal(message),
            FeatureError::Driver(error) => Self::from(error),
            FeatureError::Io(error) => Self::Io(error),
            FeatureError::Json(error) => Self::Json(error),
        }
    }
}

/// Map a named-view rejection onto the IPC envelope.
///
/// The two rule failures keep their `grid.view.*` prefix, and the prefix is
/// the first thing in the message: the front end classifies on it. A storage
/// failure is not a grid rule — the code space is closed — so it carries no
/// prefix and the front end shows its raw text.
///
/// The rule messages carry **no user-supplied text**: what the user typed
/// must not travel into the IPC payload or the host's log lines (D1).
impl From<datazen_modules::named_views::NamedViewError> for CommandError {
    fn from(error: datazen_modules::named_views::NamedViewError) -> Self {
        use datazen_modules::named_views::NamedViewError;
        match &error {
            NamedViewError::NameExists { .. } | NamedViewError::Invalid(_) => {
                Self::Validation(error.to_string())
            }
            NamedViewError::Storage(message) => Self::Internal(message.clone()),
        }
    }
}

/// Map a foreign-key navigation rejection onto the IPC envelope.
///
/// The `grid.fk.*` codes are a **prefix of the message**: the front end
/// classifies on that prefix and then shows the reason behind it, so the code
/// must stay the first thing in the string. A session failure is not a
/// navigation rule failure and therefore carries no grid prefix.
impl From<datazen_feature_core::services::fk_navigation::FkNavigationError> for CommandError {
    fn from(error: datazen_feature_core::services::fk_navigation::FkNavigationError) -> Self {
        use datazen_feature_core::services::fk_navigation::FkNavigationError;
        use datazen_feature_core::services::ports::FkSessionError;
        match error {
            FkNavigationError::Coded { code, message } => {
                Self::Validation(format!("{code}: {message}"))
            }
            FkNavigationError::Session(FkSessionError::NotFound(message)) => {
                Self::NotFound(message)
            }
            FkNavigationError::Session(FkSessionError::Unavailable(message)) => {
                Self::Internal(message)
            }
        }
    }
}

#[cfg(test)]
mod resource_busy_tests {
    use super::*;
    use datazen_application::error::{ApiError, ApiErrorCode};

    #[test]
    fn resource_busy_keeps_the_actionable_message() {
        let mapped = crate::platform::into_command_error(ApiError::new(
            ApiErrorCode::ResourceBusy,
            "连接数已达上限（上限 16，当前 16），请关闭部分连接后重试",
        ));
        match &mapped {
            CommandError::ResourceBusy(message) => {
                assert!(message.contains("连接数已达上限（上限 16，当前 16）"));
                assert!(message.contains("请关闭部分连接后重试"));
            }
            other => panic!("expected ResourceBusy, got {other:?}"),
        }
        assert_eq!(
            mapped.to_string(),
            "连接数已达上限（上限 16，当前 16），请关闭部分连接后重试"
        );
    }
}
