//! T106: the filter editor shell (`components/FilterEditor.tsx`) — the
//! collapsed summary row, the mode row, the Any Column / Raw SQL panes,
//! the structured pane's logic row and the chip list — renders its React
//! structure, commits the same actions, and stays inside the two window
//! sizes.

use rune::{col, Cx, El, Env, Runtime, TextEdit};

use crate::components::filter::{FilterCondition, FilterConnector, FilterValue};
use crate::components::filter_editor::{
    default_operator_for_column, FilterColumn, FilterEditor, FilterEditorState, FilterPanelMode,
    SHARED_FILTER_OPERATORS,
};
use crate::i18n::t;
use crate::support::cell_types::CellTypeHints;

const MIN: (f32, f32) = (960.0, 640.0);
const DESIGN: (f32, f32) = (1440.0, 816.0);
const PAD: f32 = 24.0;

/// The probe state: the panel props plus every callback's recorder.
#[derive(Default)]
struct Probe {
    columns: Vec<FilterColumn>,
    applied: Vec<FilterCondition>,
    applied_logic: Option<FilterConnector>,
    draft: Vec<FilterCondition>,
    draft_logic: Option<FilterConnector>,
    /// Closed by default so every test opens it explicitly.
    open: Option<bool>,
    loading: bool,
    apply_error: Option<String>,
    mode: Option<FilterPanelMode>,
    /// Wire the DBR-04 channels: the mode row appears and both panes are
    /// reachable (React renders them only when the caller offers them).
    wire_modes: bool,
    any_searchable: usize,
    any_skipped: usize,
    applied_raw: Option<String>,
    raw_context: Option<String>,
    editor_state: FilterEditorState,
    preview_sql: bool,
    export_filtered: bool,

    // The recorders.
    open_changes: Vec<bool>,
    logic_changes: Vec<FilterConnector>,
    changes: Vec<(usize, FilterCondition)>,
    adds: Vec<FilterCondition>,
    removes: Vec<usize>,
    applies: u32,
    clears: u32,
    previews: u32,
    exports: u32,
    mode_changes: Vec<FilterPanelMode>,
    any_applies: Vec<String>,
    raw_applies: Vec<String>,
}

/// One `column eq value` condition, the shape most lists are made of.
fn cond(column: &str, value: &str) -> FilterCondition {
    FilterCondition::new(column, "eq", FilterValue::Single(value.to_owned()))
}

/// One condition carrying a connector of its own.
fn cond_with_connector(column: &str, value: &str, connector: FilterConnector) -> FilterCondition {
    cond(column, value).connector(Some(connector))
}

/// The default columns: a text `name` and a numeric `age`.
fn default_columns() -> Vec<FilterColumn> {
    vec![
        FilterColumn {
            name: "name".to_owned(),
            column_type: Some("text".to_owned()),
        },
        FilterColumn {
            name: "age".to_owned(),
            column_type: Some("number".to_owned()),
        },
    ]
}

impl Probe {
    fn open_panel(self) -> Self {
        Self {
            open: Some(true),
            ..self
        }
    }
}

