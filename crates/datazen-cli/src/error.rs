//! Stable CLI error codes.
//!
//! The codes are part of the automation contract: the official Agent Skill and
//! scripts branch on them, so they must not be renamed when wording changes.

use serde::Serialize;
use thiserror::Error;

pub const E_USAGE: &str = "E_USAGE";
pub const E_CONFIG: &str = "E_CONFIG";
pub const E_NOT_FOUND: &str = "E_NOT_FOUND";
pub const E_CREDENTIAL: &str = "E_CREDENTIAL";
pub const E_READ_ONLY: &str = "E_READ_ONLY";
pub const E_WRITE_FLAG: &str = "E_WRITE_FLAG";
pub const E_DANGEROUS_SQL: &str = "E_DANGEROUS_SQL";
pub const E_PRODUCTION_WRITE: &str = "E_PRODUCTION_WRITE";
pub const E_TIMEOUT: &str = "E_TIMEOUT";
pub const E_DRIVER: &str = "E_DRIVER";
pub const E_EXECUTOR: &str = "E_EXECUTOR";
pub const E_OUTPUT: &str = "E_OUTPUT";
pub const E_INTERNAL: &str = "E_INTERNAL";

#[derive(Debug, Clone, Error)]
#[error("{code}: {message}")]
pub struct CliError {
    pub code: &'static str,
    pub message: String,
}

impl CliError {
    pub fn new(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }

    pub fn exit_code(&self) -> i32 {
        match self.code {
            E_INTERNAL => 1,
            E_USAGE => 2,
            E_CONFIG => 3,
            E_NOT_FOUND => 4,
            E_CREDENTIAL => 5,
            E_READ_ONLY => 6,
            E_WRITE_FLAG => 7,
            E_DANGEROUS_SQL => 8,
            E_PRODUCTION_WRITE => 9,
            E_TIMEOUT => 10,
            E_DRIVER => 11,
            E_EXECUTOR => 12,
            E_OUTPUT => 13,
            _ => 1,
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ErrorPayload<'a> {
    pub ok: bool,
    pub code: &'a str,
    pub message: &'a str,
}

impl std::fmt::Display for ErrorPayload<'_> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{}: {}", self.code, self.message)
    }
}

impl CliError {
    pub fn payload(&self) -> ErrorPayload<'_> {
        ErrorPayload {
            ok: false,
            code: self.code,
            message: &self.message,
        }
    }
}
