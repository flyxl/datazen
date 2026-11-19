//! T085: real assertions for the three remaining `lib/structureEditor` `.ts`
//! ports — `draftIds`, `draftDefaults` and `ddlApplyWarnings`. Pure value logic:
//! no Rune tree, no `AppState`, no IPC. Every field/branch comes from the React
//! sources and their vitest suite; assertions carry failure text so a revert is
//! diagnosable without re-reading the port.

use std::collections::HashSet;

use crate::structure_editor::{
    ddl_apply_warnings::{
        build_alter_apply_warning_message, is_large_table_estimate, plan_has_risky_alter,
        should_confirm_alter_apply, LARGE_TABLE_ROW_THRESHOLD,
    },
    draft_defaults::{default_create_columns, empty_column_draft, empty_index_draft},
    draft_ids::{new_column_id, new_index_id},
    PlanStatement, StatementRisk, StructureChangeMode, StructureChangePlan, StructureEditorUiConfig,
};

// --- fixtures -------------------------------------------------------------

fn plan(risks: &[StatementRisk]) -> StructureChangePlan {
    StructureChangePlan {
        statements: risks
            .iter()
            .enumerate()
            .map(|(i, risk)| PlanStatement {
                sql: format!("stmt {i}"),
                summary: format!("summary {i}"),
                risk: *risk,
            })
            .collect(),
        warnings: None,
    }
}

fn ui_config(default_type: &str) -> StructureEditorUiConfig {
    StructureEditorUiConfig {
        enabled: Some(true),
        default_column_type: default_type.to_owned(),
    }
}

// --- draftIds -------------------------------------------------------------

#[test]
fn ids_keep_the_react_prefix_and_six_character_base36_suffix() {
    // React: `col_${Math.random().toString(36).slice(2, 8)}` — prefix + 6 chars.
    for id in [new_column_id(), new_column_id(), new_index_id(), new_index_id()] {
        let is_column = id.starts_with("col_");
        let is_index = id.starts_with("idx_");
        assert!(
            is_column || is_index,
            "id {id:?} lost its col_/idx_ prefix"
        );
        let suffix = &id[4..];
        assert_eq!(suffix.len(), 6, "id {id:?} suffix must be 6 chars");
        assert_eq!(id.len(), 10, "id {id:?} must be prefix + 6 chars");
        assert!(
            suffix.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'z').contains(&b)),
            "id {id:?} suffix left the JS base-36 alphabet"
        );
    }
}

#[test]
fn ids_are_unique_across_both_generators() {
    let mut ids = HashSet::new();
    for _ in 0..100 {
        ids.insert(new_column_id());
        ids.insert(new_index_id());
    }
    assert_eq!(
        ids.len(),
        200,
        "draft ids must not collide across calls (counter adaptation)"
    );
}

// --- draftDefaults --------------------------------------------------------

#[test]
fn empty_column_draft_pins_every_react_field() {
    let draft = empty_column_draft("varchar(255)");

    assert!(
        draft.id.starts_with("col_"),
        "emptyColumnDraft must mint a column id, got {:?}",
        draft.id
    );
    assert_eq!(draft.name, "", "emptyColumnDraft.name must be ''");
    assert_eq!(
        draft.data_type, "varchar(255)",
        "emptyColumnDraft.dataType must be the caller's default type"
    );
    assert!(
        draft.nullable,
        "emptyColumnDraft.nullable must be true (React: nullable: true)"
    );
    assert_eq!(draft.default_value, None, "defaultValue must be null");
    assert_eq!(draft.comment, None, "comment must be null");
    assert!(
        !draft.is_primary_key,
        "isPrimaryKey must be false, not inherited from the seed draft"
    );
    assert!(!draft.is_auto_increment, "isAutoIncrement must be false");
    assert!(!draft.is_unique, "isUnique must be false");
}

#[test]
fn empty_index_draft_pins_every_react_field() {
    let draft = empty_index_draft("btree");

    assert!(
        draft.id.starts_with("idx_"),
        "emptyIndexDraft must mint an index id, got {:?}",
        draft.id
    );
    assert_eq!(draft.name, "", "emptyIndexDraft.name must be ''");
    assert!(
        draft.columns.is_empty(),
        "emptyIndexDraft.columns must be []"
    );
    assert!(!draft.is_unique, "isUnique must be false");
    assert!(!draft.is_primary, "isPrimary must be false");
    assert_eq!(
        draft.index_type.as_deref(),
        Some("btree"),
        "indexType must equal the default method (React: indexType: defaultMethod)"
    );
    assert_eq!(draft.include_columns, None, "includeColumns must be absent");
    assert_eq!(draft.filter, None, "filter must be null");
    assert_eq!(draft.comment, None, "comment must be null");
}

#[test]
fn default_create_columns_seeds_exactly_two_columns() {
    let columns = default_create_columns(&ui_config("int"));

    assert_eq!(columns.len(), 2, "defaultCreateColumns must return 2 drafts");
    let first = &columns[0];
    assert_eq!(first.name, "id", "the first seeded column must be named id");
    assert!(
        !first.nullable,
        "the id column must be NOT NULL (React: nullable: false)"
    );
    assert!(
        first.is_primary_key,
        "the id column must be a primary key (React: isPrimaryKey: true)"
    );
    assert_eq!(first.data_type, "int", "seed type must come from uiConfig");
    assert_eq!(first.default_value, None, "seed defaultValue must stay null");
    assert!(!first.is_auto_increment, "seed isAutoIncrement must stay false");
    assert!(!first.is_unique, "seed isUnique must stay false");

    let second = &columns[1];
    assert_eq!(second.name, "", "the second seeded column must be blank");
    assert!(
        second.nullable,
        "the second seeded column must stay nullable"
    );
    assert!(
        !second.is_primary_key,
        "only the first seeded column is a primary key"
    );
    assert_eq!(second.data_type, "int", "second seed type must come from uiConfig");
    assert_ne!(first.id, second.id, "two drafts must not share an id");
}

