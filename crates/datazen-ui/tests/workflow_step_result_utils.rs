//! T174 (the tasklist's `windows/workflow/workflowStepResultUtils.tsx` port,
//! segment-2): the step-result helpers — the `StepStatusIcon` paint ladder,
//! the `extractStepColumnNames` branch order, and the
//! `stepToStatementResult` conversion with its two null guards, React's
//! `ColumnInfo` assembly (`dataType || 'text'`, `nullable: true`) and its
//! object/array row modes — plus the status-string parse, the preview
//! projection and the chartability slice the execution-result panel
//! consumes.
//!
//! React ships no test for this file; these probes pin the port against the
//! 59-line source directly. The panel's own consumption slice stays pinned in
//! `workflow_execution_panel.rs`.

use serde_json::{json, Value};

use datazen_ui::ui::kit::kit;
use datazen_ui::views::{
    extract_step_column_names, is_chartable_preview, step_to_statement_preview,
    step_to_statement_result, ColumnInfo, StatusInk, StepColumn, StepExecutionResult,
    StatementPreview, WorkflowStepStatus,
};

/// The probe's step: a query step carrying the raw result JSON the host's
/// engine would send (React's `StepExecutionResult.result`), the step's own
/// clock, and nothing else.
fn step(result: Option<Value>) -> StepExecutionResult {
    StepExecutionResult {
        step_id: "step".to_owned(),
        step_type: Some("query".to_owned()),
        status: WorkflowStepStatus::Success,
        result,
        execution_time_ms: 7,
        error: None,
        connection_name: None,
        sql_executed: None,
    }
}

// ---- StepStatusIcon: the paint ladder ----------------------------------------

#[test]
fn step_status_icons_paint_the_four_status_ladder_and_the_unknown_falls_back_to_red() {
    // React's `StepStatusIcon`: ✓ green, ⏭ yellow, ⏱ yellow, and everything
    // else ✗ red — one glyph and one ink per rung.
    let ladder = [
        (WorkflowStepStatus::Success, "✓", StatusInk::Success),
        (WorkflowStepStatus::Skipped, "⏭", StatusInk::Warning),
        (WorkflowStepStatus::TimedOut, "⏱", StatusInk::Warning),
        (WorkflowStepStatus::Failed, "✗", StatusInk::Danger),
    ];
    for (status, glyph, ink) in ladder {
        assert_eq!(status.icon_glyph(), glyph, "{status:?} renders React's glyph");
        assert_eq!(status.icon_ink(), ink, "{status:?} paints React's ink");
    }
    // An unnamed status parses into the failure bucket and paints its ladder.
    let unknown = WorkflowStepStatus::from_str("bogus");
    assert_eq!(unknown.icon_glyph(), "✗");
    assert_eq!(unknown.icon_ink(), StatusInk::Danger);

    // The ink resolves through the kit's semantic palette — the very tokens
    // the execution-result panel paints its own ✓/✗ with, in both
    // appearances; no raw oklch literal enters the port.
    for dark in [true, false] {
        let palette = &kit(dark).c;
        assert_eq!(
            StatusInk::Success.color(dark),
            palette.success.fg,
            "green maps onto the palette's success token"
        );
        assert_eq!(
            StatusInk::Warning.color(dark),
            palette.warning.fg,
            "yellow maps onto the palette's warning token"
        );
        assert_eq!(
            StatusInk::Danger.color(dark),
            palette.danger.fg,
            "red maps onto the palette's danger token"
        );
    }
    // The three rungs are distinct colours in both appearances — a real
    // ladder, not three names for one ink.
    for dark in [true, false] {
        let inks = [
            StatusInk::Success.color(dark),
            StatusInk::Warning.color(dark),
            StatusInk::Danger.color(dark),
        ];
        for i in 0..inks.len() {
            for j in (i + 1)..inks.len() {
                assert_ne!(inks[i], inks[j], "dark={dark}: rungs {i} and {j} differ");
            }
        }
    }
}

