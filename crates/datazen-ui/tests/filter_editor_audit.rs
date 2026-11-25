//! T106 independent audit (tester): fresh vectors for the FilterEditor
//! shell — different columns, different condition shapes, and
//! observation channels that do not reuse the coder's fixtures.
//!
//! The audit never edits the implementation; it only measures it. The
//! mutation round (this session's evidence) temporarily broke the
//! implementation and these probes went red before it was restored.

use rune::{col, Cx, El, Env, Runtime, TextEdit};

use datazen_ui::components::filter::{FilterCondition, FilterConnector, FilterValue};
use datazen_ui::components::filter_editor::{
    FilterColumn, FilterEditor, FilterEditorState, FilterPanelMode,
};
use datazen_ui::i18n::t;

const MIN: (f32, f32) = (960.0, 640.0);

/// A fresh host: a uuid column and an untamed driver string, so the
/// operator tables resolve differently than the coder's text/number pair.
fn audit_columns() -> Vec<FilterColumn> {
    vec![
        FilterColumn {
            name: "email".to_owned(),
            column_type: Some("uuid".to_owned()),
        },
        FilterColumn {
            name: "raw_blob".to_owned(),
            column_type: Some("driver-specific-weird-type".to_owned()),
        },
        FilterColumn {
            name: "deleted_at".to_owned(),
            column_type: None,
        },
    ]
}

/// One condition with the no-value shape (`isNull`).
fn null_cond(column: &str) -> FilterCondition {
    FilterCondition::new(column, "isNull", FilterValue::None)
}

#[derive(Default)]
struct Audit {
    open: bool,
    applied: Vec<FilterCondition>,
    draft: Vec<FilterCondition>,
    applied_logic: Option<FilterConnector>,
    draft_logic: Option<FilterConnector>,
    wire_modes: bool,
    mode: Option<FilterPanelMode>,
    applied_raw: Option<String>,
    loading: bool,
    editor_state: FilterEditorState,
    mode_changes: Vec<FilterPanelMode>,

    // The recorders, named apart from the coder's.
    open_calls: Vec<bool>,
    added: Vec<FilterCondition>,
    changed: Vec<(usize, FilterCondition)>,
    removed: Vec<usize>,
    applied_count: u32,
    cleared: u32,
    raw_applied_values: Vec<String>,
    any_applied_values: Vec<String>,
}

fn audit_view(state: &Audit, _cx: &mut Cx<Audit>) -> El {
    let dark = _cx.system_dark();
    let columns = audit_columns();
    let mut editor = FilterEditor::new(
        &columns,
        &state.applied,
        state.applied_logic.unwrap_or(FilterConnector::Or),
        &state.draft,
        state.draft_logic.unwrap_or(FilterConnector::Or),
        state.open,
    )
    .host_state(&state.editor_state)
    .host_mut(|s: &mut Audit| &mut s.editor_state)
    .loading(state.loading)
    .mode(state.mode.unwrap_or(FilterPanelMode::Structured))
    .on_open_change(|s: &mut Audit, open| s.open_calls.push(open))
    .on_change(|s: &mut Audit, index, next| s.changed.push((index, next)))
    .on_add(|s: &mut Audit, condition| s.added.push(condition))
    .on_remove(|s: &mut Audit, index| s.removed.push(index))
    .on_apply(|s: &mut Audit| s.applied_count += 1)
    .on_clear(|s: &mut Audit| s.cleared += 1);
    editor = editor.raw_where(state.applied_raw.as_deref(), None);
    if state.wire_modes {
        editor = editor
            .raw_where_supported(true)
            .on_mode_change(|s: &mut Audit, mode| s.mode_changes.push(mode))
            .on_apply_any_column(|s: &mut Audit, value| s.any_applied_values.push(value))
            .on_apply_raw_where(|s: &mut Audit, value| s.raw_applied_values.push(value));
    }
    let mut page = col().key("audit-page").w_full().h_full().padding(24.0);
    if let Some(el) = editor.build(dark) {
        page = page.child(el);
    }
    page
}

fn audit_runtime(state: Audit) -> Runtime<Audit> {
    let mut rt = Runtime::new(
        state,
        audit_view,
        MIN,
        Env {
            system_dark: true,
            scale: 1.0,
        },
    );
    rt.frame(0.0);
    rt
}