fn view(state: &Probe, _cx: &mut Cx<Probe>) -> El {
    let dark = _cx.system_dark();
    let mut editor = FilterEditor::new(
        &state.columns,
        &state.applied,
        state.applied_logic.unwrap_or(FilterConnector::And),
        &state.draft,
        state.draft_logic.unwrap_or(FilterConnector::And),
        state.open.unwrap_or(false),
    )
    .host_state(&state.editor_state)
    .host_mut(|s: &mut Probe| &mut s.editor_state)
    .loading(state.loading)
    .mode(state.mode.unwrap_or(FilterPanelMode::Structured))
    .any_column(state.any_searchable, state.any_skipped);
    if let Some(error) = &state.apply_error {
        editor = editor.apply_error(Some(error));
    }
    editor = editor.raw_where(state.applied_raw.as_deref(), state.raw_context.as_deref());
    if state.wire_modes {
        editor = editor
            .raw_where_supported(true)
            .on_mode_change(|s: &mut Probe, mode| s.mode_changes.push(mode))
            .on_apply_any_column(|s: &mut Probe, value| s.any_applies.push(value))
            .on_apply_raw_where(|s: &mut Probe, value| s.raw_applies.push(value));
    }
    if state.preview_sql {
        editor = editor.on_preview_sql(|s: &mut Probe| s.previews += 1);
    }
    if state.export_filtered {
        editor = editor.on_export_filtered(|s: &mut Probe| s.exports += 1);
    }
    let editor = editor
        .on_open_change(|s: &mut Probe, open| s.open_changes.push(open))
        .on_logic_change(|s: &mut Probe, logic| s.logic_changes.push(logic))
        .on_change(|s: &mut Probe, index, next| s.changes.push((index, next)))
        .on_add(|s: &mut Probe, condition| s.adds.push(condition))
        .on_remove(|s: &mut Probe, index| s.removes.push(index))
        .on_apply(|s: &mut Probe| s.applies += 1)
        .on_clear(|s: &mut Probe| s.clears += 1);
    let mut page = col().key("page").w_full().h_full().padding(PAD).gap(8.0);
    if let Some(el) = editor.build(dark) {
        page = page.child(el);
    }
    page
}

fn runtime(state: Probe, size: (f32, f32), dark: bool) -> Runtime<Probe> {
    let mut rt = Runtime::new(
        state,
        view,
        size,
        Env {
            system_dark: dark,
            scale: 1.0,
        },
    );
    rt.frame(0.0);
    rt
}

fn click(rt: &mut Runtime<Probe>, key: &str) {
    let (x, y) = rt.find(key).unwrap_or_else(|| panic!("{key} is laid out"));
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    rt.frame(1.0 / 60.0);
}

fn element_text(rt: &Runtime<Probe>, key: &str) -> Option<String> {
    rt.elements()
        .into_iter()
        .find(|e| e.key.as_deref() == Some(key))
        .and_then(|e| e.text)
}

#[test]
fn a_closed_quiet_panel_renders_null() {
    let probe = Probe {
        columns: default_columns(),
        ..Probe::default()
    };
    let rt = runtime(probe, DESIGN, true);
    assert!(
        rt.find("filter-editor").is_none(),
        "closed with nothing applied, drafted or raw-applied renders nothing"
    );
}

#[test]
fn a_raw_applied_panel_stays_up_with_the_raw_badge() {
    let probe = Probe {
        columns: default_columns(),
        applied_raw: Some("id > 5".to_owned()),
        ..Probe::default()
    };
    let rt = runtime(probe, DESIGN, true);
    assert!(rt.find("filter-editor").is_some(), "the raw channel is applied");
    assert_eq!(
        element_text(&rt, "badge").as_deref(),
        Some(t("filter.rawBadge")),
        "the badge is the raw one"
    );
    let summary = element_text(&rt, "summary").unwrap_or_default();
    let expected = t("filter.rawAppliedSummary").replacen("{predicate}", "id > 5", 1);
    assert_eq!(summary, expected, "the summary quotes the predicate");
    // The badge counts one channel: the clear button is offered.
    assert!(rt.find("filter-clear").is_some(), "a badge is clearable");
    assert!(
        rt.find("filter-add-closed").is_some(),
        "the closed add button is offered"
    );
}

#[test]
fn the_collapsed_summary_joins_each_condition_with_the_connector_in_effect() {
    let probe = Probe {
        columns: default_columns(),
        applied: vec![
            cond("name", "alice"),
            cond_with_connector("age", "30", FilterConnector::Or),
            cond("city", "berlin"),
        ],
        applied_logic: Some(FilterConnector::And),
        ..Probe::default()
    };
    let rt = runtime(probe, DESIGN, true);
    let summary = element_text(&rt, "summary").unwrap_or_default();
    let and = FilterConnector::And.label().to_uppercase();
    let or = FilterConnector::Or.label().to_uppercase();
    assert!(
        summary.starts_with("name"),
        "the first condition carries no connector word: {summary}"
    );
    assert!(summary.contains(&or), "the own connector is used: {summary}");
    assert!(
        summary.contains(&format!(" {and} ")),
        "the third condition falls back to the applied logic: {summary}"
    );
    assert_eq!(
        element_text(&rt, "badge").as_deref(),
        Some(t("filter.activeCount").replacen("{count}", "3", 1).as_str()),
        "the badge counts the applied conditions"
    );
}