// --- ddlApplyWarnings -----------------------------------------------------

#[test]
fn plan_has_risky_alter_detects_destructive_and_rewrite_only() {
    assert_eq!(
        LARGE_TABLE_ROW_THRESHOLD, 100_000,
        "the online-DDL threshold must stay at 100k rows"
    );
    assert!(
        !plan_has_risky_alter(&plan(&[StatementRisk::Additive])),
        "an additive-only plan must not be risky"
    );
    assert!(
        plan_has_risky_alter(&plan(&[StatementRisk::Rewrite])),
        "a rewrite statement must be risky"
    );
    assert!(
        plan_has_risky_alter(&plan(&[StatementRisk::Destructive])),
        "a destructive statement must be risky"
    );
    assert!(
        plan_has_risky_alter(&plan(&[
            StatementRisk::Additive,
            StatementRisk::Rewrite,
        ])),
        "a mixed plan with one rewrite must be risky"
    );
}

#[test]
fn large_table_estimate_is_inclusive_and_null_is_false() {
    assert!(
        !is_large_table_estimate(None),
        "null/undefined rows must not warn"
    );
    assert!(
        !is_large_table_estimate(Some(99_999)),
        "99,999 is below the 100k threshold"
    );
    assert!(
        is_large_table_estimate(Some(100_000)),
        "the 100k threshold is inclusive"
    );
    assert!(
        is_large_table_estimate(Some(150_000)),
        "anything above the threshold must warn"
    );
}

#[test]
fn confirm_is_alter_only_and_accepts_either_risk_source() {
    assert!(
        !should_confirm_alter_apply(
            StructureChangeMode::Create,
            &plan(&[StatementRisk::Rewrite]),
            None
        ),
        "create mode must never ask for confirmation"
    );
    assert!(
        !should_confirm_alter_apply(
            StructureChangeMode::Alter,
            &plan(&[StatementRisk::Additive]),
            Some(50_000)
        ),
        "a harmless small alter must not ask for confirmation"
    );
    assert!(
        should_confirm_alter_apply(
            StructureChangeMode::Alter,
            &plan(&[StatementRisk::Rewrite]),
            None
        ),
        "an alter with a rewrite must ask for confirmation"
    );
    assert!(
        should_confirm_alter_apply(
            StructureChangeMode::Alter,
            &plan(&[StatementRisk::Additive]),
            Some(200_000)
        ),
        "a large-table alter must ask for confirmation even without risky SQL"
    );
}

#[test]
fn the_three_warning_keys_are_real_locale_entries() {
    // Guard against inventing i18n keys: these must exist in the zh-CN pack.
    for key in [
        "structEditor.ddlWarn.risky",
        "structEditor.ddlWarn.largeTable",
        "structEditor.ddlWarn.footer",
    ] {
        assert!(crate::i18n::has(key), "invented i18n key {key:?}");
    }
}

#[test]
fn warning_message_pins_exact_params_and_double_newline_joins() {
    // Independent expected value: the literal zh-CN strings with exact params,
    // not derived from the i18n table (so a wrong param fails loudly).
    let expected = concat!(
        "此计划包含 rewrite, destructive 语句，可能会锁定或重写表。",
        "\n\n",
        "估计行数约 150000。对大表执行在线 DDL 可能耗时较长并阻塞写入。",
        "\n\n",
        "继续前请仔细审阅生成的 SQL。生产环境建议安排维护窗口。",
    );
    let message = build_alter_apply_warning_message(
        &plan(&[
            StatementRisk::Rewrite,
            StatementRisk::Destructive,
            StatementRisk::Rewrite,
            StatementRisk::Additive,
        ]),
        Some(150_000),
    );

    assert_eq!(
        message, expected,
        "risky labels must be de-duplicated in first-seen order and parts joined with \\n\\n"
    );
}

#[test]
fn warning_message_includes_only_the_triggered_parts() {
    // No risk and no large table: the footer is the only part (no leading join).
    let footer = "继续前请仔细审阅生成的 SQL。生产环境建议安排维护窗口。";
    let bare = build_alter_apply_warning_message(&plan(&[StatementRisk::Additive]), Some(10));
    assert_eq!(bare, footer, "a harmless small plan must render only the footer");

    // Risky only: risky part + footer, no large-table part.
    let risky = build_alter_apply_warning_message(
        &plan(&[StatementRisk::Destructive, StatementRisk::Destructive]),
        None,
    );
    assert_eq!(
        risky,
        format!(
            "此计划包含 destructive 语句，可能会锁定或重写表。\n\n{footer}"
        ),
        "repeated risks must collapse to one label and skip the large-table part"
    );

    // Large table only: no risky part.
    let large = build_alter_apply_warning_message(
        &plan(&[StatementRisk::Additive]),
        Some(100_000),
    );
    assert_eq!(
        large,
        format!("估计行数约 100000。对大表执行在线 DDL 可能耗时较长并阻塞写入。\n\n{footer}"),
        "the large-table part must appear at the inclusive threshold with no risky part"
    );
}
