//! The outcome vocabulary of a Workflow migration step.
//!
//! A migration step runs one of three operations — data transfer, data sync
//! or schema diff — and each one reports its result in the same shape: the
//! step payload, the counters the migration run history records, and the
//! rollback outcome. The rules that map an engine result onto that shape live
//! here so every host reports the same thing; the host only runs the engine.
//!
//! The transfer counterpart lives next to the other transfer rules in
//! [`super::migration`]; it is kept here with the sync/schema pair because all
//! three describe the same vocabulary.

use super::migration::MigrationOutcome;
use datazen_data_sync::ExecutionResult as SyncExecutionResult;
use datazen_schema_diff::{DeployStatus, SchemaDiffDeployResult};

/// The outcome of a data synchronization step.
pub fn sync_migration_outcome(result: &SyncExecutionResult) -> MigrationOutcome {
    MigrationOutcome {
        result: serde_json::json!({
            "operation": "dataSync",
            "applied": result.applied,
            "affectedRows": result.affected_rows,
            "skipped": result.skipped,
            "conflicts": result.conflicts.len(),
            "rolledBack": result.rolled_back,
        }),
        committed: result.affected_rows,
        failed: u64::from(result.rolled_back),
        conflicts: result.conflicts.len() as u64,
        cancelled: false,
        success: !result.rolled_back,
        rollback_outcome: if result.rolled_back {
            "completed"
        } else {
            "notRequired"
        },
    }
}

/// The outcome of a schema diff step.
pub fn schema_migration_outcome(result: &SchemaDiffDeployResult) -> MigrationOutcome {
    let cancelled = matches!(result.status, DeployStatus::Cancelled);
    let success = matches!(result.status, DeployStatus::Committed);
    MigrationOutcome {
        result: serde_json::json!({
            "operation": "schemaDiff",
            "status": result.status,
            "executed": result.executed_count,
            "statements": result.statement_count,
            "errors": result.errors.len(),
            "cancelled": cancelled,
        }),
        committed: result.executed_count as u64,
        failed: result.errors.len() as u64,
        conflicts: 0,
        cancelled,
        success,
        rollback_outcome: match result.status {
            DeployStatus::RolledBack => "completed",
            DeployStatus::Unknown | DeployStatus::Mixed => "unknown",
            _ => "notRequired",
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_rolled_back_sync_is_reported_as_a_failure() {
        let result = SyncExecutionResult {
            applied: 3,
            rolled_back: true,
            rollback_reason: Some("boom".into()),
            affected_rows: 7,
            skipped: 1,
            conflicts: Vec::new(),
        };
        let outcome = sync_migration_outcome(&result);
        assert!(!outcome.success);
        assert_eq!(outcome.rollback_outcome, "completed");
        assert_eq!(outcome.failed, 1);
        assert_eq!(outcome.committed, 7);
        assert_eq!(outcome.result["operation"], "dataSync");
        assert_eq!(outcome.result["rolledBack"], true);
        assert_eq!(outcome.result["affectedRows"], 7);
    }

    #[test]
    fn a_committed_schema_diff_is_reported_as_a_success() {
        let result = SchemaDiffDeployResult {
            status: DeployStatus::Committed,
            executed_count: 4,
            statement_count: 6,
            errors: Vec::new(),
            statement_results: Vec::new(),
        };
        let outcome = schema_migration_outcome(&result);
        assert!(outcome.success);
        assert!(!outcome.cancelled);
        assert_eq!(outcome.committed, 4);
        assert_eq!(outcome.rollback_outcome, "notRequired");
        assert_eq!(outcome.result["operation"], "schemaDiff");
        assert_eq!(outcome.result["status"], "committed");
        assert_eq!(outcome.result["statements"], 6);
    }

    #[test]
    fn an_unknown_schema_diff_keeps_its_rollback_outcome() {
        let result = SchemaDiffDeployResult {
            status: DeployStatus::Unknown,
            executed_count: 2,
            statement_count: 3,
            errors: vec!["boom".into()],
            statement_results: Vec::new(),
        };
        let outcome = schema_migration_outcome(&result);
        assert!(!outcome.success);
        assert_eq!(outcome.rollback_outcome, "unknown");
        assert_eq!(outcome.failed, 1);
        assert_eq!(outcome.result["status"], "unknown");
    }
}
