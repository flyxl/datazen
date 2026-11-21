//! T280: the NL2SQL panel — coder vectors: the pure helpers (the
//! generate guard, the SQL write action, the context split, the
//! preview derivation), the unconfigured bar, the generating vs
//! generated button sets, the preview card's label and spinner, the
//! memory card's chip/citations/expanded list, the error bar, and the
//! press journeys.

use rune::{Cx, El, Env, Runtime};

use crate::components::ai::ai_input::AiInputState;
use crate::components::ai::chat_bubble::AiMemoryReference;
use crate::components::ai::nl2_sql_panel::{
    can_generate, preview_sql, split_context_items, sql_write_action, Nl2SqlPanel, Nl2SqlPanelVm,
};
use crate::components::ai::ai_egress_notice::{ContextItem, ContextKind};

fn env(dark: bool) -> Env {
    Env {
        system_dark: dark,
        scale: 1.0,
    }
}

fn memory(reference: &str, subject: &str) -> AiMemoryReference {
    AiMemoryReference::new(reference, format!("id-{reference}"), subject, "table", "database")
}

// -- the pure helpers ---------------------------------------------------------

#[test]
fn the_generate_guard_pins_the_react_rule() {
    assert!(can_generate("查所有用户", false, "db"), "a healthy state generates");
    assert!(!can_generate("   ", false, "db"), "a blank input refuses");
    assert!(!can_generate("查询", true, "db"), "a generating state refuses");
    assert!(!can_generate("查询", false, ""), "no database refuses");
}

#[test]
fn the_sql_write_action_pins_the_effect() {
    // Still generating: no write.
    assert_eq!(sql_write_action(true, "SELECT 1", ""), None);
    // Finished with a new SQL: the write.
    assert_eq!(sql_write_action(false, "SELECT 1", ""), Some("SELECT 1".to_owned()));
    // Already written: no second write.
    assert_eq!(sql_write_action(false, "SELECT 1", "SELECT 1"), None);
    // Finished with no SQL: nothing.
    assert_eq!(sql_write_action(false, "", ""), None);
}

#[test]
fn the_context_split_sorts_tables_from_files() {
    let items = vec![
        ContextItem { kind: ContextKind::Table, id: "public.users".to_owned(), name: "users".to_owned() },
        ContextItem { kind: ContextKind::File, id: "schema.sql".to_owned(), name: "schema".to_owned() },
        ContextItem { kind: ContextKind::Dir, id: "docs/".to_owned(), name: "docs".to_owned() },
    ];
    let (files, tables) = split_context_items(&items);
    assert_eq!(tables, vec!["public.users".to_owned()]);
    assert_eq!(
        files,
        vec!["schema.sql".to_owned(), "docs/".to_owned()],
        "files and dirs fall back to the id (the minimal mirror carries no path)"
    );
}

#[test]
fn the_preview_derivation_follows_the_two_disjuncts() {
    // Generating with a streaming preview: the preview.
    let vm = Nl2SqlPanelVm {
        is_generating: true,
        streaming_preview: "SELECT".to_owned(),
        ..Default::default()
    };
    assert_eq!(preview_sql(&vm), Some("SELECT".to_owned()));
    // Generating with an empty preview: nothing.
    let vm = Nl2SqlPanelVm { is_generating: true, ..Default::default() };
    assert_eq!(preview_sql(&vm), None);
    // Idle with a generated SQL: the result.
    let vm = Nl2SqlPanelVm { generated_sql: "SELECT 2".to_owned(), ..Default::default() };
    assert_eq!(preview_sql(&vm), Some("SELECT 2".to_owned()));
    // Idle with nothing: nothing.
    assert_eq!(preview_sql(&Nl2SqlPanelVm::default()), None);
}

// -- the render ----------------------------------------------------------------

#[derive(Clone, Default)]
struct Probe {
    vm: Nl2SqlPanelVm,
    input: AiInputState,
    log: Vec<String>,
}

