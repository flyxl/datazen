//! T027 independent audit of the `SchemaDiffPanel` port.
//!
//! Coverage-driven additions on top of `tests/schema_diff_panel.rs`:
//!  - `??` semantics when a preferred field is *defined but empty* (the legacy
//!    alias must NOT be consulted);
//!  - all 10 `schemaDiff.*` keys resolve and never echo as visible text;
//!  - element keys are unique and stable when many rows are rendered;
//!  - horizontal bounds with long unbroken tokens, observed from runtime-measured
//!    element rects (an observation channel independent of the component's own
//!    reported data);
//!  - the deliberate React asymmetry: `targetOnly` suppresses the identical
//!    *panel* line but the clipboard summary has no such special case.

use std::collections::{HashMap, HashSet};

use rune::{col, Cx, El, Env, Runtime};

use crate::components::schema::schema_diff_panel::{
    format_schema_diff_text, ChangedColumnDiffVm, CheckConstraintDiffVm, ColumnDiffVm,
    SchemaDiffPanel, TableOptionsDiffVm, TableOptionsSnapshotVm, TableSchemaDiffVm,
 };
use crate::i18n::{has, t};

fn env(dark: bool) -> Env {
    Env {
        system_dark: dark,
        scale: 1.0,
    }
}

#[derive(Clone)]
struct Probe {
    diff: TableSchemaDiffVm,
    dark: bool,
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

fn has_key<S>(rt: &Runtime<S>, key: &str) -> bool {
    rt.elements()
        .iter()
        .any(|e| e.key.as_deref() == Some(key))
}

fn text_at<S>(rt: &Runtime<S>, key: &str) -> String {
    rt.elements()
        .into_iter()
        .find(|e| e.key.as_deref() == Some(key))
        .unwrap_or_else(|| panic!("{key} is laid out"))
        .text
        .unwrap_or_else(|| panic!("{key} carries text"))
}

/// A diff that uses every one of the ten keys the component asks for.
fn full_diff() -> TableSchemaDiffVm {
    TableSchemaDiffVm {
        table: "audit.all".to_string(),
        target_only: true,
        missing_on_target: Some(vec![ColumnDiffVm::new("add_col", "INT", false, true)]),
        extra_on_target: Some(vec![ColumnDiffVm::new("drop_col", "TEXT", true, false)]),
        changed: vec![ChangedColumnDiffVm {
            name: "changed_col".to_string(),
            source: ColumnDiffVm::new("changed_col", "INT", true, false),
            target: ColumnDiffVm::new("changed_col", "BIGINT", false, false),
            changes: vec!["type".to_string()],
        }],
        missing_check_constraints: Some(vec![CheckConstraintDiffVm {
            name: "ck_add".to_string(),
            expression: "x > 0".to_string(),
        }]),
        extra_check_constraints: Some(vec![CheckConstraintDiffVm {
            name: "ck_drop".to_string(),
            expression: "y > 0".to_string(),
        }]),
        table_options: Some(TableOptionsDiffVm {
            source: TableOptionsSnapshotVm {
                comment: Some("s".to_string()),
                ..Default::default()
            },
            target: TableOptionsSnapshotVm {
                comment: Some("t".to_string()),
                ..Default::default()
            },
            changes: vec!["comment".to_string()],
        }),
        added: vec![ColumnDiffVm::new("legacy_add", "TEXT", true, false)],
        removed: vec![ColumnDiffVm::new("legacy_drop", "TEXT", true, false)],
    }
}

// ---------------------------------------------------------------------------
// 1. Nullish-coalescing edge: a *defined, empty* preferred array wins.
// ---------------------------------------------------------------------------

#[test]
fn defined_empty_preferred_arrays_do_not_fall_back_to_legacy_aliases() {
    let diff = TableSchemaDiffVm {
        table: "edge".to_string(),
        missing_on_target: Some(vec![]), // defined-empty, not absent
        extra_on_target: Some(vec![]),
        added: vec![ColumnDiffVm::new("ghost_add", "TEXT", true, false)],
        removed: vec![ColumnDiffVm::new("ghost_drop", "TEXT", true, false)],
        ..Default::default()
    };
    let rt = run(diff.clone(), true, (960.0, 640.0));

    // React `missingOnTarget ?? added` returns `[]` for a defined `[]`, so the
    // legacy rows must be absent and the identical fallback must appear.
    assert!(!has_key(&rt, "schema-diff-missing-row-ghost_add"));
    assert!(!has_key(&rt, "schema-diff-extra-row-ghost_drop"));
    assert!(has_key(&rt, "schema-diff-identical"));
    assert_eq!(
        format_schema_diff_text(&diff),
        "-- Schema diff: edge\n(identical)"
    );
}

#[test]
fn a_nonempty_legacy_alias_is_ignored_when_preferred_is_nonempty() {
    let diff = TableSchemaDiffVm {
        table: "prefer".to_string(),
        missing_on_target: Some(vec![ColumnDiffVm::new("real", "INT", true, false)]),
        added: vec![ColumnDiffVm::new("ghost", "TEXT", true, false)],
        ..Default::default()
    };
    let rt = run(diff.clone(), true, (960.0, 640.0));
    assert!(has_key(&rt, "schema-diff-missing-row-real"));
    assert!(!has_key(&rt, "schema-diff-missing-row-ghost"));
    assert_eq!(
        format_schema_diff_text(&diff),
        "-- Schema diff: prefer\n+ real INT"
    );
}

// ---------------------------------------------------------------------------
// 2. i18n: every key resolves to localized, non-key text; no echo in the tree.
// ---------------------------------------------------------------------------

#[test]
fn all_ten_schema_diff_keys_resolve_and_never_echo() {
    const KEYS: [&str; 10] = [
        "schemaDiff.targetOnlyDetail",
        "schemaDiff.missingOnTarget",
        "schemaDiff.extraOnTarget",
        "schemaDiff.colChanged",
        "schemaDiff.source",
        "schemaDiff.target",
        "schemaDiff.checkMissing",
        "schemaDiff.checkExtra",
        "schemaDiff.tableOptions",
        "schemaDiff.schemaIdentical",
    ];
    for key in KEYS {
        assert!(has(key), "{key} must exist in the zh-CN pack");
        assert_ne!(t(key), key, "{key} must not echo the key");
        assert!(!t(key).is_empty(), "{key} must not be empty");
    }

    // A full render must not leak any raw `schemaDiff.*` key as visible text.
    let rt = run(full_diff(), true, (1440.0, 816.0));
    for e in rt.elements() {
        if let Some(txt) = &e.text {
            assert!(
                !txt.contains("schemaDiff."),
                "unresolved key echoed as visible text: {txt}"
            );
        }
    }

    // Spot-check the localized text behind source/target/detail.
    assert!(text_at(&rt, "schema-diff-changed-changed_col-source").starts_with(t("schemaDiff.source")));
    assert!(text_at(&rt, "schema-diff-changed-changed_col-target").starts_with(t("schemaDiff.target")));
    assert_eq!(
        text_at(&rt, "schema-diff-target-only-detail"),
        t("schemaDiff.targetOnlyDetail")
    );
}

// ---------------------------------------------------------------------------
// 3. Element keys are unique and stable when many rows are rendered.
// ---------------------------------------------------------------------------

#[test]
fn repeated_rows_get_unique_and_stable_keys() {
    let mut diff = full_diff();
    diff.target_only = false;
    diff.missing_on_target = Some(vec![
        ColumnDiffVm::new("m1", "INT", true, false),
        ColumnDiffVm::new("m2", "INT", false, false),
        ColumnDiffVm::new("m3", "INT", false, true),
    ]);
    diff.extra_on_target = Some(vec![
        ColumnDiffVm::new("x1", "TEXT", true, false),
        ColumnDiffVm::new("x2", "TEXT", false, false),
    ]);
    diff.changed = vec![
        ChangedColumnDiffVm {
            name: "c1".to_string(),
            source: ColumnDiffVm::new("c1", "INT", true, false),
            target: ColumnDiffVm::new("c1", "BIGINT", false, false),
            changes: vec!["type".to_string()],
        },
        ChangedColumnDiffVm {
            name: "c2".to_string(),
            source: ColumnDiffVm::new("c2", "TEXT", true, false),
            target: ColumnDiffVm::new("c2", "VARCHAR", true, false),
            changes: vec!["type".to_string()],
        },
    ];
    diff.missing_check_constraints = Some(vec![
        CheckConstraintDiffVm {
            name: "k1".to_string(),
            expression: "a>1".to_string(),
        },
        CheckConstraintDiffVm {
            name: "k2".to_string(),
            expression: "b>2".to_string(),
        },
    ]);
    diff.extra_check_constraints = Some(vec![CheckConstraintDiffVm {
        name: "k3".to_string(),
        expression: "c>3".to_string(),
    }]);
    diff.table_options = Some(TableOptionsDiffVm {
        changes: vec![
            "comment".to_string(),
            "engine".to_string(),
            "charset".to_string(),
        ],
        ..Default::default()
    });

    let expected = [
        "schema-diff-missing-row-m1",
        "schema-diff-missing-row-m2",
        "schema-diff-missing-row-m3",
        "schema-diff-extra-row-x1",
        "schema-diff-extra-row-x2",
        "schema-diff-changed-c1",
        "schema-diff-changed-c2",
        "schema-diff-changed-c1-source",
        "schema-diff-changed-c1-target",
        "schema-diff-changed-c2-changes",
        "schema-diff-check-missing-row-k1",
        "schema-diff-check-missing-row-k2",
        "schema-diff-check-extra-row-k3",
        "schema-diff-option-row-comment",
        "schema-diff-option-row-engine",
        "schema-diff-option-row-charset",
    ];

    let rt = run(diff.clone(), true, (960.0, 640.0));
    let mut counts: HashMap<String, usize> = HashMap::new();
    for e in rt.elements() {
        if let Some(k) = &e.key {
            *counts.entry(k.clone()).or_insert(0) += 1;
        }
    }
    for k in expected {
        assert!(has_key(&rt, k), "expected row key {k} to be present");
        assert_eq!(counts.get(k).copied(), Some(1), "key {k} must appear exactly once");
    }
    for (k, n) in &counts {
        assert_eq!(*n, 1, "duplicate element key {k}");
    }

    // Stability: rendering the same diff twice yields the identical key set.
    let rt2 = run(diff, true, (960.0, 640.0));
    let keys1: HashSet<String> = rt.elements().into_iter().filter_map(|e| e.key).collect();
    let keys2: HashSet<String> = rt2.elements().into_iter().filter_map(|e| e.key).collect();
    assert_eq!(keys1, keys2, "element keys must be stable across renders");
}

// ---------------------------------------------------------------------------
// 4. Horizontal bounds with long unbroken tokens (runtime rects, not self-report).
// ---------------------------------------------------------------------------

#[test]
fn long_unbroken_tokens_do_not_overflow_horizontally() {
    let long = "X".repeat(400);
    let diff = TableSchemaDiffVm {
        table: "wide".to_string(),
        target_only: true,
        missing_on_target: Some(vec![ColumnDiffVm::new(
            &long,
            &"Y".repeat(200),
            false,
            true,
        )]),
        extra_on_target: Some(vec![ColumnDiffVm::new(
            &format!("drop_{long}"),
            "TEXT",
            false,
            false,
        )]),
        changed: vec![ChangedColumnDiffVm {
            name: long.clone(),
            source: ColumnDiffVm::new("s", &long, true, false),
            target: ColumnDiffVm::new("t", &long, false, false),
            changes: vec![long.clone()],
        }],
        missing_check_constraints: Some(vec![CheckConstraintDiffVm {
            name: long.clone(),
            expression: long.clone(),
        }]),
        extra_check_constraints: Some(vec![CheckConstraintDiffVm {
            name: long.clone(),
            expression: long.clone(),
        }]),
        table_options: Some(TableOptionsDiffVm {
            source: TableOptionsSnapshotVm {
                comment: Some(long.clone()),
                ..Default::default()
            },
            target: TableOptionsSnapshotVm {
                comment: Some(long.clone()),
                ..Default::default()
            },
            changes: vec!["comment".to_string()],
        }),
        ..Default::default()
    };

    for dark in [true, false] {
        for (w, h) in [(960.0, 640.0), (1440.0, 816.0)] {
            let rt = run(diff.clone(), dark, (w, h));
            let mut out = Vec::new();
            for e in rt.elements() {
                let r = e.rect; // runtime-measured, independent of the component
                if r[0] < -0.5 || r[0] + r[2] > w + 0.5 {
                    out.push(format!("{} {:?}", e.key.unwrap_or(e.path), r));
                }
            }
            assert!(
                out.is_empty(),
                "horizontal overflow dark={dark} {w}x{h}: {} node(s): {out:?}",
                out.len()
            );
            // Keep `h` meaningful so the loop documents both required sizes.
            assert!(h > 0.0);
        }
    }
}

// ---------------------------------------------------------------------------
// 5. React asymmetry: targetOnly affects the panel, not the text summary.
// ---------------------------------------------------------------------------

#[test]
fn target_only_suppresses_identical_panel_but_not_text_summary() {
    let diff = TableSchemaDiffVm {
        table: "only_target".to_string(),
        target_only: true,
        ..Default::default()
    };
    let rt = run(diff.clone(), true, (960.0, 640.0));
    assert!(has_key(&rt, "schema-diff-target-only-detail"));
    assert!(!has_key(&rt, "schema-diff-identical"));
    // `formatSchemaDiffText` never reads `targetOnly`: React emits `(identical)`.
    assert_eq!(
        format_schema_diff_text(&diff),
        "-- Schema diff: only_target\n(identical)"
    );
}
