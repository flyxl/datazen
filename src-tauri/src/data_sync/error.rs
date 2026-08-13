//! Errors for the Data Synchronization domain (not Transfer / Schema Diff).

use thiserror::Error;

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum DataSyncError {
    #[error("{0}")]
    Validation(String),
    #[error("illegal phase transition: {from:?} → {to:?}")]
    IllegalTransition { from: String, to: String },
    #[error("{0}")]
    Incompatible(String),
    #[error("{0}")]
    Cancelled(String),
}

impl DataSyncError {
    pub fn validation(msg: impl Into<String>) -> Self {
        Self::Validation(msg.into())
    }

    pub fn incompatible(msg: impl Into<String>) -> Self {
        Self::Incompatible(msg.into())
    }

    pub fn cancelled(msg: impl Into<String>) -> Self {
        Self::Cancelled(msg.into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn helpers_and_display() {
        assert_eq!(DataSyncError::validation("bad").to_string(), "bad");
        assert_eq!(DataSyncError::incompatible("no pk").to_string(), "no pk");
        assert_eq!(DataSyncError::cancelled("user").to_string(), "user");
        let t = DataSyncError::IllegalTransition {
            from: "comparing".into(),
            to: "executing".into(),
        };
        assert!(t.to_string().contains("comparing"));
        assert!(t.to_string().contains("executing"));
    }

    #[test]
    fn clones_and_eq() {
        let a = DataSyncError::validation("x");
        let b = a.clone();
        assert_eq!(a, b);
    }
}
