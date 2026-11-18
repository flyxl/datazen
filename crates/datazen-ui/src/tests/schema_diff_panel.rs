//! T027: `SchemaDiffPanel` port — view branches, clipboard summary and bounds.

use rune::{col, Cx, El, Env, Runtime};

use crate::components::schema::schema_diff_panel::{
    format_schema_diff_text, ChangedColumnDiffVm, CheckConstraintDiffVm, ColumnDiffVm,
    SchemaDiffPanel, TableOptionsDiffVm, TableOptionsSnapshotVm, TableSchemaDiffVm,
};
use crate::i18n::t;
use crate::ui::kit::kit;

fn env(dark: bool) -> Env {
    Env {
        system_dark: dark,
        scale: 1.0,
    }
}

fn element<S>(rt: &Runtime<S>, key: &str) -> rune::ElementInfo {
    rt.elements()
        .into_iter()
        .find(|e| e.key.as_deref() == Some(key))
        .unwrap_or_else(|| panic!("{key} is laid out"))
}

fn has_key<S>(rt: &Runtime<S>, key: &str) -> bool {
    rt.elements().iter().any(|e| e.key.as_deref() == Some(key))
}

fn text_at<S>(rt: &Runtime<S>, key: &str) -> String {
    element(rt, key)
        .text
        .unwrap_or_else(|| panic!("{key} carries text"))
}

fn assert_in_window<S>(rt: &Runtime<S>, w: f32, h: f32, label: &str) {
    let mut outside = Vec::new();
    for e in rt.elements() {
        let r = e.rect;
        if r[0] < -0.5 || r[1] < -0.5 || r[0] + r[2] > w + 0.5 || r[1] + r[3] > h + 0.5 {
            outside.push(format!("{} {:?}", e.key.unwrap_or(e.path), r));
        }
    }
    assert!(
        outside.is_empty(),
        "{label}: {} node(s) out of window {w}x{h}: {outside:?}",
        outside.len()
    );
}

// ---------------------------------------------------------------------------
// Probe views
// ---------------------------------------------------------------------------

#[derive(Clone)]
struct Probe {
    diff: TableSchemaDiffVm,
    dark: bool,
}

impl Default for Probe {
    fn default() -> Self {
        Self {
            diff: TableSchemaDiffVm::default(),
            dark: true,
        }
    }
}

fn probe_view(state: &Probe, _cx: &mut Cx<Probe>) -> El {
    col()
        .w_full()
        .h_full()
        .padding(16.0)
        .child(SchemaDiffPanel::new(state.diff.clone()).build(state.dark))
}

fn run(diff: TableSchemaDiffVm, dark: bool, size: (f32, f32)) -> Runtime<Probe> {
    let mut rt = Runtime::new(Probe { diff, dark }, probe_view, size, env(dark));
    rt.frame(0.0);
    rt
}

fn kitchen_sink() -> TableSchemaDiffVm {
    TableSchemaDiffVm {
        table: "public.users".to_string(),
        target_only: false,
        // Preferred fields intentionally differ from the legacy aliases: the
        // panel must use these and ignore `added` / `removed`.
        missing_on_target: Some(vec![
            ColumnDiffVm::new("id", "INTEGER", true, true),
            ColumnDiffVm::new("email", "TEXT", false, false),
        ]),
        extra_on_target: Some(vec![ColumnDiffVm::new(
            "old_col",
            "VARCHAR(20)",
            false,
            false,
        )]),
        added: vec![ColumnDiffVm::new("legacy_added", "TEXT", true, false)],
        removed: vec![ColumnDiffVm::new("legacy_removed", "TEXT", true, false)],
        changed: vec![ChangedColumnDiffVm {
            name: "score".to_string(),
            source: ColumnDiffVm::new("score", "INTEGER", true, false),
            target: ColumnDiffVm::new("score", "BIGINT", false, false),
            changes: vec!["type".to_string(), "nullability".to_string()],
        }],
        missing_check_constraints: Some(vec![CheckConstraintDiffVm {
            name: "ck_age".to_string(),
            expression: "age > 0".to_string(),
        }]),
        extra_check_constraints: Some(vec![CheckConstraintDiffVm {
            name: "ck_status".to_string(),
            expression: "status IN ('a','b')".to_string(),
        }]),
        table_options: Some(TableOptionsDiffVm {
            source: TableOptionsSnapshotVm {
                comment: Some("source note".to_string()),
                engine: Some("InnoDB".to_string()),
                charset: None,
            },
            target: TableOptionsSnapshotVm {
                comment: Some("target note".to_string()),
                engine: Some("MyISAM".to_string()),
                charset: None,
            },
            changes: vec![
                "comment".to_string(),
                "engine".to_string(),
                "charset".to_string(),
            ],
        }),
    }
}

