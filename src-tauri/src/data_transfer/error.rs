//! Errors for the Data Transfer domain (not Data Sync / Schema Diff).

use thiserror::Error;

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum TransferError {
    #[error("{0}")]
    Validation(String),
    #[error("{0}")]
    Unsupported(String),
    #[error("{0}")]
    NotImplemented(String),
    #[error("{0}")]
    Cancelled(String),
}

impl TransferError {
    pub fn validation(msg: impl Into<String>) -> Self {
        Self::Validation(msg.into())
    }

    pub fn unsupported(msg: impl Into<String>) -> Self {
        Self::Unsupported(msg.into())
    }

    pub fn not_implemented(msg: impl Into<String>) -> Self {
        Self::NotImplemented(msg.into())
    }

    pub fn cancelled(msg: impl Into<String>) -> Self {
        Self::Cancelled(msg.into())
    }
}
