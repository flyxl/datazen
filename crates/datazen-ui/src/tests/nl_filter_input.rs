//! T299: the NL filter input — coder vectors: the pure helpers (the
//! table-change reset, the parsed-filter outcome's four branches, the
//! unknown-columns sentence), the three mounts (unconfigured,
//! collapsed, expanded), the parsing swap, the status lines, and the
//! press journeys.

use std::collections::HashMap;

use rune::{Cx, El, Env, Runtime};

use crate::ai_store::FilterCondition;
use crate::components::ai::nl_filter_input::{
    parsed_filter_outcome, table_changed, unknown_columns_message, NlFilterInput, NlFilterInputVm,
    ParsedFilterOutcome,
};
use crate::table_data_connection_state::ColumnSchema;

fn env(dark: bool) -> Env {
    Env {
        system_dark: dark,
        scale: 1.0,
    }
}

fn column(name: &str) -> ColumnSchema {
    ColumnSchema {
        name: name.to_owned(),
        data_type: "text".to_owned(),
        nullable: true,
        default_value: None,
        is_primary_key: false,
        is_auto_increment: false,
        comment: None,
    }
}

fn filter(column: &str) -> FilterCondition {
    FilterCondition {
        column: column.to_owned(),
        operator: "=".to_owned(),
        value: "1".to_owned(),
    }
}

// -- the pure helpers ---------------------------------------------------------

#[test]
fn the_table_change_resets_everything() {
    let reset = table_changed();
    assert!(reset.clear_nl_filter);
    assert!(reset.clear_validation_error);
    assert!(reset.collapse);
    assert!(reset.rearm_abort);
}

#[test]
fn the_parsed_outcome_drops_a_stale_parse() {
    // A different table: dropped.
    assert_eq!(
        parsed_filter_outcome(
            &[filter("name")],
            &[column("name")],
            Some("other"),
            Some("s1"),
            "users",
            "s1"
        ),
        ParsedFilterOutcome::Stale
    );
    // A different session: dropped too.
    assert_eq!(
        parsed_filter_outcome(
            &[filter("name")],
            &[column("name")],
            Some("users"),
            Some("s1"),
            "users",
            "s2"
        ),
        ParsedFilterOutcome::Stale
    );
    // No context (the panel gone): dropped.
    assert_eq!(
        parsed_filter_outcome(&[filter("name")], &[column("name")], None, None, "users", "s1"),
        ParsedFilterOutcome::Stale
    );
}

#[test]
fn the_unknown_column_validation_reports_the_strangers() {
    // Two filters naming one unknown column (deduped) and one known.
    let columns = vec![column("name"), column("age")];
    let filters = vec![filter("Name"), filter("email"), filter("EMAIL"), filter("age")];
    // React's Set is case-SENSITIVE: EMAIL and email both ride the
    // warning, first-seen order.
    assert_eq!(
        parsed_filter_outcome(&filters, &columns, Some("users"), Some("s1"), "users", "s1"),
        ParsedFilterOutcome::UnknownColumns(vec!["email".to_owned(), "EMAIL".to_owned()])
    );

    // The trim applies: a padded unknown column reports trimmed.
    let padded = FilterCondition {
        column: "  email  ".to_owned(),
        operator: "=".to_owned(),
        value: "1".to_owned(),
    };
    assert_eq!(
        parsed_filter_outcome(&[padded], &columns, Some("users"), Some("s1"), "users", "s1"),
        ParsedFilterOutcome::UnknownColumns(vec!["email".to_owned()])
    );

    // The case-insensitive membership: Name matches name.
    assert_eq!(
        parsed_filter_outcome(&[filter("NAME")], &columns, Some("users"), Some("s1"), "users", "s1"),
        ParsedFilterOutcome::Apply(vec![filter("NAME")])
    );

    // An empty column list must not turn a valid filter into a false
    // warning (the metadata may still be loading).
    assert_eq!(
        parsed_filter_outcome(&[filter("anything")], &[], Some("users"), Some("s1"), "users", "s1"),
        ParsedFilterOutcome::Apply(vec![filter("anything")])
    );
}

#[test]
fn the_unknown_columns_sentence_replaces_the_list() {
    let message = unknown_columns_message(&["email".to_owned(), "phone".to_owned()]);
    assert!(message.contains("email, phone"), "the joined list: {message}");
    assert!(message.contains("确认"), "the pack word: {message}");
}