// ---------------------------------------------------------------------------
// View branches
// ---------------------------------------------------------------------------

#[test]
fn target_only_renders_the_danger_tinted_detail() {
    let diff = TableSchemaDiffVm {
        table: "public.gone".to_string(),
        target_only: true,
        ..Default::default()
    };
    let rt = run(diff, true, (960.0, 640.0));
    let detail = element(&rt, "schema-diff-target-only-detail");
    assert_eq!(detail.text.as_deref(), Some(t("schemaDiff.targetOnlyDetail")));
    assert_eq!(detail.text_color, Some(kit(true).c.danger.fg));
    assert_eq!(detail.fill, kit(true).alpha(kit(true).c.danger.fg, 0.1));
    assert!(!has_key(&rt, "schema-diff-identical"), "target-only is not identical");
    for key in [
        "schema-diff-section-missing",
        "schema-diff-section-extra",
        "schema-diff-section-changed",
        "schema-diff-section-check-missing",
        "schema-diff-section-check-extra",
        "schema-diff-section-table-options",
    ] {
        assert!(!has_key(&rt, key), "empty section {key} must not render");
    }
}

#[test]
fn legacy_aliases_feed_the_rows_when_preferred_fields_are_absent() {
    let diff = TableSchemaDiffVm {
        table: "legacy".to_string(),
        added: vec![ColumnDiffVm::new("legacy_added", "TEXT", false, false)],
        removed: vec![ColumnDiffVm::new("legacy_removed", "INT", true, true)],
        ..Default::default()
    };
    let rt = run(diff, true, (960.0, 640.0));
    assert_eq!(
        text_at(&rt, "schema-diff-missing-row-legacy_added"),
        "+ legacy_added (TEXT, NOT NULL)"
    );
    assert_eq!(
        text_at(&rt, "schema-diff-extra-row-legacy_removed"),
        "- legacy_removed (INT, PK)"
    );
    assert_eq!(
        format_schema_diff_text(&TableSchemaDiffVm {
            table: "legacy".to_string(),
            added: vec![ColumnDiffVm::new("legacy_added", "TEXT", false, false)],
            removed: vec![ColumnDiffVm::new("legacy_removed", "INT", true, true)],
            ..Default::default()
        }),
        "-- Schema diff: legacy\n+ legacy_added TEXT NOT NULL\n- legacy_removed INT"
    );
}

#[test]
fn all_sections_render_headers_and_exact_row_text() {
    let rt = run(kitchen_sink(), true, (1440.0, 816.0));
    let k = kit(true);

    // Every section header, with the React color mapping.
    for (key, label_key, color) in [
        ("schema-diff-header-missing", "schemaDiff.missingOnTarget", k.c.success.fg),
        ("schema-diff-header-extra", "schemaDiff.extraOnTarget", k.c.danger.fg),
        ("schema-diff-header-changed", "schemaDiff.colChanged", k.c.warning.fg),
        ("schema-diff-header-check-missing", "schemaDiff.checkMissing", k.c.success.fg),
        ("schema-diff-header-check-extra", "schemaDiff.checkExtra", k.c.danger.fg),
        ("schema-diff-header-table-options", "schemaDiff.tableOptions", k.c.warning.fg),
    ] {
        let el = element(&rt, key);
        assert_eq!(el.text.as_deref(), Some(t(label_key)), "{key} label");
        assert_eq!(el.text_color, Some(color), "{key} color");
    }

    // `+`/`-` rows with every NOT NULL / PK suffix combination, and the
    // preferred fields winning over the legacy aliases.
    assert_eq!(text_at(&rt, "schema-diff-missing-row-id"), "+ id (INTEGER, PK)");
    assert_eq!(text_at(&rt, "schema-diff-missing-row-email"), "+ email (TEXT, NOT NULL)");
    assert_eq!(text_at(&rt, "schema-diff-extra-row-old_col"), "- old_col (VARCHAR(20), NOT NULL)");
    assert!(!has_key(&rt, "schema-diff-missing-row-legacy_added"));
    assert!(!has_key(&rt, "schema-diff-extra-row-legacy_removed"));

    // Changed card: name, source, target (`type` then `nullability` suffixes) and changes.
    assert_eq!(text_at(&rt, "schema-diff-changed-score-name"), "score");
    assert_eq!(text_at(&rt, "schema-diff-changed-score-source"), "源数据库: INTEGER");
    assert_eq!(text_at(&rt, "schema-diff-changed-score-target"), "目标数据库: BIGINT, NOT NULL");
    assert_eq!(text_at(&rt, "schema-diff-changed-score-changes"), "type, nullability");

    // CHECK constraint rows.
    assert_eq!(text_at(&rt, "schema-diff-check-missing-row-ck_age"), "+ ck_age: CHECK (age > 0)");
    assert_eq!(text_at(&rt, "schema-diff-check-extra-row-ck_status"), "- ck_status: CHECK (status IN ('a','b'))");

    // Table options: React renders `target -> source`; null becomes empty text.
    assert_eq!(text_at(&rt, "schema-diff-option-row-comment"), "~ comment: target note -> source note");
    assert_eq!(text_at(&rt, "schema-diff-option-row-engine"), "~ engine: MyISAM -> InnoDB");
    assert_eq!(text_at(&rt, "schema-diff-option-row-charset"), "~ charset:  -> ");
}