#[test]
fn the_closed_add_adds_the_first_column_with_its_default_operator() {
    // The closed add renders only on a panel that is up at all: React
    // returns null when closed with nothing applied, drafted or raw.
    let mut rt = runtime(
        Probe {
            columns: default_columns(),
            applied: vec![cond("name", "alice")],
            open: Some(false),
            ..Probe::default()
        },
        DESIGN,
        true,
    );
    click(&mut rt, "filter-add-closed");
    assert_eq!(
        rt.state.adds,
        vec![cond("name", "")],
        "React's addEmpty: the first column, its default operator, an empty value"
    );
    assert_eq!(rt.state.open_changes, vec![true], "the closed add also opens");
}

#[test]
fn clear_needs_a_badge_and_fires_once() {
    let probe = Probe {
        columns: default_columns(),
        applied: vec![cond("name", "alice")],
        ..Probe::default()
    };
    let mut rt = runtime(probe, DESIGN, true);
    assert!(rt.find("filter-clear").is_some(), "a badge is clearable");
    click(&mut rt, "filter-clear");
    assert_eq!(rt.state.clears, 1, "pressing clear calls onClear");
}

#[test]
fn a_dirty_draft_marks_the_summary_row() {
    let probe = Probe {
        columns: default_columns(),
        applied: vec![cond("name", "alice")],
        draft: vec![cond("name", "bob")],
        ..Probe::default()
    };
    let rt = runtime(probe, DESIGN, true);
    assert!(
        rt.find("unapplied").is_some(),
        "a draft that differs from the applied list is marked"
    );
}

#[test]
fn the_open_panel_without_the_dbr04_channels_keeps_the_prew_mode_row_hidden() {
    let probe = Probe {
        columns: default_columns(),
        ..Probe::default()
    }
    .open_panel();
    let rt = runtime(probe, DESIGN, true);
    assert!(rt.find("filter-logic-and").is_some(), "the structured pane is the panel");
    assert!(
        rt.find("filter-mode-structured").is_none(),
        "a caller that offers no modes renders the panel it always had"
    );
    assert!(rt.find("filter-raw-sql-pane").is_none(), "no raw pane");
    assert!(rt.find("filter-any-column-pane").is_none(), "no any-column pane");
}

#[test]
fn the_mode_row_offers_the_three_tabs_and_switches() {
    let probe = Probe {
        columns: default_columns(),
        wire_modes: true,
        ..Probe::default()
    }
    .open_panel();
    let mut rt = runtime(probe, DESIGN, true);
    for key in [
        "filter-mode-structured",
        "filter-mode-anyColumn",
        "filter-mode-rawSql",
    ] {
        assert!(rt.find(key).is_some(), "{key} is offered");
    }
    click(&mut rt, "filter-mode-anyColumn");
    assert_eq!(
        rt.state.mode_changes,
        vec![FilterPanelMode::AnyColumn],
        "switching panes is render-only: it just reports the mode"
    );
}

#[test]
fn the_any_column_pane_applies_the_draft_and_scopes_it() {
    let mut editor_state = FilterEditorState::default();
    editor_state.any_column_edit = TextEdit::new("alice");
    let probe = Probe {
        columns: default_columns(),
        wire_modes: true,
        mode: Some(FilterPanelMode::AnyColumn),
        any_searchable: 7,
        any_skipped: 3,
        editor_state,
        ..Probe::default()
    }
    .open_panel();
    let mut rt = runtime(probe, DESIGN, true);
    assert!(rt.find("filter-any-column-pane").is_some(), "the pane renders");
    assert!(rt.find("any-column-input").is_some(), "the search field renders");
    let scope = element_text(&rt, "any-column-scope").unwrap_or_default();
    assert!(scope.contains('7') && scope.contains('3'), "the scope line carries both counts: {scope}");
    click(&mut rt, "any-column-apply");
    assert_eq!(rt.state.any_applies, vec!["alice".to_owned()], "the draft is applied");
}