#[test]
fn the_empty_parse_applies_nothing() {
    // An empty filter list: the write is empty (React's length gate).
    assert_eq!(
        parsed_filter_outcome(&[], &[column("name")], Some("users"), Some("s1"), "users", "s1"),
        ParsedFilterOutcome::Apply(Vec::new())
    );
}

// -- the mounts -----------------------------------------------------------------

#[derive(Clone, Default)]
struct Probe {
    vm: NlFilterInputVm,
    log: Vec<String>,
}

fn view(s: &Probe, _cx: &mut Cx<Probe>) -> El {
    NlFilterInput::<Probe>::new(&s.vm)
        .on_open_ai_settings(|st| st.log.push("settings".to_owned()))
        .on_expand(|st| st.log.push("expand".to_owned()))
        .on_parse(|st| st.log.push("parse".to_owned()))
        .on_clear(|st| st.log.push("clear".to_owned()))
        .build(crate::ui::kit::kit(true))
}

fn mount(state: Probe, size: (f32, f32)) -> Runtime<Probe> {
    let mut rt = Runtime::new(state, view, size, env(true));
    rt.frame(0.0);
    rt
}

fn keyed<S>(rt: &Runtime<S>, key: &str) -> Option<rune::ElementInfo> {
    rt.elements()
        .into_iter()
        .find(|e| e.key.as_deref() == Some(key) && !e.exiting)
}

fn has_key<S>(rt: &Runtime<S>, key: &str) -> bool {
    keyed(rt, key).is_some()
}

fn text_of<S>(rt: &Runtime<S>, key: &str) -> Option<String> {
    keyed(rt, key).and_then(|e| e.text)
}

fn click<S>(rt: &mut Runtime<S>, key: &str) {
    let r = keyed(rt, key)
        .unwrap_or_else(|| panic!("{key} is laid out"))
        .rect;
    let (x, y) = (r[0] + r[2] * 0.5, r[1] + r[3] * 0.5);
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    rt.frame(1.0 / 60.0);
    rt.frame(1.0 / 60.0);
}

#[test]
fn the_unconfigured_notice_is_the_whole_input() {
    let rt = mount(Probe::default(), (960.0, 640.0));
    assert!(has_key(&rt, "nl-filter-not-configured"));
    assert!(!has_key(&rt, "smart-filter-toggle"), "no toggle unconfigured");
    assert!(!has_key(&rt, "nl-filter-input-row"), "no composer unconfigured");
}

#[test]
fn the_collapsed_toggle_mounts_the_react_testid() {
    let mut state = Probe::default();
    state.vm.is_configured = true;
    let rt = mount(state.clone(), (960.0, 640.0));
    assert!(has_key(&rt, "smart-filter-toggle"));
    assert!(!has_key(&rt, "nl-filter-input-row"), "collapsed: no composer");
}

#[test]
fn the_expanded_composer_mounts_its_row_and_children() {
    let mut state = Probe::default();
    state.vm.is_configured = true;
    state.vm.expanded = true;
    let rt = mount(state.clone(), (960.0, 640.0));
    assert!(has_key(&rt, "nl-filter-input"));
    assert!(has_key(&rt, "nl-filter-input-row"));
    assert!(has_key(&rt, "nl-filter-input-parse"), "the parse button");
    assert!(has_key(&rt, "nl-filter-input-clear"), "the clear button");
    assert!(!has_key(&rt, "nl-filter-input-parsing"), "idle: no spinner");

    // Parsing: the swap.
    state.vm.is_parsing = true;
    let rt = mount(state.clone(), (960.0, 640.0));
    assert!(has_key(&rt, "nl-filter-input-parsing"));
    assert!(has_key(&rt, "nl-filter-input-spinner"));
    assert!(!has_key(&rt, "nl-filter-input-parse"), "parsing: no parse button");
    let parsing_row = keyed(&rt, "nl-filter-input-parsing").expect("the row mounts");
    let word_mounted = rt
        .elements()
        .iter()
        .any(|e| e.text.as_deref().is_some_and(|t| t.contains(crate::i18n::t("smartFilter.parsing")))
            && e.path.starts_with(&parsing_row.path));
    assert!(word_mounted, "the parsing word rides the row's children");
}