fn audit_click(rt: &mut Runtime<Audit>, key: &str) {
    let (x, y) = rt.find(key).unwrap_or_else(|| panic!("{key} is laid out"));
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    rt.frame(1.0 / 60.0);
}

fn text_of(rt: &Runtime<Audit>, key: &str) -> Option<String> {
    rt.elements()
        .into_iter()
        .find(|e| e.key.as_deref() == Some(key))
        .and_then(|e| e.text)
}

#[test]
fn audit_summary_uses_the_condition_connector_not_the_request_logic() {
    // Four applied conditions: only the third carries its own connector
    // (OR); the rest follow the applied logic (OR here too, to make the
    // distinction meaningful the audit flips the global to OR and marks
    // the own one AND).
    let applied = vec![
        null_cond("deleted_at"),
        FilterCondition::new("email", "eq", FilterValue::Single("a@b.c".to_owned())),
        FilterCondition::new("raw_blob", "isEmpty", FilterValue::None)
            .connector(Some(FilterConnector::And)),
        FilterCondition::new("age", "gt", FilterValue::Single("1".to_owned())),
    ];
    let columns = audit_columns();
    let editor: FilterEditor<'_, Audit> = FilterEditor::new(
        &columns,
        &applied,
        FilterConnector::Or,
        &[],
        FilterConnector::Or,
        false,
    );
    let summary = editor.summary_text();
    let and = FilterConnector::And.label().to_uppercase();
    let or = FilterConnector::Or.label().to_uppercase();
    assert!(
        summary.contains(&format!(" {and} ")),
        "the third condition joins with its own connector, not the request logic: {summary}"
    );
    // The conditions that follow the request logic join with OR.
    assert!(
        summary.matches(&format!(" {or} ")).count() >= 2,
        "the inherited conditions join with the request logic: {summary}"
    );
    assert!(
        summary.starts_with("deleted_at"),
        "the first condition has no connector word: {summary}"
    );
}

#[test]
fn audit_the_badge_counts_one_channel_at_a_time() {
    // A raw predicate is applied alongside two structured conditions:
    // the badge counts the raw channel only.
    let rt = audit_runtime(Audit {
        open: false,
        applied: vec![
            null_cond("deleted_at"),
            FilterCondition::new("email", "eq", FilterValue::Single("a@b.c".to_owned())),
        ],
        applied_raw: Some("x > 0".to_owned()),
        ..Audit::default()
    });
    assert_eq!(
        text_of(&rt, "badge").as_deref(),
        Some(t("filter.rawBadge")),
        "DBR-04: one applied channel at a time, the badge counts either"
    );
    // With structured conditions applied, React's summary lists them
    // (the raw summary only leads when no condition is applied).
    let summary = text_of(&rt, "summary").unwrap_or_default();
    assert!(
        summary.contains("deleted_at") && summary.contains("email"),
        "the conditions still summarize: {summary}"
    );
    assert!(
        !summary.contains(t("filter.rawAppliedSummary")),
        "the raw summary is reserved for the raw-only state: {summary}"
    );
}

#[test]
fn audit_the_closed_add_opens_then_adds_the_first_column() {
    let mut rt = audit_runtime(Audit {
        open: false,
        applied: vec![null_cond("deleted_at")],
        ..Audit::default()
    });
    audit_click(&mut rt, "filter-add-closed");
    assert_eq!(rt.state.open_calls, vec![true], "the press opens the panel");
    assert_eq!(
        rt.state.added,
        vec![FilterCondition::new(
            "email",
            "eq",
            FilterValue::Single(String::new())
        )],
        "the add-empty default is the first column with its default operator"
    );
}

#[test]
fn audit_apply_clears_the_recorded_expanded_row_and_respects_dirty() {
    // Dirty via a differing draft; the editing row is preset to 1.
    let mut editor_state = FilterEditorState::default();
    editor_state.editing_index = Some(1);
    let mut rt = audit_runtime(Audit {
        open: true,
        applied: vec![null_cond("deleted_at")],
        draft: vec![
            null_cond("deleted_at"),
            FilterCondition::new("email", "eq", FilterValue::Single("a@b.c".to_owned())),
        ],
        editor_state,
        ..Audit::default()
    });
    audit_click(&mut rt, "filter-apply");
    assert_eq!(rt.state.applied_count, 1, "the dirty draft applies");
    assert_eq!(
        rt.state.editor_state.editing_index,
        None,
        "Apply clears the row the editor would expand"
    );
}