#[test]
fn an_empty_any_column_draft_blocks_apply() {
    let probe = Probe {
        columns: default_columns(),
        wire_modes: true,
        mode: Some(FilterPanelMode::AnyColumn),
        ..Probe::default()
    }
    .open_panel();
    let mut rt = runtime(probe, DESIGN, true);
    click(&mut rt, "any-column-apply");
    assert!(
        rt.state.any_applies.is_empty(),
        "React disables the apply button while the draft is blank"
    );
}

#[test]
fn the_raw_sql_pane_shows_the_context_and_applies_the_predicate() {
    let mut editor_state = FilterEditorState::default();
    editor_state.raw_where_edit = TextEdit::new("id > 5");
    let probe = Probe {
        columns: default_columns(),
        wire_modes: true,
        mode: Some(FilterPanelMode::RawSql),
        raw_context: Some("demo · public".to_owned()),
        editor_state,
        ..Probe::default()
    }
    .open_panel();
    let mut rt = runtime(probe, DESIGN, true);
    assert!(rt.find("filter-raw-sql-pane").is_some(), "the pane renders");
    assert!(rt.find("raw-where-input").is_some(), "the predicate field renders");
    let hint = element_text(&rt, "raw-where-hint").unwrap_or_default();
    assert!(hint.contains("demo · public"), "the hint names where it runs: {hint}");
    click(&mut rt, "raw-where-apply");
    assert_eq!(rt.state.raw_applies, vec!["id > 5".to_owned()]);
}

#[test]
fn the_raw_sql_pane_without_context_shows_the_plain_hint() {
    let probe = Probe {
        columns: default_columns(),
        wire_modes: true,
        mode: Some(FilterPanelMode::RawSql),
        ..Probe::default()
    }
    .open_panel();
    let rt = runtime(probe, DESIGN, true);
    assert_eq!(
        element_text(&rt, "raw-where-hint").as_deref(),
        Some(t("filter.rawHint")),
        "no context line falls back to the raw hint"
    );
}

#[test]
fn the_logic_row_presses_and_and_or() {
    let probe = Probe {
        columns: default_columns(),
        ..Probe::default()
    }
    .open_panel();
    let mut rt = runtime(probe, DESIGN, true);
    click(&mut rt, "filter-logic-and");
    click(&mut rt, "filter-logic-or");
    assert_eq!(
        rt.state.logic_changes,
        vec![FilterConnector::And, FilterConnector::Or]
    );
}

#[test]
fn per_condition_logic_disables_the_global_row_and_offers_the_reset() {
    let probe = Probe {
        columns: default_columns(),
        draft: vec![
            cond("name", "alice"),
            cond_with_connector("age", "30", FilterConnector::Or),
        ],
        draft_logic: Some(FilterConnector::And),
        ..Probe::default()
    }
    .open_panel();
    let mut rt = runtime(probe, DESIGN, true);
    click(&mut rt, "filter-logic-and");
    assert!(
        rt.state.logic_changes.is_empty(),
        "the request-wide logic is meaningless while a condition carries its own"
    );
    assert!(
        rt.find("filter-connector-reset").is_some(),
        "the reset is offered"
    );
    click(&mut rt, "filter-connector-reset");
    assert_eq!(
        rt.state.changes,
        vec![(1, cond("age", "30"))],
        "React's resetConnectors: one onChange per rewritten condition, connector dropped"
    );
}