fn view(s: &Probe, _cx: &mut Cx<Probe>) -> El {
    Nl2SqlPanel::<Probe>::new(&s.vm, &s.input, "db")
        .on_generate(|st| st.log.push("generate".to_owned()))
        .on_stop(|st| st.log.push("stop".to_owned()))
        .on_copy(|st| st.log.push("copy".to_owned()))
        .on_clear(|st| st.log.push("clear".to_owned()))
        .on_open_ai_settings(|st| st.log.push("settings".to_owned()))
        .on_toggle_memory(|st| st.log.push("memory".to_owned()))
        .on_focus_memory(|st, reference| st.log.push(format!("focus:{reference}")))
        .build(crate::ui::kit::kit(true), "")
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
fn the_unconfigured_bar_is_the_whole_panel() {
    let rt = mount(Probe::default(), (960.0, 640.0));
    assert!(has_key(&rt, "nl2sql-panel-not-configured"));
    assert_eq!(
        text_of(&rt, "nl2sql-panel-not-configured-text"),
        Some(crate::i18n::t("common.aiNotConfigured").to_owned())
    );
    assert!(has_key(&rt, "nl2sql-panel-open-settings"), "the settings button mounts");
    // Nothing else mounts.
    assert!(!has_key(&rt, "nl2sql-panel-input-row"));
    assert!(!has_key(&rt, "nl2sql-preview-card"));
}

#[test]
fn the_generating_state_swaps_generate_for_stop() {
    let mut state = Probe::default();
    state.vm.is_configured = true;
    state.vm.is_generating = true;
    let rt = mount(state, (960.0, 640.0));
    assert!(has_key(&rt, "nl2sql-stop"), "the stop button mounts");
    assert!(!has_key(&rt, "nl2sql-generate"), "the generate button is swapped out");

    // Idle: generate mounts, stop does not.
    let mut state = Probe::default();
    state.vm.is_configured = true;
    let rt = mount(state, (960.0, 640.0));
    assert!(has_key(&rt, "nl2sql-generate"));
    assert!(!has_key(&rt, "nl2sql-stop"));
    assert!(!has_key(&rt, "nl2sql-copy"), "no copy without a generated SQL");
    assert!(!has_key(&rt, "nl2sql-clear"));
}

#[test]
fn a_generated_sql_mounts_the_copy_and_clear_buttons_and_the_result_card() {
    let mut state = Probe::default();
    state.vm.is_configured = true;
    state.vm.generated_sql = "SELECT 1".to_owned();
    let rt = mount(state, (960.0, 640.0));
    assert!(has_key(&rt, "nl2sql-copy"));
    assert!(has_key(&rt, "nl2sql-clear"));
    assert!(has_key(&rt, "nl2sql-preview-card"));
    assert_eq!(
        text_of(&rt, "nl2sql-preview-label"),
        Some(crate::i18n::t("nl2sql.result").to_owned()),
        "the idle card carries the result label"
    );
    assert!(!has_key(&rt, "nl2sql-preview-spinner"), "no spinner while idle");

    // Generating with a streaming preview: the preview label and the spinner.
    let mut state = Probe::default();
    state.vm.is_configured = true;
    state.vm.is_generating = true;
    state.vm.streaming_preview = "SELECT".to_owned();
    let rt = mount(state, (960.0, 640.0));
    assert_eq!(
        text_of(&rt, "nl2sql-preview-label"),
        Some(crate::i18n::t("nl2sql.preview").to_owned())
    );
    assert!(has_key(&rt, "nl2sql-preview-spinner"));
}

#[test]
fn the_memory_card_mounts_its_chip_citations_and_the_expanded_list() {
    let mut state = Probe::default();
    state.vm.is_configured = true;
    state.vm.memory_used = vec![memory("M1", "用户表结构"), memory("M2", "订单表结构")];
    state.vm.memory_open = true;
    let rt = mount(state, (960.0, 640.0));
    assert!(has_key(&rt, "nl2sql-memory-card"));
    assert!(has_key(&rt, "ai-memory-chip"));
    // The chip's label carries the count through the pack word.
    let chip_label = keyed(&rt, "ai-memory-chip").and_then(|e| e.label).unwrap_or_default();
    assert!(chip_label.contains("2"), "the count rides the chip label: {chip_label}");
    for reference in ["M1", "M2"] {
        assert!(has_key(&rt, &format!("ai-memory-citation-{reference}")));
        assert!(has_key(&rt, &format!("ai-memory-item-{reference}")));
    }
    assert_eq!(text_of(&rt, "ai-memory-item-M1-subject"), Some("用户表结构".to_owned()));
    assert!(has_key(&rt, "ai-memory-expanded"), "the open list mounts");

    // Closed: the list does not mount.
    let mut state = Probe::default();
    state.vm.is_configured = true;
    state.vm.memory_used = vec![memory("M1", "s")];
    let rt = mount(state, (960.0, 640.0));
    assert!(!has_key(&rt, "ai-memory-expanded"));

    // No memory: no card at all.
    let mut state = Probe::default();
    state.vm.is_configured = true;
    let rt = mount(state, (960.0, 640.0));
    assert!(!has_key(&rt, "nl2sql-memory-card"));
}

#[test]
fn the_focused_memory_washes_its_row() {
    let mut state = Probe::default();
    state.vm.is_configured = true;
    state.vm.memory_used = vec![memory("M1", "s"), memory("M2", "s")];
    state.vm.memory_open = true;
    state.vm.memory_focused = Some("M2".to_owned());
    let rt = mount(state, (960.0, 640.0));
    // The focus-selected row: ElementInfo's item_selected flag (the
    // focus.rs selected styling).
    let focused = keyed(&rt, "ai-memory-item-M2").expect("the focused row mounts");
    let plain = keyed(&rt, "ai-memory-item-M1").expect("the plain row mounts");
    assert!(focused.item_selected, "the focused reference's row is focus-selected");
    assert!(!plain.item_selected);
}

#[test]
fn the_error_bar_mounts_the_error_verbatim() {
    let mut state = Probe::default();
    state.vm.is_configured = true;
    state.vm.error = Some("生成失败：服务不可用".to_owned());
    let rt = mount(state, (960.0, 640.0));
    assert_eq!(
        text_of(&rt, "nl2sql-error-text"),
        Some("生成失败：服务不可用".to_owned())
    );

    // React's truthiness: a whitespace-only error is truthy and mounts;
    // only the empty string (or None) mounts nothing.
    let mut state = Probe::default();
    state.vm.is_configured = true;
    state.vm.error = Some("   ".to_owned());
    let rt = mount(state, (960.0, 640.0));
    assert!(has_key(&rt, "nl2sql-error"), "a whitespace error is truthy");
    let mut state = Probe::default();
    state.vm.is_configured = true;
    state.vm.error = Some(String::new());
    let rt = mount(state, (960.0, 640.0));
    assert!(!has_key(&rt, "nl2sql-error"), "the empty string mounts nothing");
}

#[test]
fn the_presses_reach_the_seams() {
    let mut state = Probe::default();
    state.vm.is_configured = true;
    state.vm.input = "查询用户".to_owned();
    state.vm.generated_sql = "SELECT 1".to_owned();
    let mut rt = mount(state, (960.0, 640.0));
    click(&mut rt, "nl2sql-generate");
    click(&mut rt, "nl2sql-copy");
    click(&mut rt, "nl2sql-clear");
    assert_eq!(rt.state.log, vec!["generate".to_owned(), "copy".to_owned(), "clear".to_owned()]);

    // The memory chip and a citation.
    let mut state = Probe::default();
    state.vm.is_configured = true;
    state.vm.memory_used = vec![memory("M1", "s")];
    let mut rt = mount(state, (960.0, 640.0));
    click(&mut rt, "ai-memory-chip");
    click(&mut rt, "ai-memory-citation-M1");
    assert_eq!(rt.state.log, vec!["memory".to_owned(), "focus:M1".to_owned()]);

    // The settings button (the unconfigured bar).
    let mut rt = mount(Probe::default(), (960.0, 640.0));
    click(&mut rt, "nl2sql-panel-open-settings");
    assert_eq!(rt.state.log, vec!["settings".to_owned()]);

    // The stop button while generating.
    let mut state = Probe::default();
    state.vm.is_configured = true;
    state.vm.is_generating = true;
    let mut rt = mount(state, (960.0, 640.0));
    click(&mut rt, "nl2sql-stop");
    assert_eq!(rt.state.log, vec!["stop".to_owned()]);
}

#[test]
fn the_generate_button_follows_the_guard() {
    // The nav's own precedent: the disabled button's press stays
    // silent (the kit removes the handler with State::Disabled).
    let mut state = Probe::default();
    state.vm.is_configured = true;
    let mut rt = mount(state, (960.0, 640.0));
    click(&mut rt, "nl2sql-generate");
    assert!(rt.state.log.is_empty(), "a blank input's generate stays silent");

    // With an input: the press fires.
    let mut state = Probe::default();
    state.vm.is_configured = true;
    state.vm.input = "查所有用户".to_owned();
    let mut rt = mount(state, (960.0, 640.0));
    click(&mut rt, "nl2sql-generate");
    assert_eq!(rt.state.log, vec!["generate".to_owned()]);
}
