//! Errors for the Data Synchronization domain (not Transfer / Schema Diff).

use thiserror::Error;

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum DataSyncError {
    #[error("{0}")]
    Validation(String),
    #[error("{0}")]
    Conflict(String),
    #[error("illegal phase transition: {from:?} → {to:?}")]
    IllegalTransition { from: String, to: String },
    #[error("{0}")]
    Incompatible(String),
    #[error("{0}")]
    Cancelled(String),
    /// The request failed before a transaction could issue any target writes.
    #[error("{0}")]
    NotStarted(String),
    /// A write or transaction-finalization request may have reached the target,
    /// but no confirmed commit or rollback result is available.
    #[error("{0}")]
    OutcomeUnknown(String),
}

impl DataSyncError {
    pub fn validation(msg: impl Into<String>) -> Self {
        Self::Validation(msg.into())
    }

    pub fn conflict(msg: impl Into<String>) -> Self {
        Self::Conflict(msg.into())
    }

    pub fn incompatible(msg: impl Into<String>) -> Self {
        Self::Incompatible(msg.into())
    }

    pub fn cancelled(msg: impl Into<String>) -> Self {
        Self::Cancelled(msg.into())
    }

    pub fn not_started(msg: impl Into<String>) -> Self {
        Self::NotStarted(msg.into())
    }

    pub fn outcome_unknown(msg: impl Into<String>) -> Self {
        Self::OutcomeUnknown(msg.into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn helpers_and_display() {
        assert_eq!(DataSyncError::validation("bad").to_string(), "bad");
        assert_eq!(DataSyncError::conflict("changed").to_string(), "changed");
        assert_eq!(DataSyncError::incompatible("no pk").to_string(), "no pk");
        assert_eq!(DataSyncError::cancelled("user").to_string(), "user");
        assert_eq!(
            DataSyncError::not_started("preflight").to_string(),
            "preflight"
        );
        assert_eq!(
            DataSyncError::outcome_unknown("lost commit").to_string(),
            "lost commit"
        );
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
