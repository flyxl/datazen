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
            Self::NotFound(msg) | Self::NotConfigured(msg) | Self::Validation(msg) | Self::Internal(msg) => {
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
            tracing::error!(cmd, error = %err);
            err
        })
    }
}