#[test]
fn connector_toggles_press_per_condition_and_or() {
    let probe = Probe {
        columns: default_columns(),
        draft: vec![
            cond("name", "alice"),
            cond_with_connector("age", "30", FilterConnector::And),
        ],
        ..Probe::default()
    }
    .open_panel();
    let mut rt = runtime(probe, DESIGN, true);
    click(&mut rt, "filter-connector-1-or");
    assert_eq!(
        rt.state.changes,
        vec![(1, cond_with_connector("age", "30", FilterConnector::Or))],
        "setConnector rewrites the one condition with the pressed connector"
    );
}

#[test]
fn chips_edit_records_the_row_and_remove_fires() {
    let probe = Probe {
        columns: default_columns(),
        draft: vec![cond("name", "alice")],
        ..Probe::default()
    }
    .open_panel();
    let mut rt = runtime(probe, DESIGN, true);
    assert!(rt.find("filter-row-0").is_some(), "the condition row renders");
    assert!(
        rt.find("filter-connector-0").is_none(),
        "the first condition has no predecessor to name a connector for"
    );
    click(&mut rt, "filter-chip-0-edit");
    assert_eq!(
        rt.state.editor_state.editing_index,
        Some(0),
        "the chip's edit records the row the editor would expand (T056)"
    );
    click(&mut rt, "filter-chip-0-remove");
    assert_eq!(rt.state.removes, vec![0], "the remove presses with its index");
}

#[test]
fn incomplete_conditions_are_marked_and_block_apply() {
    let probe = Probe {
        columns: default_columns(),
        draft: vec![cond("name", "alice"), FilterCondition::new(
            "age",
            "eq",
            FilterValue::Single(String::new()),
        )],
        applied: vec![cond("name", "alice")],
        ..Probe::default()
    }
    .open_panel();
    let mut rt = runtime(probe, DESIGN, true);
    assert!(
        rt.find("filter-incomplete").is_some(),
        "the incomplete condition is marked in the list"
    );
    assert!(
        rt.find("filter-incomplete-error").is_some(),
        "the banner names the blocking column"
    );
    click(&mut rt, "filter-apply");
    assert_eq!(rt.state.applies, 0, "an incomplete condition blocks Apply");
}

#[test]
fn apply_presses_only_when_dirty_and_clears_the_expanded_row() {
    // Dirty: the draft has one more condition than the applied list.
    let mut editor_state = FilterEditorState::default();
    editor_state.editing_index = Some(0);
    let probe = Probe {
        columns: default_columns(),
        applied: vec![cond("name", "alice")],
        draft: vec![cond("name", "alice"), cond("age", "30")],
        editor_state,
        ..Probe::default()
    }
    .open_panel();
    let mut rt = runtime(probe, DESIGN, true);
    click(&mut rt, "filter-apply");
    assert_eq!(rt.state.applies, 1, "a dirty complete draft applies");
    assert_eq!(
        rt.state.editor_state.editing_index,
        None,
        "React clears the editing row on Apply"
    );

    // Not dirty: the draft equals the applied list.
    let probe = Probe {
        columns: default_columns(),
        applied: vec![cond("name", "alice")],
        draft: vec![cond("name", "alice")],
        ..Probe::default()
    }
    .open_panel();
    let mut rt = runtime(probe, DESIGN, true);
    click(&mut rt, "filter-apply");
    assert_eq!(rt.state.applies, 0, "an unchanged draft cannot apply");
}

#[test]
fn loading_freezes_every_control() {
    let mut editor_state = FilterEditorState::default();
    editor_state.any_column_edit = TextEdit::new("alice");
    editor_state.raw_where_edit = TextEdit::new("id > 5");
    let probe = Probe {
        columns: default_columns(),
        loading: true,
        wire_modes: true,
        applied: vec![cond("name", "alice")],
        draft: vec![cond("name", "alice"), cond("age", "30")],
        editor_state,
        ..Probe::default()
    }
    .open_panel();
    let mut rt = runtime(probe, DESIGN, true);
    for key in [
        "filter-summary-toggle",
        "filter-add",
        "filter-clear",
        "filter-logic-and",
        "filter-logic-or",
        "filter-mode-anyColumn",
        "filter-apply",
        "filter-collapse",
    ] {
        click(&mut rt, key);
    }
    assert!(
        rt.state.open_changes.is_empty()
            && rt.state.adds.is_empty()
            && rt.state.clears == 0
            && rt.state.logic_changes.is_empty()
            && rt.state.mode_changes.is_empty()
            && rt.state.applies == 0,
        "no press handler fires while the applied request is in flight"
    );
}

