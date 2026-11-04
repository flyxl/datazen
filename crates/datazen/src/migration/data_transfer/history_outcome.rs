//! 迁移运行历史的回滚口径（Data Transfer 域）。
//!
//! B2 data_transfer 域刀下沉物（原桌面 `commands/data_transfer/mod.rs` 的
//! `transfer_rollback_history_outcome` / `transfer_error_history_outcome`）：
//! 这条规则回答「目标库事务到底回滚了没有」，属于 Data Transfer 域而与
//! workflow 无关。SQL 文件输出不走目标库事务，也没有逐表结果，因此沿用
//! 它既有的运行历史口径。口径测试（原桌面 tests.rs 的
//! `transfer_history_outcome_uses_typed_table_results`）随逻辑归位本模块。

use super::model::{TableExecutionOutcome, TransferExecutionResult};

/// 把一次传输执行的结果映射成迁移运行历史里的 `rollback_outcome`。
pub fn rollback_history_outcome(result: &TransferExecutionResult) -> &'static str {
    // SQL-file output does not use target database transactions or typed
    // per-table outcomes. Preserve its existing run-history contract here.
    if result.tables.iter().all(|table| table.outcome.is_none()) {
        return if result.partial {
            "unknown"
        } else {
            "notRequired"
        };
    }

    if result
        .tables
        .iter()
        .any(|table| table.outcome == Some(TableExecutionOutcome::Unknown))
    {
        "unknown"
    } else if result
        .tables
        .iter()
        .any(|table| table.outcome == Some(TableExecutionOutcome::PartiallyApplied))
    {
        "partiallyApplied"
    } else if result
        .tables
        .iter()
        .any(|table| table.outcome == Some(TableExecutionOutcome::RolledBack))
    {
        "rolledBack"
    } else if result.partial
        && result
            .tables
            .iter()
            .all(|table| table.outcome == Some(TableExecutionOutcome::NotStarted))
    {
        "notStarted"
    } else {
        "notRequired"
    }
}

/// 传输失败时的回滚口径：写入已经开始就只能说「未知」，没开始才敢写
/// 「未开始」。
pub fn error_history_outcome(write_started: bool) -> &'static str {
    if write_started {
        "unknown"
    } else {
        "notStarted"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::migration::data_transfer::model::TableExecutionResult;

    /// 迁自桌面 `commands/data_transfer/tests.rs`（B2 域刀：纯逻辑归位）。
    #[test]
    fn transfer_history_outcome_uses_typed_table_results() {
        let result = |outcome| TransferExecutionResult {
            tables: vec![TableExecutionResult::database(
                "source",
                "target",
                None,
                outcome,
                Some("injected table result".into()),
            )],
            partial: true,
            ..Default::default()
        };

        assert_eq!(
            rollback_history_outcome(&result(TableExecutionOutcome::Unknown)),
            "unknown"
        );
        assert_eq!(
            rollback_history_outcome(&result(TableExecutionOutcome::RolledBack)),
            "rolledBack"
        );
        assert_eq!(
            rollback_history_outcome(&result(TableExecutionOutcome::PartiallyApplied)),
            "partiallyApplied"
        );
        assert_eq!(
            rollback_history_outcome(&result(TableExecutionOutcome::NotStarted)),
            "notStarted"
        );
        assert_eq!(
            rollback_history_outcome(&TransferExecutionResult::default()),
            "notRequired"
        );
        let sql_file_partial = TransferExecutionResult {
            tables: vec![TableExecutionResult {
                source_table: "users".into(),
                target_table: "users".into(),
                rows_inserted: Some(1),
                success: false,
                error: Some("injected SQL-file failure".into()),
                outcome: None,
            }],
            partial: true,
            ..Default::default()
        };
        assert_eq!(
            rollback_history_outcome(&sql_file_partial),
            "unknown",
            "SQL-file history semantics remain outside the typed DB outcomes"
        );
        let mixed = TransferExecutionResult {
            tables: vec![
                TableExecutionResult::database(
                    "committed-before-unknown",
                    "committed-before-unknown",
                    Some(1),
                    TableExecutionOutcome::RolledBack,
                    Some("confirmed rollback".into()),
                ),
                TableExecutionResult::database(
                    "unknown",
                    "unknown",
                    None,
                    TableExecutionOutcome::Unknown,
                    Some("lost acknowledgement".into()),
                ),
            ],
            partial: true,
            ..Default::default()
        };
        assert_eq!(rollback_history_outcome(&mixed), "unknown");
        assert_eq!(error_history_outcome(false), "notStarted");
        assert_eq!(error_history_outcome(true), "unknown");
    }
}