#[test]
fn identical_fallback_only_when_everything_is_empty() {
    let empty = TableSchemaDiffVm {
        table: "t".to_string(),
        ..Default::default()
    };
    let rt = run(empty.clone(), true, (960.0, 640.0));
    assert_eq!(text_at(&rt, "schema-diff-identical"), t("schemaDiff.schemaIdentical"));
    for key in [
        "schema-diff-target-only-detail",
        "schema-diff-section-missing",
        "schema-diff-section-extra",
        "schema-diff-section-changed",
        "schema-diff-section-check-missing",
        "schema-diff-section-check-extra",
        "schema-diff-section-table-options",
    ] {
        assert!(!has_key(&rt, key), "no section {key} in the identical case");
    }

    // React tests `!tableOptions`: a present object suppresses the identical
    // line even when its `changes` array is empty.
    let with_options = TableSchemaDiffVm {
        table: "t".to_string(),
        table_options: Some(TableOptionsDiffVm::default()),
        ..Default::default()
    };
    let rt = run(with_options, true, (960.0, 640.0));
    assert!(has_key(&rt, "schema-diff-section-table-options"));
    assert!(!has_key(&rt, "schema-diff-identical"));
}

// ---------------------------------------------------------------------------
// Plain-text clipboard summary
// ---------------------------------------------------------------------------

#[test]
fn format_schema_diff_text_matches_react_exactly() {
    assert_eq!(
        format_schema_diff_text(&kitchen_sink()),
        concat!(
            "-- Schema diff: public.users\n",
            "+ id INTEGER\n",
            "+ email TEXT NOT NULL\n",
            "- old_col VARCHAR(20)\n",
            "~ score: BIGINT -> INTEGER (type, nullability)\n",
            "+ ck_age: CHECK (age > 0)\n",
            "- ck_status: CHECK (status IN ('a','b'))\n",
            "~ table comment: target note -> source note\n",
            "~ table engine: MyISAM -> InnoDB\n",
            "~ table charset:  -> ",
        )
    );
    assert_eq!(
        format_schema_diff_text(&TableSchemaDiffVm {
            table: "t".to_string(),
            ..Default::default()
        }),
        "-- Schema diff: t\n(identical)"
    );
    // No PK suffix in the text summary (React only appends NOT NULL).
    assert_eq!(
        format_schema_diff_text(&TableSchemaDiffVm {
            table: "pk".to_string(),
            added: vec![ColumnDiffVm::new("id", "INTEGER", true, true)],
            ..Default::default()
        }),
        "-- Schema diff: pk\n+ id INTEGER"
    );
}

// ---------------------------------------------------------------------------
// Bounds at both required window sizes
// ---------------------------------------------------------------------------

#[test]
fn the_panel_stays_inside_both_windows() {
    let mut diff = kitchen_sink();
    diff.target_only = true;
    diff.missing_on_target = Some(vec![ColumnDiffVm::new(
        &"very_long_column_name_".repeat(6),
        "DECIMAL(38, 18)",
        false,
        true,
    )]);
    diff.extra_check_constraints = Some(vec![CheckConstraintDiffVm {
        name: "ck_very_long_constraint_name".to_string(),
        expression: format!("status IN ({})", "'value', ".repeat(40) + "'last'"),
    }]);
    diff.changed[0].changes = vec!["type".to_string(), "nullability".to_string(), "default".to_string()];

    for dark in [true, false] {
        for (w, h) in [(960.0, 640.0), (1440.0, 816.0)] {
            let rt = run(diff.clone(), dark, (w, h));
            assert_in_window(&rt, w, h, &format!("schema diff dark={dark} {w}x{h}"));
            assert!(has_key(&rt, "schema-diff-panel"));
            assert!(has_key(&rt, "schema-diff-target-only-detail"));
        }
    }
}
