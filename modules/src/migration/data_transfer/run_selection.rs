//! Run-selection semantics for Data Transfer preview/execute orchestration.
//!
//! B2 data_transfer 域刀下沉物（原桌面 `commands/data_transfer/exec.rs` 的
//! 选择/恢复纯规则）：这些函数只看 `TransferJob` / `TransferRunSelection` /
//! 表执行结果——形态中立，桌面命令层与将来 Web 宿主的执行编排爬同一份。
//! 校验错误文本是 IPC 契约（宿主映射成 Validation 错误），下沉时逐字保留；
//! 纯规则测试（原 exec.rs 内联 tests 与桌面 tests.rs 的
//! `resume_checkpoint_binds_the_effective_subselection`）随逻辑归位本模块。

use std::collections::HashSet;

use super::model::{
    TableExecutionResult, TransferExecutionResult, TransferJob, TransferRunSelection, TransferMode,
    WriteMode,
};
use super::plan_store::participating_tables;

/// Validate a client-supplied run selection against the preview plan.
///
/// Returns the wire-contract message on rejection; the host maps it onto its
/// validation error kind.
pub fn validate_selection(
    job: &TransferJob,
    selection: &TransferRunSelection,
) -> Result<(), String> {
    let Some(source_tables) = &selection.source_tables else {
        return Ok(());
    };
    // SQL-file previews may discover their default source table set on the
    // server because the UI has no target snapshot to inspect. Treat an
    // empty client selection as “use that immutable server selection” rather
    // than silently disabling every table. Database targets retain their
    // existing explicit-selection semantics.
    if job.sql_file_target.is_some() && source_tables.is_empty() {
        return Ok(());
    }
    let mut seen = HashSet::new();
    for name in source_tables {
        if name.trim().is_empty() || !seen.insert(name) {
            return Err("transfer selection contains an empty or duplicate source table".into());
        }
        let Some(table) = job.tables.iter().find(|table| table.source_table == *name) else {
            return Err("transfer selection contains a table that was not in the preview plan".into());
        };
        if !table.enabled {
            return Err("transfer selection cannot enable a table disabled in the preview plan".into());
        }
    }
    Ok(())
}

/// Fold the effective selection into the claimed job before execution.
pub fn apply_selection(job: &mut TransferJob, selection: &TransferRunSelection) {
    let Some(source_tables) = &selection.source_tables else {
        return;
    };
    if job.sql_file_target.is_some() && source_tables.is_empty() {
        return;
    }
    for table in &mut job.tables {
        table.enabled = source_tables
            .iter()
            .any(|source_table| source_table == &table.source_table);
    }
}

/// The source tables a run will actually move, after the selection is applied.
pub fn selected_source_tables(
    job: &TransferJob,
    selection: &TransferRunSelection,
) -> Vec<String> {
    job.tables
        .iter()
        .filter(|table| table.enabled)
        .filter(|table| {
            selection.source_tables.as_ref().map_or(true, |selected| {
                selected.iter().any(|name| name == &table.source_table)
            })
        })
        .map(|table| table.source_table.clone())
        .collect()
}

/// The first resumability slice is intentionally narrow. Every completed unit
/// is an existing target table committed in its own transaction, so replaying
/// a resume token can only start an uncommitted table from its source boundary.
pub fn supports_bounded_resume(job: &TransferJob) -> bool {
    job.sql_file_target.is_none()
        && job.mode == TransferMode::Data
        && job.write_mode == WriteMode::Insert
        && participating_tables(job).all(|table| !table.create_new)
}

fn has_unknown_table_outcome(results: &[TableExecutionResult]) -> bool {
    results
        .iter()
        .any(|table| table.outcome == Some(super::model::TableExecutionOutcome::Unknown))
}

pub fn should_stop_after_structure_phase(
    results: &[TableExecutionResult],
    cancelled: bool,
    partial: bool,
    stop_on_error: bool,
) -> bool {
    has_unknown_table_outcome(results) || cancelled || (partial && stop_on_error)
}

pub fn has_unknown_outcome(result: &TransferExecutionResult) -> bool {
    result
        .tables
        .iter()
        .any(|table| table.outcome == Some(super::model::TableExecutionOutcome::Unknown))
}

pub fn completed_tables_from_result(prior: &[String], result: &TransferExecutionResult) -> Vec<String> {
    let mut completed: HashSet<String> = prior.iter().cloned().collect();
    completed.extend(
        result
            .tables
            .iter()
            .filter(|table| table.success)
            .map(|table| table.source_table.clone()),
    );
    let mut completed: Vec<_> = completed.into_iter().collect();
    completed.sort();
    completed
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::migration::data_transfer::model::{
        Endpoint, TableExecutionOutcome, TableMapping, TransferOptions,
    };

    fn plan_job(source_db_session_id: String, target_db_session_id: String) -> TransferJob {
        TransferJob {
            source: Endpoint {
                db_session_id: source_db_session_id,
                database: "app".into(),
                schema: None,
            },
            target: Some(Endpoint {
                db_session_id: target_db_session_id,
                database: "app".into(),
                schema: None,
            }),
            sql_file_target: None,
            mode: TransferMode::Data,
            write_mode: WriteMode::Insert,
            tables: vec![TableMapping::auto("users")],
            options: TransferOptions::default(),
        }
    }

    /// 迁自桌面 `commands/data_transfer/exec.rs` 内联 tests（B2 域刀）。
    #[test]
    fn unknown_structure_outcome_always_stops_before_data() {
        let results = [TableExecutionResult::database(
            "first",
            "first",
            None,
            TableExecutionOutcome::Unknown,
            Some("lost DDL acknowledgement".into()),
        )];
        assert!(should_stop_after_structure_phase(
            &results, false, true, false
        ));
    }

    /// 迁自桌面 `commands/data_transfer/exec.rs` 内联 tests（B2 域刀）。
    #[test]
    fn confirmed_structure_failure_follows_continue_preference() {
        let results = [TableExecutionResult::database(
            "first",
            "first",
            Some(0),
            TableExecutionOutcome::NotStarted,
            Some("preflight rejected table".into()),
        )];
        assert!(!should_stop_after_structure_phase(
            &results, false, true, false
        ));
        assert!(should_stop_after_structure_phase(
            &results, false, true, true
        ));
        assert!(should_stop_after_structure_phase(&[], true, false, false));
    }

    /// 迁自桌面 `commands/data_transfer/exec.rs` 内联 tests（B2 域刀）。
    #[test]
    fn partially_applied_preambles_cannot_create_or_replay_resume_tokens() {
        let mut job = plan_job("source".into(), "target".into());

        assert!(supports_bounded_resume(&job));
        job.write_mode = WriteMode::TruncateInsert;
        assert!(!supports_bounded_resume(&job));
        job.write_mode = WriteMode::DropCreateInsert;
        assert!(!supports_bounded_resume(&job));
        job.write_mode = WriteMode::Insert;
        job.mode = TransferMode::StructureAndData;
        assert!(!supports_bounded_resume(&job));
    }

    /// 迁自桌面 `commands/data_transfer/tests.rs`（B2 域刀：纯逻辑归位）。
    #[test]
    fn resume_checkpoint_binds_the_effective_subselection() {
        let mut job = plan_job("source".into(), "target".into());
        job.tables.push(TableMapping::auto("orders"));
        let selection = TransferRunSelection {
            source_tables: Some(vec!["orders".into()]),
        };
        assert_eq!(
            selected_source_tables(&job, &selection),
            vec!["orders"]
        );
    }
}