#[test]
fn export_and_preview_render_only_when_offered() {
    let probe = Probe {
        columns: default_columns(),
        preview_sql: true,
        export_filtered: true,
        ..Probe::default()
    }
    .open_panel();
    let mut rt = runtime(probe, DESIGN, true);
    assert!(rt.find("filter-preview-sql").is_some(), "the preview is offered");
    assert!(rt.find("filter-export-filtered").is_some(), "the export is offered");
    click(&mut rt, "filter-preview-sql");
    click(&mut rt, "filter-export-filtered");
    assert_eq!(rt.state.previews, 1);
    assert_eq!(rt.state.exports, 1);

    let plain = Probe {
        columns: default_columns(),
        ..Probe::default()
    }
    .open_panel();
    let rt = runtime(plain, DESIGN, true);
    assert!(
        rt.find("filter-preview-sql").is_none(),
        "a panel that cannot reach the preview renders no button"
    );
    assert!(
        rt.find("filter-export-filtered").is_none(),
        "a panel without a filtered export renders no button"
    );
}

#[test]
fn the_apply_error_banner_renders_in_the_open_panels() {
    let probe = Probe {
        columns: default_columns(),
        apply_error: Some("boom".to_owned()),
        ..Probe::default()
    }
    .open_panel();
    let rt = runtime(probe, DESIGN, true);
    assert!(
        rt.find("filter-error").is_some(),
        "the caller-owned error is mounted in the structured pane"
    );
    assert_eq!(
        element_text(&rt, "filter-error-text").as_deref(),
        Some("boom"),
        "the banner carries the error text"
    );
}

#[test]
fn the_empty_list_shows_the_no_active_placeholder() {
    let probe = Probe {
        columns: default_columns(),
        ..Probe::default()
    }
    .open_panel();
    let rt = runtime(probe, DESIGN, true);
    assert!(
        element_text(&rt, "filter-no-active").as_deref() == Some(t("filter.noActive")),
        "an empty draft list says there is nothing active"
    );
}

#[test]
fn the_header_row_is_32pt_and_everything_stays_inside_both_window_sizes() {
    for size in [MIN, DESIGN] {
        for dark in [true, false] {
            let probe = Probe {
                columns: default_columns(),
                wire_modes: true,
                mode: Some(FilterPanelMode::RawSql),
                raw_context: Some("demo · public".to_owned()),
                apply_error: Some("boom".to_owned()),
                applied: vec![cond("name", "alice"), cond_with_connector("age", "30", FilterConnector::Or)],
                draft: vec![cond("name", "alice"), cond_with_connector("age", "30", FilterConnector::Or)],
                ..Probe::default()
            }
            .open_panel();
            let rt = runtime(probe, size, dark);
            let (width, height) = size;
            let header = rt
                .elements()
                .into_iter()
                .find(|e| e.key.as_deref() == Some("filter-header"))
                .expect("the header row is laid out");
            let [x, y, w, h] = header.rect;
            assert!(
                (h - 32.0).abs() < 0.5,
                "the summary row is React's h-8: {h}"
            );
            assert!(x >= -0.5 && y >= -0.5 && x + w <= width + 0.5 && y + h <= height + 0.5);
            for element in rt.elements() {
                let [x, y, w, h] = element.rect;
                let key = element.key.clone().unwrap_or_else(|| "<unnamed>".to_owned());
                assert!(
                    x >= -0.5 && y >= -0.5 && x + w <= width + 0.5 && y + h <= height + 0.5,
                    "{key} rect [{x}, {y}, {w}, {h}] leaves {width}x{height} (dark={dark})"
                );
            }
        }
    }
}