#[test]
fn audit_the_incomplete_marker_blocks_apply_and_names_the_column() {
    // A `contains` condition with no value is incomplete.
    let mut rt = audit_runtime(Audit {
        open: true,
        applied: vec![null_cond("deleted_at")],
        draft: vec![
            null_cond("deleted_at"),
            FilterCondition::new("email", "contains", FilterValue::None),
        ],
        ..Audit::default()
    });
    assert!(rt.find("filter-incomplete").is_some(), "the marker is drawn");
    let banner = text_of(&rt, "filter-incomplete-error-text").unwrap_or_default();
    assert!(
        banner.contains("email"),
        "the banner names the blocking column: {banner}"
    );
    audit_click(&mut rt, "filter-apply");
    assert_eq!(rt.state.applied_count, 0, "an incomplete draft cannot apply");
}

#[test]
fn audit_the_reset_rewrites_every_condition_that_carries_a_connector() {
    // Three conditions, two of which carry connectors; the reset issues
    // one onChange per rewritten condition and keeps the rest intact.
    let mut rt = audit_runtime(Audit {
        open: true,
        draft: vec![
            FilterCondition::new("email", "eq", FilterValue::Single("a@b.c".to_owned()))
                .connector(Some(FilterConnector::And)),
            null_cond("deleted_at"),
            FilterCondition::new("raw_blob", "isEmpty", FilterValue::None)
                .connector(Some(FilterConnector::Or)),
        ],
        draft_logic: Some(FilterConnector::Or),
        ..Audit::default()
    });
    audit_click(&mut rt, "filter-connector-reset");
    assert_eq!(
        rt.state.changed.len(),
        2,
        "only the connector-carrying conditions are rewritten"
    );
    let (index, next) = &rt.state.changed[0];
    assert_eq!(*index, 0);
    assert!(
        next.connector.is_none(),
        "the own connector is dropped, the condition shape is kept"
    );
    assert_eq!(next.column, "email");
    assert!(matches!(next.value, FilterValue::Single(ref v) if v == "a@b.c"));
    let (index, next) = &rt.state.changed[1];
    assert_eq!(*index, 2);
    assert_eq!(next.operator, "isEmpty");
}

#[test]
fn audit_a_connector_press_rewrites_only_that_condition() {
    let mut rt = audit_runtime(Audit {
        open: true,
        draft: vec![
            null_cond("deleted_at"),
            FilterCondition::new("email", "eq", FilterValue::Single("a@b.c".to_owned()))
                .connector(Some(FilterConnector::And)),
        ],
        ..Audit::default()
    });
    audit_click(&mut rt, "filter-connector-1-or");
    assert_eq!(rt.state.changed.len(), 1);
    let (index, next) = &rt.state.changed[0];
    assert_eq!(*index, 1);
    assert_eq!(next.connector, Some(FilterConnector::Or));
    assert!(next.column == "email" && next.operator == "eq");
}

#[test]
fn audit_the_raw_pane_applies_only_a_nonblank_predicate() {
    let mut editor_state = FilterEditorState::default();
    editor_state.raw_where_edit = TextEdit::new("  id % 2 = 0  ");
    let mut rt = audit_runtime(Audit {
        open: true,
        wire_modes: true,
        mode: Some(FilterPanelMode::RawSql),
        editor_state,
        ..Audit::default()
    });
    audit_click(&mut rt, "raw-where-apply");
    assert_eq!(
        rt.state.raw_applied_values,
        vec!["  id % 2 = 0  ".to_owned()],
        "the predicate is applied verbatim, whitespace included"
    );

    // A blank predicate cannot apply.
    let mut rt = audit_runtime(Audit {
        open: true,
        wire_modes: true,
        mode: Some(FilterPanelMode::RawSql),
        ..Audit::default()
    });
    audit_click(&mut rt, "raw-where-apply");
    assert!(
        rt.state.raw_applied_values.is_empty(),
        "React disables apply while the predicate is blank"
    );
}