#[test]
fn the_four_status_lines_mount_by_state() {
    // The parse error banner.
    let mut state = Probe::default();
    state.vm.is_configured = true;
    state.vm.expanded = true;
    state.vm.parse_error = Some("解析失败".to_owned());
    let rt = mount(state.clone(), (960.0, 640.0));
    assert!(has_key(&rt, "nl-filter-input-error"));

    // The validation warning.
    state.vm.parse_error = None;
    state.vm.validation_error = Some("筛选字段需要确认：email".to_owned());
    let rt = mount(state.clone(), (960.0, 640.0));
    let warning = keyed(&rt, "nl-filter-input-validation").expect("the warning mounts");
    assert_eq!(warning.role, "alert");
    assert_eq!(text_of(&rt, "nl-filter-input-validation"), Some("筛选字段需要确认：email".to_owned()));

    // The no-filters note.
    state.vm.validation_error = None;
    state.vm.parsed_filters = Some(Vec::new());
    let rt = mount(state.clone(), (960.0, 640.0));
    assert!(has_key(&rt, "nl-filter-input-empty"));
    assert!(!has_key(&rt, "nl-filter-input-parsed"));

    // The parsed-count status (suppressed by a validation error).
    state.vm.parsed_filters = Some(vec![filter("name"), filter("age")]);
    let rt = mount(state.clone(), (960.0, 640.0));
    let status = keyed(&rt, "nl-filter-input-parsed").expect("the status mounts");
    assert_eq!(status.role, "status");
    let text = text_of(&rt, "nl-filter-input-parsed").unwrap_or_default();
    assert!(text.contains("2"), "the count rides the status: {text}");

    // Suppressed by a validation error.
    state.vm.validation_error = Some("warn".to_owned());
    let rt = mount(state.clone(), (960.0, 640.0));
    assert!(!has_key(&rt, "nl-filter-input-parsed"), "a validation error suppresses the status");
}

#[test]
fn the_presses_reach_the_seams() {
    // The collapsed toggle expands.
    let mut state = Probe::default();
    state.vm.is_configured = true;
    let mut rt = mount(state, (960.0, 640.0));
    click(&mut rt, "smart-filter-toggle");
    assert_eq!(rt.state.log, vec!["expand".to_owned()]);

    // The parse press — a non-empty input keeps the button live
    // (React's `disabled={!nlFilterInput.trim()}`).
    let mut state = Probe::default();
    state.vm.is_configured = true;
    state.vm.expanded = true;
    state.vm.input = "年龄大于 18".to_owned();
    let mut rt = mount(state, (960.0, 640.0));
    click(&mut rt, "nl-filter-input-parse");
    assert_eq!(rt.state.log, vec!["parse".to_owned()]);

    // The clear press.
    click(&mut rt, "nl-filter-input-clear");
    assert_eq!(rt.state.log, vec!["parse".to_owned(), "clear".to_owned()]);

    // The unconfigured notice's settings press.
    let mut rt = mount(Probe::default(), (960.0, 640.0));
    // The notice's press rides its own button key: find the row's button
    // child by path (the banner's button carries the pack word label).
    let notice = keyed(&rt, "nl-filter-not-configured").expect("the notice mounts");
    let button = rt
        .elements()
        .iter()
        .find(|e| e.role == "button" && e.path.starts_with(&notice.path) && !e.exiting)
        .expect("the notice's button mounts")
        .rect;
    let (x, y) = (button[0] + button[2] * 0.5, button[1] + button[3] * 0.5);
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    rt.frame(1.0 / 60.0);
    rt.frame(1.0 / 60.0);
    assert_eq!(rt.state.log, vec!["settings".to_owned()]);
}

#[test]
fn the_unwired_input_renders_the_model_text() {
    let mut state = Probe::default();
    state.vm.is_configured = true;
    state.vm.expanded = true;
    state.vm.input = "年龄大于 18".to_owned();
    let rt = mount(state.clone(), (960.0, 640.0));
    assert_eq!(
        text_of(&rt, "nl-filter-input-field"),
        Some("年龄大于 18".to_owned())
    );
}

/// Keeps the HashMap import honest: the host's panel slice map is the
/// store's own; the component reads only its copied columns.
#[allow(dead_code)]
fn _no_panel_map_here(_: &HashMap<String, u8>) {}