// ---- the pure model computations (no runtime needed) -----------------------

#[test]
fn the_summary_text_has_the_three_react_shapes() {
    let columns: Vec<FilterColumn> = Vec::new();
    // No active: nothing applied.
    let quiet: FilterEditor<'_, Probe> = FilterEditor::new(
        &columns,
        &[],
        FilterConnector::And,
        &[],
        FilterConnector::And,
        true,
    );
    assert_eq!(quiet.summary_text(), t("filter.noActive"));

    // Raw applied with no conditions: the predicate is quoted.
    let raw: FilterEditor<'_, Probe> = FilterEditor::new(
        &columns,
        &[],
        FilterConnector::And,
        &[],
        FilterConnector::And,
        false,
    )
    .raw_where(Some("id > 5"), None);
    assert_eq!(
        raw.summary_text(),
        t("filter.rawAppliedSummary").replacen("{predicate}", "id > 5", 1)
    );

    // Mixed list: each condition joins with the connector in effect.
    let mixed_conditions = vec![
        cond("name", "alice"),
        cond_with_connector("age", "30", FilterConnector::Or),
        cond("city", "berlin"),
    ];
    let mixed: FilterEditor<'_, Probe> = FilterEditor::new(
        &columns,
        &mixed_conditions,
        FilterConnector::And,
        &[],
        FilterConnector::And,
        false,
    );
    let and = FilterConnector::And.label().to_uppercase();
    let or = FilterConnector::Or.label().to_uppercase();
    let text = mixed.summary_text();
    assert!(
        text.starts_with("name") && text.contains(&format!(" {or} age")) && text.contains(&format!(" {and} city")),
        "the joined summary reads {text}"
    );
}

#[test]
fn default_operator_and_the_shared_set_follow_the_react_tables() {
    assert_eq!(default_operator_for_column(Some("text"), None), "eq");
    assert_eq!(default_operator_for_column(Some("number"), None), "eq");
    assert_eq!(default_operator_for_column(Some("boolean"), None), "eq");
    assert_eq!(default_operator_for_column(None, None), "eq", "unknown falls back to eq");
    // The hints path takes precedence over the type string.
    let hints = CellTypeHints::default();
    assert_eq!(default_operator_for_column(Some("custom"), Some(&hints)), "eq");
    assert_eq!(SHARED_FILTER_OPERATORS.len(), 19, "the ten existing plus the nine shared extensions");
    assert_eq!(SHARED_FILTER_OPERATORS.first(), Some(&"eq"));
    assert_eq!(SHARED_FILTER_OPERATORS.last(), Some(&"isNotEmpty"));
}

#[test]
fn per_condition_logic_ignores_the_first_conditions_connector() {
    // The first condition's own connector is meaningless: only the rows
    // after the first decide whether the request-wide row disables and
    // the reset is offered.
    let first_only = Probe {
        columns: default_columns(),
        draft: vec![cond_with_connector("name", "alice", FilterConnector::Or)],
        ..Probe::default()
    }
    .open_panel();
    let rt = runtime(first_only, DESIGN, true);
    assert!(
        rt.find("filter-connector-reset").is_none(),
        "a lone condition cannot offer the reset"
    );
    assert!(
        rt.find("filter-connector-0").is_none(),
        "the first row renders no connector toggle either"
    );

    let second_carries = Probe {
        columns: default_columns(),
        draft: vec![
            cond("name", "alice"),
            cond_with_connector("age", "30", FilterConnector::Or),
        ],
        ..Probe::default()
    }
    .open_panel();
    let rt = runtime(second_carries, DESIGN, true);
    assert!(
        rt.find("filter-connector-reset").is_some(),
        "the second condition's connector offers the reset"
    );
    assert!(
        rt.find("filter-connector-1").is_some(),
        "the second row renders its connector toggle"
    );
}