#[test]
fn the_status_ladder_parses_the_four_contract_strings() {
    assert_eq!(
        WorkflowStepStatus::from_str("success"),
        WorkflowStepStatus::Success
    );
    assert_eq!(
        WorkflowStepStatus::from_str("failed"),
        WorkflowStepStatus::Failed
    );
    assert_eq!(
        WorkflowStepStatus::from_str("skipped"),
        WorkflowStepStatus::Skipped
    );
    assert_eq!(
        WorkflowStepStatus::from_str("timed_out"),
        WorkflowStepStatus::TimedOut
    );
    // Anything the contract does not name is the failure bucket — the same
    // bucketing `StepStatusIcon`'s red ✗ applies to unknown statuses.
    assert_eq!(
        WorkflowStepStatus::from_str("bogus"),
        WorkflowStepStatus::Failed
    );
    assert_eq!(WorkflowStepStatus::from_str(""), WorkflowStepStatus::Failed);
}

// ---- extractStepColumnNames: the branch order --------------------------------

#[test]
fn extract_step_column_names_prefers_declared_columns_then_row_keys_then_nothing() {
    // Branch one: a non-empty columns array wins — dataType and all — even
    // when the rows carry more keys than the declaration names.
    let declared = json!({
        "columns": [
            { "name": "a", "dataType": "integer" },
            { "name": "b" }
        ],
        "rows": [{ "a": 1, "b": 2, "c": 3 }]
    });
    assert_eq!(
        extract_step_column_names(&declared),
        vec![
            StepColumn {
                name: "a".to_owned(),
                data_type: Some("integer".to_owned())
            },
            StepColumn {
                name: "b".to_owned(),
                data_type: None
            },
        ]
    );
    // An empty columns array is not a declaration (React's `cols.length > 0`):
    // the first row object's keys take over.
    let undeclared = json!({ "columns": [], "rows": [{ "b": 2, "a": 1 }] });
    let cols = extract_step_column_names(&undeclared);
    assert_eq!(cols.len(), 2);
    assert!(cols.iter().any(|c| c.name == "a" && c.data_type.is_none()));
    assert!(cols.iter().any(|c| c.name == "b"));
    // Branch three: nothing to extract.
    assert!(extract_step_column_names(&json!({})).is_empty());
    assert!(extract_step_column_names(&json!({ "rows": [] })).is_empty());
    // Array-shaped rows have no object keys to fall back to.
    assert!(extract_step_column_names(&json!({ "rows": [[1, 2]] })).is_empty());
}

// ---- stepToStatementResult: the conversion -----------------------------------

#[test]
fn step_to_statement_result_returns_null_without_rows_or_without_columns() {
    // React's first null: `!r?.rows` — no result at all, or no rows array.
    assert!(step_to_statement_result(&step(None)).is_none());
    assert!(step_to_statement_result(&step(Some(json!({})))).is_none());
    assert!(
        step_to_statement_result(&step(Some(json!({ "columns": [{ "name": "a" }] }))))
            .is_none()
    );
    // React's second null: nothing extractable as columns (empty rows carry
    // no keys to read).
    assert!(step_to_statement_result(&step(Some(json!({ "rows": [] })))).is_none());
    // A scalar row under a declared column set cannot become cells (React's
    // unchecked cast would hand the raw scalar to the grid as a "row"); the
    // port refuses it.
    let scalar_row = json!({ "columns": [{ "name": "a" }], "rows": [5] });
    assert!(step_to_statement_result(&step(Some(scalar_row))).is_none());
}

