//! Structured workflow errors — IPC boundaries still stringify for compatibility.

use thiserror::Error;

#[derive(Debug, Error)]
pub enum WorkflowError {
    #[error("Required variable '{0}' is missing")]
    MissingVariable(String),

    #[error("Global timeout ({0}s) exceeded")]
    GlobalTimeout(u64),

    #[error("Command step '{step_id}' requires a database connection")]
    MissingConnection { step_id: String },

    #[error("Failed to connect '{connection_id}': {message}")]
    ConnectionFailed {
        connection_id: String,
        message: String,
    },

    #[error("Unsupported driver command '{command}' for connection '{connection_id}'")]
    UnsupportedCommand {
        command: String,
        connection_id: String,
    },

    #[error("Command '{command}' is not available in workflows")]
    CommandNotInWorkflow { command: String },

    #[error("{0}")]
    Driver(String),

    #[error("{0}")]
    Validation(String),

    #[error("{0}")]
    Template(String),

    #[error("{0}")]
    Step(String),

    #[error("{0}")]
    Ai(String),

    #[error("{0}")]
    Other(String),
}

impl From<String> for WorkflowError {
    fn from(value: String) -> Self {
        Self::Other(value)
    }
}

impl From<&str> for WorkflowError {
    fn from(value: &str) -> Self {
        Self::Other(value.to_string())
    }
}

impl WorkflowError {
    pub fn into_ipc_string(self) -> String {
        self.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn displays_missing_variable() {
        let err = WorkflowError::MissingVariable("env".into());
        assert_eq!(err.to_string(), "Required variable 'env' is missing");
    }

    #[test]
    fn from_string_preserves_message() {
        let err: WorkflowError = "step failed".into();
        assert_eq!(err.to_string(), "step failed");
    }
}
