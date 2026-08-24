//! Structured error type for IPC command handlers.
//!
//! `CommandError` categorises failures while serialising to a plain string
//! so the frontend receives the same format as the previous `Result<T, String>`.

use crate::db::DriverError;
use crate::services::connection_manager::ConnectionError;
use crate::store::StoreError;
use datazen_ai_api::AiError;

#[derive(Debug)]
pub enum CommandError {
    Store(StoreError),
    Connection(ConnectionError),
    Driver(DriverError),
    Ai(AiError),
    Io(std::io::Error),
    Json(serde_json::Error),
    NotFound(String),
    NotConfigured(String),
    Validation(String),
    Internal(String),
}

impl std::fmt::Display for CommandError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Store(e) => write!(f, "{e}"),
            Self::Connection(e) => write!(f, "{e}"),
            Self::Driver(e) => write!(f, "{e}"),
            Self::Ai(e) => write!(f, "{e}"),
            Self::Io(e) => write!(f, "{e}"),
            Self::Json(e) => write!(f, "{e}"),
            Self::NotFound(msg)
            | Self::NotConfigured(msg)
            | Self::Validation(msg)
            | Self::Internal(msg) => {
                write!(f, "{msg}")
            }
        }
    }
}

impl serde::Serialize for CommandError {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_string())
    }
}

impl From<StoreError> for CommandError {
    fn from(e: StoreError) -> Self {
        Self::Store(e)
    }
}

impl From<ConnectionError> for CommandError {
    fn from(e: ConnectionError) -> Self {
        Self::Connection(e)
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

impl From<crate::data_sync::DataSyncError> for CommandError {
    fn from(e: crate::data_sync::DataSyncError) -> Self {
        Self::Validation(e.to_string())
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
            let redacted = crate::log_redact::redact_secrets_for_log(&err.to_string());
            tracing::error!(cmd, error = %redacted);
            err
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_and_serialize_variants() {
        let cases: Vec<CommandError> = vec![
            CommandError::Store(StoreError::ReadError("x".into())),
            CommandError::Connection(ConnectionError::ConnectionConfigNotFound("c".into())),
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
        let _: CommandError = ConnectionError::DbSessionNotFound("id".into()).into();
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
        let ok: Result<i32, ConnectionError> = Ok(7);
        assert_eq!(ok.cmd_err("test_cmd").unwrap(), 7);

        let err: Result<i32, ConnectionError> =
            Err(ConnectionError::DbSessionNotFound("gone".into()));
        assert!(err.cmd_err("test_cmd").is_err());
    }

    #[test]
    fn cmd_ext_preserves_original_error_for_ipc() {
        let err: Result<i32, DriverError> = Err(DriverError::ConnectionFailed(
            "mysql://root:s3cret@127.0.0.1:3306/db".into(),
        ));
        let mapped = err.cmd_err("test_connection").unwrap_err();
        // IPC payload keeps the original message; only the log line is redacted.
        assert!(mapped.to_string().contains("s3cret"));
    }
}