#[test]
fn step_to_statement_result_assembles_the_react_column_defaults_sql_and_timing() {
    let mut with_sql = step(Some(json!({
        "columns": [
            { "name": "a", "dataType": "integer" },
            { "name": "b" }
        ],
        "rows": [{ "a": 1, "b": "x" }],
        "execution_time_ms": 42
    })));
    with_sql.sql_executed = Some("SELECT a, b".to_owned());
    let statement = step_to_statement_result(&with_sql).expect("the statement converts");
    assert_eq!(statement.sql, "SELECT a, b");
    assert_eq!(
        statement.columns,
        vec![
            ColumnInfo {
                name: "a".to_owned(),
                data_type: "integer".to_owned(),
                nullable: true
            },
            // React's default typing: `c.dataType || 'text'`, nullable pinned.
            ColumnInfo {
                name: "b".to_owned(),
                data_type: "text".to_owned(),
                nullable: true
            },
        ]
    );
    assert_eq!(statement.execution_time_ms, 42);

    // Without the result's own timing and SQL, the step's clock and the ''
    // default stand in (React: `?? step.executionTimeMs` / `?? ''`).
    let bare = step(Some(json!({
        "columns": [{ "name": "a" }],
        "rows": [{ "a": 1 }]
    })));
    let statement = step_to_statement_result(&bare).expect("the statement converts");
    assert_eq!(statement.sql, "");
    assert_eq!(statement.execution_time_ms, 7);

    // A timing the u64 field cannot hold honestly (a fraction) falls back
    // to the step's own clock rather than lying about the type.
    let fractional = step(Some(json!({
        "columns": [{ "name": "a" }],
        "rows": [{ "a": 1 }],
        "execution_time_ms": 0.5
    })));
    let statement = step_to_statement_result(&fractional).expect("the statement converts");
    assert_eq!(statement.execution_time_ms, 7);
}

#[test]
fn step_to_statement_result_maps_object_rows_per_column_and_passes_arrays_through() {
    // Object mode: the first row is an object, so every row is read per
    // column — a missing key is a null cell, an extra key is ignored, and a
    // non-object row maps to nulls (React's property access there yields
    // undefined → null).
    let object_mode = step(Some(json!({
        "columns": [{ "name": "a" }, { "name": "b" }],
        "rows": [
            { "a": 1, "b": "x", "extra": 9 },
            { "a": 2 },
            [7, 8]
        ]
    })));
    let statement = step_to_statement_result(&object_mode).expect("the statement converts");
    assert_eq!(
        statement.rows,
        vec![
            vec![Some(json!(1)), Some(json!("x"))],
            vec![Some(json!(2)), None],
            vec![None, None],
        ]
    );

    // Pass-through mode: the first row is an array, so rows keep their own
    // shape and length — including rows shorter than the column set, the way
    // React hands `rawRows` to the grid unchanged.
    let array_mode = step(Some(json!({
        "columns": [{ "name": "a" }, { "name": "b" }],
        "rows": [[1, "x"], [2]]
    })));
    let statement = step_to_statement_result(&array_mode).expect("the statement converts");
    assert_eq!(
        statement.rows,
        vec![
            vec![Some(json!(1)), Some(json!("x"))],
            vec![Some(json!(2))],
        ]
    );

    // The empty-rows branch: `[]` is truthy in React, so declared columns
    // still assemble a zero-row statement result.
    let empty_rows = step(Some(json!({ "columns": [{ "name": "a" }], "rows": [] })));
    let statement = step_to_statement_result(&empty_rows).expect("the statement converts");
    assert!(statement.rows.is_empty());
    assert_eq!(statement.columns.len(), 1);
}

// ---- the preview the panel consumes -------------------------------------------

#[test]
fn the_preview_projects_the_statement_result_with_the_hosts_row_count() {
    // The preview is the panel's display slice of the statement result: the
    // host's `rows_count` wins when it reported one (React's panel reads it
    // off the raw result the same way).
    let counted = step(Some(json!({
        "columns": [{ "name": "a", "dataType": "integer" }],
        "rows": [{ "a": 1 }, { "a": 2 }],
        "rows_count": 57
    })));
    let preview = step_to_statement_preview(&counted).expect("the preview converts");
    assert_eq!(preview.rows_count, 57);
    assert_eq!(preview.rows.len(), 2);
    assert_eq!(
        preview.columns,
        vec![StepColumn {
            name: "a".to_owned(),
            data_type: Some("integer".to_owned())
        }]
    );
    // Without a host count the preview counts the rows it actually carries,
    // and an undeclared column arrives re-typed with React's 'text' default.
    let uncounted = step(Some(json!({ "columns": [{ "name": "a" }], "rows": [{ "a": 1 }] })));
    let preview = step_to_statement_preview(&uncounted).expect("the preview converts");
    assert_eq!(preview.rows_count, 1);
    assert_eq!(
        preview.columns,
        vec![StepColumn {
            name: "a".to_owned(),
            data_type: Some("text".to_owned())
        }]
    );
}

