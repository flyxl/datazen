//! Sync session phase machine.
//!
//! Forbidden: COMPARING → any execute-related phase.

use serde::{Deserialize, Serialize};

use super::error::DataSyncError;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum SyncPhase {
    Draft,
    Configured,
    Comparing,
    Compared,
    Reviewing,
    GeneratingSql,
    ReadyToExecute,
    Revalidating,
    Executing,
    Completed,
    CompareFailed,
    ValidationFailed,
    ExecutionFailed,
    Cancelled,
    RolledBack,
}

impl SyncPhase {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Draft => "DRAFT",
            Self::Configured => "CONFIGURED",
            Self::Comparing => "COMPARING",
            Self::Compared => "COMPARED",
            Self::Reviewing => "REVIEWING",
            Self::GeneratingSql => "GENERATING_SQL",
            Self::ReadyToExecute => "READY_TO_EXECUTE",
            Self::Revalidating => "REVALIDATING",
            Self::Executing => "EXECUTING",
            Self::Completed => "COMPLETED",
            Self::CompareFailed => "COMPARE_FAILED",
            Self::ValidationFailed => "VALIDATION_FAILED",
            Self::ExecutionFailed => "EXECUTION_FAILED",
            Self::Cancelled => "CANCELLED",
            Self::RolledBack => "ROLLED_BACK",
        }
    }

    pub fn is_terminal_success(self) -> bool {
        self == Self::Completed
    }

    pub fn is_failure(self) -> bool {
        matches!(
            self,
            Self::CompareFailed
                | Self::ValidationFailed
                | Self::ExecutionFailed
                | Self::Cancelled
                | Self::RolledBack
        )
    }

    pub fn is_comparing(self) -> bool {
        self == Self::Comparing
    }

    pub fn is_execute_related(self) -> bool {
        matches!(
            self,
            Self::GeneratingSql
                | Self::ReadyToExecute
                | Self::Revalidating
                | Self::Executing
                | Self::Completed
        )
    }

    pub fn can_transition_to(self, to: SyncPhase) -> bool {
        if self == to {
            return false;
        }
        // Hard rule: never jump from live compare into execute pipeline.
        if self == Self::Comparing && to.is_execute_related() {
            return false;
        }
        matches!(
            (self, to),
            (Self::Draft, Self::Configured)
                | (Self::Draft, Self::Cancelled)
                | (Self::Configured, Self::Comparing)
                | (Self::Configured, Self::Draft)
                | (Self::Configured, Self::Cancelled)
                | (Self::Comparing, Self::Compared)
                | (Self::Comparing, Self::CompareFailed)
                | (Self::Comparing, Self::Cancelled)
                | (Self::Compared, Self::Reviewing)
                | (Self::Compared, Self::Comparing)
                | (Self::Compared, Self::Cancelled)
                | (Self::Reviewing, Self::GeneratingSql)
                | (Self::Reviewing, Self::Comparing)
                | (Self::Reviewing, Self::Cancelled)
                | (Self::GeneratingSql, Self::ReadyToExecute)
                | (Self::GeneratingSql, Self::ValidationFailed)
                | (Self::GeneratingSql, Self::Cancelled)
                | (Self::ReadyToExecute, Self::Revalidating)
                | (Self::ReadyToExecute, Self::Reviewing)
                | (Self::ReadyToExecute, Self::ValidationFailed)
                | (Self::ReadyToExecute, Self::Cancelled)
                | (Self::Revalidating, Self::Executing)
                | (Self::Revalidating, Self::ValidationFailed)
                | (Self::Revalidating, Self::Cancelled)
                | (Self::Executing, Self::Completed)
                | (Self::Executing, Self::ExecutionFailed)
                | (Self::Executing, Self::Cancelled)
                | (Self::Executing, Self::RolledBack)
                | (Self::Completed, Self::Comparing)
                | (Self::Completed, Self::Cancelled)
                | (Self::CompareFailed, Self::Configured)
                | (Self::CompareFailed, Self::Comparing)
                | (Self::CompareFailed, Self::Cancelled)
                | (Self::ValidationFailed, Self::ReadyToExecute)
                | (Self::ValidationFailed, Self::Reviewing)
                | (Self::ValidationFailed, Self::Cancelled)
                | (Self::ExecutionFailed, Self::ReadyToExecute)
                | (Self::ExecutionFailed, Self::RolledBack)
                | (Self::ExecutionFailed, Self::Cancelled)
                | (Self::Cancelled, Self::Draft)
                | (Self::Cancelled, Self::Configured)
                | (Self::RolledBack, Self::Reviewing)
                | (Self::RolledBack, Self::Comparing)
                | (Self::RolledBack, Self::Cancelled)
        )
    }

    pub fn transition(self, to: SyncPhase) -> Result<SyncPhase, DataSyncError> {
        if self.can_transition_to(to) {
            Ok(to)
        } else {
            Err(DataSyncError::IllegalTransition {
                from: self.as_str().to_string(),
                to: to.as_str().to_string(),
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn happy_path_to_completed() {
        let mut phase = SyncPhase::Draft;
        for next in [
            SyncPhase::Configured,
            SyncPhase::Comparing,
            SyncPhase::Compared,
            SyncPhase::Reviewing,
            SyncPhase::GeneratingSql,
            SyncPhase::ReadyToExecute,
            SyncPhase::Revalidating,
            SyncPhase::Executing,
            SyncPhase::Completed,
        ] {
            phase = phase.transition(next).expect("legal");
        }
        assert!(phase.is_terminal_success());
        assert!(!phase.is_failure());
    }

    #[test]
    fn comparing_cannot_jump_to_execute_pipeline() {
        for to in [
            SyncPhase::GeneratingSql,
            SyncPhase::ReadyToExecute,
            SyncPhase::Revalidating,
            SyncPhase::Executing,
            SyncPhase::Completed,
        ] {
            let err = SyncPhase::Comparing.transition(to).unwrap_err();
            match err {
                DataSyncError::IllegalTransition { from, to: dest } => {
                    assert_eq!(from, "COMPARING");
                    assert_eq!(dest, to.as_str());
                }
                other => panic!("unexpected {other:?}"),
            }
        }
    }

    #[test]
    fn draft_cannot_compare_or_execute() {
        assert!(SyncPhase::Draft.transition(SyncPhase::Comparing).is_err());
        assert!(SyncPhase::Draft.transition(SyncPhase::Executing).is_err());
        assert!(SyncPhase::Draft.transition(SyncPhase::Configured).is_ok());
    }

    #[test]
    fn compared_cannot_skip_review() {
        assert!(SyncPhase::Compared
            .transition(SyncPhase::Executing)
            .is_err());
        assert!(SyncPhase::Compared
            .transition(SyncPhase::ReadyToExecute)
            .is_err());
        assert!(SyncPhase::Compared.transition(SyncPhase::Reviewing).is_ok());
    }

    #[test]
    fn cancel_and_retry_paths() {
        assert!(SyncPhase::Comparing
            .transition(SyncPhase::Cancelled)
            .is_ok());
        assert!(SyncPhase::Cancelled.transition(SyncPhase::Draft).is_ok());
        assert!(SyncPhase::CompareFailed
            .transition(SyncPhase::Comparing)
            .is_ok());
        assert!(SyncPhase::ExecutionFailed
            .transition(SyncPhase::RolledBack)
            .is_ok());
        assert!(SyncPhase::RolledBack
            .transition(SyncPhase::Reviewing)
            .is_ok());
        assert!(SyncPhase::Completed
            .transition(SyncPhase::Comparing)
            .is_ok());
        assert!(SyncPhase::ReadyToExecute
            .transition(SyncPhase::ValidationFailed)
            .is_ok());
    }

    #[test]
    fn same_phase_is_not_a_transition() {
        assert!(SyncPhase::Draft.transition(SyncPhase::Draft).is_err());
    }

    #[test]
    fn serde_and_helpers() {
        let json = serde_json::to_string(&SyncPhase::ReadyToExecute).unwrap();
        assert_eq!(json, "\"READY_TO_EXECUTE\"");
        let back: SyncPhase = serde_json::from_str(&json).unwrap();
        assert_eq!(back, SyncPhase::ReadyToExecute);
        assert!(SyncPhase::CompareFailed.is_failure());
        assert!(SyncPhase::Comparing.is_comparing());
        assert!(SyncPhase::Executing.is_execute_related());
        assert!(!SyncPhase::Reviewing.is_execute_related());
        for phase in [
            SyncPhase::Draft,
            SyncPhase::Configured,
            SyncPhase::Comparing,
            SyncPhase::Compared,
            SyncPhase::Reviewing,
            SyncPhase::GeneratingSql,
            SyncPhase::ReadyToExecute,
            SyncPhase::Revalidating,
            SyncPhase::Executing,
            SyncPhase::Completed,
            SyncPhase::CompareFailed,
            SyncPhase::ValidationFailed,
            SyncPhase::ExecutionFailed,
            SyncPhase::Cancelled,
            SyncPhase::RolledBack,
        ] {
            assert!(!phase.as_str().is_empty());
        }
    }
}
