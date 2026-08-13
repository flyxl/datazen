//! In-memory sync session: task + phase + optional comparison / change set.

use serde::{Deserialize, Serialize};

use super::changeset::ChangeSet;
use super::error::DataSyncError;
use super::model::{ComparisonResult, SyncTask};
use super::state::SyncPhase;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncSession {
    pub task: SyncTask,
    pub phase: SyncPhase,
    pub comparison: Option<ComparisonResult>,
    pub change_set: Option<ChangeSet>,
}

impl SyncSession {
    pub fn new(task: SyncTask) -> Self {
        Self {
            task,
            phase: SyncPhase::Draft,
            comparison: None,
            change_set: None,
        }
    }

    fn go(&mut self, to: SyncPhase) -> Result<(), DataSyncError> {
        self.phase = self.phase.transition(to)?;
        Ok(())
    }

    pub fn configure(&mut self) -> Result<(), DataSyncError> {
        self.task.validate()?;
        self.go(SyncPhase::Configured)
    }

    pub fn start_compare(&mut self) -> Result<(), DataSyncError> {
        self.task.validate()?;
        self.comparison = None;
        self.change_set = None;
        self.go(SyncPhase::Comparing)
    }

    pub fn finish_compare(&mut self, comparison: ComparisonResult) -> Result<(), DataSyncError> {
        self.comparison = Some(comparison);
        self.go(SyncPhase::Compared)
    }

    pub fn fail_compare(&mut self, _reason: impl Into<String>) -> Result<(), DataSyncError> {
        self.go(SyncPhase::CompareFailed)
    }

    pub fn begin_review(&mut self) -> Result<(), DataSyncError> {
        if self.comparison.is_none() {
            return Err(DataSyncError::validation(
                "cannot review without a comparison result",
            ));
        }
        self.go(SyncPhase::Reviewing)
    }

    pub fn generate_change_set(&mut self) -> Result<&ChangeSet, DataSyncError> {
        self.go(SyncPhase::GeneratingSql)?;
        let comparison = self.comparison.as_ref().ok_or_else(|| {
            DataSyncError::validation("cannot generate change set without comparison")
        })?;
        let set = ChangeSet::from_comparison(&self.task.id, comparison, &self.task.options);
        self.change_set = Some(set);
        self.go(SyncPhase::ReadyToExecute)?;
        Ok(self.change_set.as_ref().expect("just assigned"))
    }

    pub fn start_revalidate(&mut self) -> Result<(), DataSyncError> {
        let set = self
            .change_set
            .as_ref()
            .ok_or_else(|| DataSyncError::validation("cannot execute without a change set"))?;
        set.validate_executable()?;
        self.go(SyncPhase::Revalidating)
    }

    pub fn start_execute(&mut self) -> Result<(), DataSyncError> {
        self.go(SyncPhase::Executing)
    }

    pub fn complete(&mut self) -> Result<(), DataSyncError> {
        self.go(SyncPhase::Completed)
    }

    pub fn fail_validation(&mut self) -> Result<(), DataSyncError> {
        self.go(SyncPhase::ValidationFailed)
    }

    pub fn fail_execution(&mut self) -> Result<(), DataSyncError> {
        self.go(SyncPhase::ExecutionFailed)
    }

    pub fn rollback(&mut self) -> Result<(), DataSyncError> {
        self.go(SyncPhase::RolledBack)
    }

    pub fn cancel(&mut self) -> Result<(), DataSyncError> {
        self.go(SyncPhase::Cancelled)
    }

    pub fn reset_to_draft(&mut self) -> Result<(), DataSyncError> {
        self.comparison = None;
        self.change_set = None;
        self.go(SyncPhase::Draft)
    }
}

#[cfg(test)]
mod tests {
    use datazen_driver_api::Value;

    use super::*;
    use crate::data_sync::model::{
        Endpoint, RowChange, SyncOptions, SyncTask, TableMapping, TableResult,
    };

    fn task() -> SyncTask {
        SyncTask::new(
            "task-1",
            Endpoint::new("a", "db_a", None),
            Endpoint::new("b", "db_b", None),
            SyncOptions::default(),
            vec![TableMapping::auto("users")],
        )
        .unwrap()
    }

    fn diff_result() -> ComparisonResult {
        let opts = SyncOptions::default();
        ComparisonResult::new(vec![TableResult::matched(
            "users",
            "users",
            vec![RowChange::insert(
                vec![Value::Integer(1)],
                vec![Some(Value::Integer(1))],
                &opts,
            )],
        )])
    }

    #[test]
    fn compare_review_execute_happy_path() {
        let mut session = SyncSession::new(task());
        assert_eq!(session.phase, SyncPhase::Draft);
        session.configure().unwrap();
        session.start_compare().unwrap();
        assert!(SyncSession::new(task())
            .start_compare()
            .unwrap_err()
            .to_string()
            .contains("DRAFT"));
        session.finish_compare(diff_result()).unwrap();
        session.begin_review().unwrap();
        let set = session.generate_change_set().unwrap();
        assert_eq!(set.insert_count(), 1);
        session.start_revalidate().unwrap();
        session.start_execute().unwrap();
        session.complete().unwrap();
        assert_eq!(session.phase, SyncPhase::Completed);
    }

    #[test]
    fn cannot_review_without_comparison() {
        let mut session = SyncSession::new(task());
        session.configure().unwrap();
        session.start_compare().unwrap();
        session.finish_compare(ComparisonResult::default()).unwrap();
        session.comparison = None;
        assert!(session.begin_review().is_err());
    }

    #[test]
    fn cannot_revalidate_empty_changeset() {
        let mut session = SyncSession::new(task());
        session.configure().unwrap();
        session.start_compare().unwrap();
        session.finish_compare(ComparisonResult::default()).unwrap();
        session.begin_review().unwrap();
        session.generate_change_set().unwrap();
        assert!(session.start_revalidate().is_err());
    }

    #[test]
    fn comparing_cannot_execute() {
        let mut session = SyncSession::new(task());
        session.configure().unwrap();
        session.start_compare().unwrap();
        assert!(session.start_execute().is_err());
        assert!(session.start_revalidate().is_err());
        session.cancel().unwrap();
        session.reset_to_draft().unwrap();
        assert_eq!(session.phase, SyncPhase::Draft);
        assert!(session.comparison.is_none());
    }

    #[test]
    fn failure_and_rollback_paths() {
        let mut session = SyncSession::new(task());
        session.configure().unwrap();
        session.start_compare().unwrap();
        session.fail_compare("timeout").unwrap();
        assert_eq!(session.phase, SyncPhase::CompareFailed);
        session.start_compare().unwrap();
        session.finish_compare(diff_result()).unwrap();
        session.begin_review().unwrap();
        session.generate_change_set().unwrap();
        session.fail_validation().unwrap();
        assert_eq!(session.phase, SyncPhase::ValidationFailed);
        session.go_ready_after_validation_fail();
        session.start_revalidate().unwrap();
        session.start_execute().unwrap();
        session.fail_execution().unwrap();
        session.rollback().unwrap();
        assert_eq!(session.phase, SyncPhase::RolledBack);
    }
}

impl SyncSession {
    #[cfg(test)]
    fn go_ready_after_validation_fail(&mut self) {
        self.go(SyncPhase::ReadyToExecute).unwrap();
    }
}