// ---- T174 audit: the pins the mutation round added -------------------------
//
// The audit's mutation round found the chartability slice entirely unpinned
// (no probe above reached `is_chartable_preview`), plus three narrower gaps:
// the second null guard's rows-present leg, the nameless-column drop, and
// React's falsy `||` on the default column type. These pins close them.

#[test]
fn the_second_null_guard_holds_even_when_rows_are_present() {
    // React's second null: nothing extractable as columns converts to
    // nothing — even when the result carries rows. Array-shaped or scalar
    // rows have no object keys to extract, so the statement result is null
    // despite `rows` being truthy (React's `!r?.rows` passes them through).
    assert!(step_to_statement_result(&step(Some(json!({ "rows": [[1, 2]] })))).is_none());
    assert!(step_to_statement_result(&step(Some(json!({ "rows": [5] })))).is_none());
}

#[test]
fn nameless_column_entries_are_dropped_and_the_row_keys_take_over() {
    // A `columns` entry without a string `name` is dropped rather than kept
    // as a nameless column (React would hand `{ name: undefined }` on); when
    // that empties the set, the row-key fallback takes over exactly like
    // React's `cols.length > 0` check.
    let nameless = json!({
        "columns": [{ "dataType": "integer" }],
        "rows": [{ "a": 1 }]
    });
    assert_eq!(
        extract_step_column_names(&nameless),
        vec![StepColumn {
            name: "a".to_owned(),
            data_type: None
        }]
    );
    // A non-string `name` is just as nameless.
    let mixed = json!({
        "columns": [{ "name": "a" }, { "name": 5 }],
        "rows": [{ "a": 1 }]
    });
    assert_eq!(
        extract_step_column_names(&mixed),
        vec![StepColumn {
            name: "a".to_owned(),
            data_type: None
        }]
    );
}

#[test]
fn an_empty_data_type_string_defaults_like_reacts_falsy_or() {
    // React's `c.dataType || 'text'` is falsy semantics: an empty string
    // defaults exactly like an absent one, at the ColumnInfo assembly.
    let declared_empty = step(Some(json!({
        "columns": [{ "name": "a", "dataType": "" }],
        "rows": [{ "a": 1 }]
    })));
    let statement = step_to_statement_result(&declared_empty).expect("the statement converts");
    assert_eq!(
        statement.columns,
        vec![ColumnInfo {
            name: "a".to_owned(),
            data_type: "text".to_owned(),
            nullable: true
        }]
    );
    // The preview re-types off the defaulted statement column.
    let preview = step_to_statement_preview(&declared_empty).expect("the preview converts");
    assert_eq!(
        preview.columns,
        vec![StepColumn {
            name: "a".to_owned(),
            data_type: Some("text".to_owned())
        }]
    );
}

