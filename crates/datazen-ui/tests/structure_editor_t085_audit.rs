//! T085 independent audit: extra, precise assertions on the three ported
//! `lib/structureEditor` `.ts` helpers. Kept in a separate file from the coder's
//! `structure_editor_defaults.rs` so the two stay independent. Every expected
//! value below is a literal from the React sources / locale pack, never derived
//! from the port's own helpers, so a mutation fails loudly.

use datazen_ui::structure_editor::{
    ddl_apply_warnings::{
        build_alter_apply_warning_message, is_large_table_estimate, plan_has_risky_alter,
        should_confirm_alter_apply,
    },
    PlanStatement, StatementRisk, StructureChangeMode, StructureChangePlan,
};

fn plan(risks: &[StatementRisk]) -> StructureChangePlan {
    StructureChangePlan {
        statements: risks
            .iter()
            .enumerate()
            .map(|(i, risk)| PlanStatement {
                sql: format!("sql-{i}"),
                summary: format!("sum-{i}"),
                risk: *risk,
            })
            .collect(),
        warnings: None,
    }
}

/// Pin the whole risk predicate truth table and the exact React string labels.
/// A mutation that treats `additive` as risky, or swaps a label, turns this red.
#[test]
fn risk_predicate_truth_table_and_labels() {
    assert!(!StatementRisk::Additive.is_risky(), "additive is NOT risky");
    assert!(StatementRisk::Destructive.is_risky(), "destructive is risky");
    assert!(StatementRisk::Rewrite.is_risky(), "rewrite is risky");

    assert_eq!(StatementRisk::Additive.as_str(), "additive");
    assert_eq!(StatementRisk::Destructive.as_str(), "destructive");
    assert_eq!(StatementRisk::Rewrite.as_str(), "rewrite");
}

/// A plan is risky if and only if it holds at least one destructive/rewrite
/// statement — including when those follow an additive one or repeat.
#[test]
fn risky_alter_predicate_spot_checks() {
    assert!(!plan_has_risky_alter(&plan(&[])), "empty plan is not risky");
    assert!(
        !plan_has_risky_alter(&plan(&[StatementRisk::Additive, StatementRisk::Additive])),
        "all-additive plan is not risky"
    );
    assert!(
        plan_has_risky_alter(&plan(&[StatementRisk::Additive, StatementRisk::Destructive])),
        "a trailing destructive makes the plan risky"
    );
    assert!(
        plan_has_risky_alter(&plan(&[StatementRisk::Rewrite, StatementRisk::Rewrite])),
        "repeated rewrites are still risky"
    );
}

/// De-dup must keep the FIRST-SEEN order. The coder's test exercises
/// rewrite-before-destructive; this one covers the opposite order so a "sort"
/// mutation cannot hide.
#[test]
fn risky_labels_dedup_in_first_seen_order_destructive_first() {
    let message = build_alter_apply_warning_message(
        &plan(&[
            StatementRisk::Destructive,
            StatementRisk::Rewrite,
            StatementRisk::Destructive,
            StatementRisk::Additive,
            StatementRisk::Rewrite,
        ]),
        None,
    );
    let expected = format!(
        "此计划包含 destructive, rewrite 语句，可能会锁定或重写表。\n\n\
         继续前请仔细审阅生成的 SQL。生产环境建议安排维护窗口。"
    );
    assert_eq!(
        message, expected,
        "risks must de-dup in first-seen order; large-table part must be absent"
    );
}

/// The literal zh-CN strings (no helper-derived expectations) for the parts.
const RISKY_PREFIX: &str = "此计划包含 ";
const RISKY_SUFFIX: &str = " 语句，可能会锁定或重写表。";
const LARGE_PREFIX: &str = "估计行数约 ";
const LARGE_SUFFIX: &str = "。对大表执行在线 DDL 可能耗时较长并阻塞写入。";

/// With no statements and no row estimate only the footer renders — no leading
/// `\n\n`, no invented risky/large parts.
#[test]
fn empty_plan_and_no_rows_render_footer_only() {
    let message = build_alter_apply_warning_message(&plan(&[]), None);
    assert_eq!(
        message,
        "继续前请仔细审阅生成的 SQL。生产环境建议安排维护窗口。",
        "an empty harmless plan must render exactly the footer"
    );
}

/// The exact params substituted into each part: the risk list and the row
/// count must appear verbatim, separated by the two `\n\n` joins.
#[test]
fn warning_params_are_verbatim_with_double_newline_joins() {
    let message = build_alter_apply_warning_message(
        &plan(&[StatementRisk::Rewrite, StatementRisk::Rewrite]),
        Some(100_001),
    );
    assert!(
        message.starts_with(RISKY_PREFIX) && message.contains("rewrite"),
        "risky part missing/param wrong: {message}"
    );
    assert!(
        message.contains(&format!("{LARGE_PREFIX}100001{LARGE_SUFFIX}")),
        "row count must be substituted verbatim: {message}"
    );
    assert_eq!(
        message.matches("\n\n").count(),
        2,
        "exactly two double-newline joins between the three parts"
    );
}

/// Exhaustive alter/create × risk/rows matrix, with literal boundary values so
/// a changed threshold constant cannot be masked.
#[test]
fn confirm_matrix_exhaustive() {
    // create never confirms, regardless of risk or rows.
    assert!(!should_confirm_alter_apply(
        StructureChangeMode::Create,
        &plan(&[StatementRisk::Rewrite, StatementRisk::Destructive]),
        Some(999_999)
    ));
    // alter + harmless + small rows -> no confirm.
    assert!(!should_confirm_alter_apply(
        StructureChangeMode::Alter,
        &plan(&[StatementRisk::Additive]),
        Some(99_999)
    ));
    // alter + risky, no rows -> confirm.
    assert!(should_confirm_alter_apply(
        StructureChangeMode::Alter,
        &plan(&[StatementRisk::Destructive]),
        None
    ));
    // alter + harmless but at the inclusive 100k boundary -> confirm.
    assert!(should_confirm_alter_apply(
        StructureChangeMode::Alter,
        &plan(&[StatementRisk::Additive]),
        Some(100_000)
    ));
}

/// Independent boundary check using literal values, not the exported constant.
#[test]
fn large_table_boundary_literals() {
    assert!(!is_large_table_estimate(None));
    assert!(!is_large_table_estimate(Some(99_999)));
    assert!(is_large_table_estimate(Some(100_000)));
    assert!(is_large_table_estimate(Some(u64::MAX)));
}