#[test]
fn audit_the_any_column_pane_applies_the_draft_and_reports_the_scope() {
    let mut editor_state = FilterEditorState::default();
    editor_state.any_column_edit = TextEdit::new("berlin");
    let mut rt = audit_runtime(Audit {
        open: true,
        wire_modes: true,
        mode: Some(FilterPanelMode::AnyColumn),
        editor_state,
        ..Audit::default()
    });
    // The counts come through the i18n template: 0 skipped by default.
    let scope = text_of(&rt, "any-column-scope").unwrap_or_default();
    assert!(
        scope.contains('0'),
        "the scope line reports the counts it was given: {scope}"
    );
    audit_click(&mut rt, "any-column-apply");
    assert_eq!(
        rt.state.any_applied_values,
        vec!["berlin".to_owned()],
        "the any-column draft applies with its value"
    );
}

#[test]
fn audit_the_chip_row_records_edit_and_removes_with_the_index() {
    let mut rt = audit_runtime(Audit {
        open: true,
        draft: vec![
            null_cond("deleted_at"),
            FilterCondition::new("email", "eq", FilterValue::Single("a@b.c".to_owned())),
        ],
        ..Audit::default()
    });
    audit_click(&mut rt, "filter-chip-1-edit");
    assert_eq!(
        rt.state.editor_state.editing_index,
        Some(1),
        "the chip's edit records its own row, not the first"
    );
    audit_click(&mut rt, "filter-chip-1-remove");
    assert_eq!(rt.state.removed, vec![1], "the remove carries its index");
}

#[test]
fn audit_the_closed_quiet_panel_and_the_closed_rows_geometry() {
    // Nothing anywhere: the panel does not mount at all.
    let rt = audit_runtime(Audit {
        ..Audit::default()
    });
    assert!(rt.find("filter-editor").is_none());

    // A raw predicate keeps the closed panel up and inside the window.
    let rt = audit_runtime(Audit {
        open: false,
        applied_raw: Some("y > 0".to_owned()),
        ..Audit::default()
    });
    let (width, height) = MIN;
    for element in rt.elements() {
        let [x, y, w, h] = element.rect;
        let key = element.key.clone().unwrap_or_else(|| "<unnamed>".to_owned());
        assert!(
            x >= -0.5 && y >= -0.5 && x + w <= width + 0.5 && y + h <= height + 0.5,
            "{key} rect [{x}, {y}, {w}, {h}] leaves {width}x{height}"
        );
    }
    let header = rt
        .elements()
        .into_iter()
        .find(|e| e.key.as_deref() == Some("filter-header"))
        .expect("the header is laid out");
    let [_, _, _, h] = header.rect;
    assert!((h - 32.0).abs() < 0.5, "the summary row is h-8: {h}");
}

#[test]
fn audit_loading_keeps_every_channel_silent() {
    let mut editor_state = FilterEditorState::default();
    editor_state.raw_where_edit = TextEdit::new("id > 0");
    let mut rt = audit_runtime(Audit {
        open: true,
        loading: true,
        wire_modes: true,
        mode: Some(FilterPanelMode::RawSql),
        applied: vec![null_cond("deleted_at")],
        draft: vec![
            null_cond("deleted_at"),
            FilterCondition::new("email", "eq", FilterValue::Single("a@b.c".to_owned())),
        ],
        editor_state,
        ..Audit::default()
    });
    // The structured pane is not mounted in Raw SQL mode, so its controls
    // (`filter-add`, `filter-apply`, the logic row) are covered by the
    // coder's loading probe in structured mode; this one presses the
    // header, the raw pane and the mode row.
    for key in [
        "filter-summary-toggle",
        "filter-clear",
        "raw-where-apply",
        "filter-mode-structured",
    ] {
        audit_click(&mut rt, key);
    }
    assert!(
        rt.state.open_calls.is_empty()
            && rt.state.added.is_empty()
            && rt.state.cleared == 0
            && rt.state.applied_count == 0
            && rt.state.raw_applied_values.is_empty()
            && rt.state.mode_changes.is_empty()
            && rt.state.any_applied_values.is_empty(),
        "no channel fires while the request is in flight"
    );
}