#[test]
fn chartability_infers_the_declared_and_sampled_ladder() {
    // The chartability slice of React's `inferFieldType`: the declared
    // keyword table first, the boolean and datetime families that outrank
    // the sample checks, then the sampled values themselves.
    let declared = |dt: &str| -> StatementPreview {
        step_to_statement_preview(&step(Some(json!({
            // Categorical samples: only the declaration can make the column
            // numeric.
            "columns": [{ "name": "a", "dataType": dt }],
            "rows": [{ "a": "x" }]
        }))))
        .expect("the preview converts")
    };
    // The declared numeric keywords — every subsumption-free word of
    // React's regex (`smallint`/`bigint` ride in on `int`'s substring).
    for dt in [
        "int", "serial", "double", "numeric", "decimal", "real", "float", "money", "bigint",
    ] {
        assert!(
            is_chartable_preview(&declared(dt)),
            "a declared `{dt}` column is numeric, so the result is chartable"
        );
    }
    // The boolean and datetime families outrank the sample checks — numeric
    // samples included, which is what distinguishes the family branch from
    // the sampled inference below.
    let declared_numeric = |dt: &str| -> StatementPreview {
        step_to_statement_preview(&step(Some(json!({
            "columns": [{ "name": "a", "dataType": dt }],
            "rows": [{ "a": 1 }]
        }))))
        .expect("the preview converts")
    };
    assert!(!is_chartable_preview(&declared_numeric("boolean")));
    assert!(!is_chartable_preview(&declared_numeric("timestamp")));
    assert!(!is_chartable_preview(&declared_numeric("date")));
    // `time` on its own: a keyword the family shares with `timestamp`'s
    // substring, pinned so its deletion cannot ride by unnoticed.
    assert!(!is_chartable_preview(&declared_numeric("time")));

    // Sampled inference (no declaration, so the row keys name the column
    // and React's 'text' default types it): numbers and numeric-like
    // strings are numeric...
    let sampled = |values: Vec<Value>| -> StatementPreview {
        let rows: Vec<Value> = values
            .into_iter()
            .map(|v| json!({ "a": v }))
            .collect();
        step_to_statement_preview(&step(Some(json!({ "rows": rows }))))
            .expect("the preview converts")
    };
    assert!(is_chartable_preview(&sampled(vec![json!(1), json!(2.5)])));
    assert!(is_chartable_preview(&sampled(vec![json!("3.5"), json!(" -7 ")])));
    // ...and the non-numeric families: categorical strings...
    assert!(!is_chartable_preview(&sampled(vec![json!("x"), json!("y")])));
    // ...booleans (React infers `boolean`)...
    assert!(!is_chartable_preview(&sampled(vec![json!(true), json!(false)])));
    // ...empty strings (React's `s !== ''` guard; `Number('')` would be 0)...
    assert!(!is_chartable_preview(&sampled(vec![json!("")])));
    // ...and `NaN` sentinels — React's `Number('NaN')` is NaN, so the
    // string is not numeric-like.
    assert!(!is_chartable_preview(&sampled(vec![json!("NaN")])));
    // Nulls are not samples: a nullable numeric column stays numeric, and
    // an all-null column is `unknown`, not vacuously numeric.
    assert!(is_chartable_preview(&sampled(vec![json!(null), json!(3)])));
    assert!(!is_chartable_preview(&sampled(vec![json!(null)])));
    // No rows at all: not chartable (React's rows.length === 0 gate), even
    // with a numeric declaration.
    let empty_rows = step(Some(json!({
        "columns": [{ "name": "a", "dataType": "integer" }],
        "rows": []
    })));
    let preview = step_to_statement_preview(&empty_rows).expect("the preview converts");
    assert!(!is_chartable_preview(&preview), "no rows → not chartable");
}

#[test]
fn chartability_samples_only_the_first_fifty_rows() {
    // React's sample window: `Math.min(result.rows.length, 50)` — row 50 is
    // inside it, row 51 is not.
    let rows = |numeric: usize| -> Vec<Value> {
        (1..=numeric).map(|i| json!({ "a": i })).collect()
    };
    // 50 numeric rows + the categorical 51st: the window never sees the
    // tail, so the column stays numeric and the result chartable.
    let mut beyond_rows = rows(50);
    beyond_rows.push(json!({ "a": "x" }));
    let preview = step_to_statement_preview(&step(Some(json!({ "rows": beyond_rows }))))
        .expect("the preview converts");
    assert!(
        is_chartable_preview(&preview),
        "the 51st row is outside React's 50-row sample window"
    );
    // 49 numeric rows + the categorical 50th: the window sees it, so the
    // column is not numeric and the result not chartable.
    let mut inside_rows = rows(49);
    inside_rows.push(json!({ "a": "x" }));
    let preview = step_to_statement_preview(&step(Some(json!({ "rows": inside_rows }))))
        .expect("the preview converts");
    assert!(
        !is_chartable_preview(&preview),
        "the 50th row is inside React's 50-row sample window"
    );
}
