//! T166: `FilterTokenInput` + `FilterValueInput` — commit/remove/keyboard,
//! disabled state, exact keys/i18n and two-window/two-palette geometry.
//!
//! State is owned by a probe `S`; every assertion observes the rendered tree or
//! the probe's recorded callbacks after a real pointer/key event.

use rune::{Cx, El, Env, Key, KeyEvent, Mods, Runtime, TextEdit};
use rune_kit::root;

use datazen_ui::components::filter::{FilterTokenInput, FilterValueInput};
use datazen_ui::i18n::t;

#[derive(Default)]
struct Probe {
    dark: bool,
    disabled: bool,
    tokens: Vec<String>,
    token_draft: TextEdit,
    value: TextEdit,
    token_commits: Vec<Vec<String>>,
    value_commits: Vec<String>,
    completes: usize,
}

fn token_view(s: &Probe, _cx: &mut Cx<Probe>) -> El {
    let widget = FilterTokenInput::<Probe>::new(
        "edge",
        s.tokens.clone(),
        &s.token_draft,
        |st: &mut Probe| &mut st.token_draft,
    )
    .placeholder(t("filter.tokensPlaceholder"))
    .disabled(s.disabled)
    .remove_label(t("filter.tokenRemove"))
    .on_commit(|st: &mut Probe, next: Vec<String>| {
        st.token_commits.push(next.clone());
        st.tokens = next;
    })
    .on_enter_complete(|st: &mut Probe| st.completes += 1);
    root(widget.build(s.dark), Vec::new())
}

fn value_view(s: &Probe, _cx: &mut Cx<Probe>) -> El {
    let widget = FilterValueInput::<Probe>::new("edge", &s.value, |st: &mut Probe| &mut st.value)
        .placeholder(t("filter.value"))
        .disabled(s.disabled)
        .on_commit(|st: &mut Probe, value: String| st.value_commits.push(value))
        .on_enter_complete(|st: &mut Probe| st.completes += 1);
    root(widget.build(s.dark), Vec::new())
}

fn env(dark: bool) -> Env {
    Env {
        system_dark: dark,
        scale: 1.0,
    }
}

fn token_rt(dark: bool, size: (f32, f32), tokens: Vec<&str>, draft: &str, disabled: bool) -> Runtime<Probe> {
    let mut state = Probe {
        dark,
        disabled,
        tokens: tokens.into_iter().map(str::to_owned).collect(),
        token_draft: TextEdit::new(draft.to_owned()),
        ..Probe::default()
    };
    state.dark = dark;
    let mut rt = Runtime::new(state, token_view, size, env(dark));
    rt.frame(0.0);
    rt
}

fn value_rt(dark: bool, size: (f32, f32), draft: &str, disabled: bool) -> Runtime<Probe> {
    let mut state = Probe {
        dark,
        disabled,
        value: TextEdit::new(draft.to_owned()),
        ..Probe::default()
    };
    state.dark = dark;
    let mut rt = Runtime::new(state, value_view, size, env(dark));
    rt.frame(0.0);
    rt
}

fn has<S>(rt: &Runtime<S>, key: &str) -> bool {
    rt.elements()
        .iter()
        .any(|e| e.key.as_deref() == Some(key) && !e.exiting)
}

fn element<S>(rt: &Runtime<S>, key: &str) -> rune::ElementInfo {
    rt.elements()
        .into_iter()
        .find(|e| e.key.as_deref() == Some(key) && !e.exiting)
        .unwrap_or_else(|| panic!("{key} is laid out"))
}

fn texts<S>(rt: &Runtime<S>) -> Vec<String> {
    rt.elements().iter().filter_map(|e| e.text.clone()).collect()
}

fn click<S>(rt: &mut Runtime<S>, key: &str) {
    let (x, y) = rt.find(key).unwrap_or_else(|| panic!("{key} is laid out before click"));
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    rt.frame(0.0);
}

fn press<S>(rt: &mut Runtime<S>, key: Key) {
    rt.key(KeyEvent {
        key,
        mods: Mods::default(),
    });
    rt.frame(0.0);
}

fn type_into<S>(rt: &mut Runtime<S>, key: &str, text: &str) {
    click(rt, key);
    rt.paste(text);
    rt.frame(0.0);
}

// ---- Token input: keys, labels and chips -------------------------------------------------------

#[test]
fn token_input_keeps_react_testids_and_chip_labels() {
    let rt = token_rt(true, (1440.0, 816.0), vec!["paid", "sent"], "", false);
    for key in [
        "filter-token-input",
        "edge-tokens",
        "filter-token-0",
        "filter-token-1",
        "edge-token-0",
        "edge-token-1",
        "edge-token-0-remove",
        "filter-token-remove-0",
        "filter-token-field",
        "edge-token-field",
    ] {
        assert!(has(&rt, key), "{key} is mounted");
    }
    let rendered = texts(&rt);
    assert!(rendered.iter().any(|x| x == "paid") && rendered.iter().any(|x| x == "sent"));
    assert_eq!(
        element(&rt, "edge-token-0-remove").help.as_deref(),
        Some(t("filter.tokenRemove")),
        "the chip remove control carries React's exact label"
    );
    // Placeholders are drawing hints, not `text` nodes; pin the exact key at
    // the editor's call site and verify it resolves to a translation.
    assert!(
        include_str!("../src/components/filter/filter_condition_editor.rs")
            .contains("t(\"filter.tokensPlaceholder\")"),
        "the editor supplies React's placeholder key to the port"
    );
    assert!(t("filter.tokensPlaceholder") != "filter.tokensPlaceholder");
}

// ---- Token input: keyboard and click behavior ---------------------------------------------------

#[test]
fn enter_commits_the_pending_token_clears_the_field_and_completes() {
    let mut rt = token_rt(true, (1440.0, 816.0), vec!["paid"], "sent", false);
    click(&mut rt, "edge-token-field");
    press(&mut rt, Key::Enter);

    assert_eq!(rt.state.token_commits, vec![vec!["paid".to_owned(), "sent".to_owned()]]);
    assert_eq!(rt.state.token_draft.text, "", "the pending field is cleared");
    assert_eq!(rt.state.completes, 1, "Enter calls onEnterComplete");
    assert!(has(&rt, "edge-token-1"), "the new chip is mounted after the host applied the commit");
}

#[test]
fn comma_commits_a_token_without_completing() {
    let mut rt = token_rt(true, (1440.0, 816.0), vec![], "paid", false);
    click(&mut rt, "edge-token-field");
    press(&mut rt, Key::Text(",".to_owned()));

    assert_eq!(rt.state.token_commits, vec![vec!["paid".to_owned()]]);
    assert_eq!(rt.state.token_draft.text, "");
    assert_eq!(rt.state.completes, 0, "comma alone never collapses");
}

#[test]
fn backspace_on_empty_field_removes_the_last_chip() {
    let mut rt = token_rt(true, (1440.0, 816.0), vec!["paid", "sent"], "", false);
    click(&mut rt, "edge-token-field");
    press(&mut rt, Key::Backspace);

    assert_eq!(rt.state.token_commits, vec![vec!["paid".to_owned()]]);
    assert_eq!(rt.state.completes, 0);
}

#[test]
fn clicking_a_chip_remove_button_removes_it_by_position() {
    let mut rt = token_rt(true, (1440.0, 816.0), vec!["paid", "sent"], "", false);
    click(&mut rt, "edge-token-0-remove");
    assert_eq!(rt.state.token_commits, vec![vec!["sent".to_owned()]]);
    assert_eq!(rt.state.completes, 0, "removing a token is not a collapse");
}

#[test]
fn disabled_token_input_ignores_typing_and_cannot_remove() {
    let mut rt = token_rt(true, (1440.0, 816.0), vec!["paid"], "", true);
    assert!(!element(&rt, "edge-token-field").focusable, "disabled field has no caret");
    assert!(!element(&rt, "edge-token-0-remove").focusable, "disabled remove is inert");
    type_into(&mut rt, "edge-token-field", "ignored");
    assert_eq!(rt.state.token_draft.text, "", "typing is ignored while disabled");
    assert!(rt.state.token_commits.is_empty());
    let (x, y) = rt.find("edge-token-0-remove").unwrap_or_else(|| panic!("remove laid out"));
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    rt.frame(0.0);
    assert!(rt.state.token_commits.is_empty(), "disabled remove never commits");
}

// ---- Single value input ------------------------------------------------------------------------

#[test]
fn value_input_keeps_its_key_placeholder_and_commits_on_enter() {
    let mut rt = value_rt(true, (1440.0, 816.0), "", false);
    assert!(has(&rt, "edge-value"), "editor key survives");
    assert!(has(&rt, "filter-value"), "React testid marker survives");
    assert_eq!(element(&rt, "edge-value").label.as_deref(), None);

    type_into(&mut rt, "edge-value", "hello");
    press(&mut rt, Key::Enter);
    assert_eq!(rt.state.value_commits, vec!["hello".to_owned()]);
    assert_eq!(rt.state.completes, 1);
}

#[test]
fn value_input_empty_enter_commits_but_does_not_complete() {
    let mut rt = value_rt(true, (1440.0, 816.0), "", false);
    click(&mut rt, "edge-value");
    press(&mut rt, Key::Enter);
    assert_eq!(rt.state.value_commits, vec![String::new()]);
    assert_eq!(rt.state.completes, 0, "an empty value does not collapse");
}

#[test]
fn disabled_value_input_ignores_typing() {
    let mut rt = value_rt(true, (1440.0, 816.0), "", true);
    assert!(!element(&rt, "edge-value").focusable, "disabled value field has no caret");
    type_into(&mut rt, "edge-value", "ignored");
    assert_eq!(rt.state.value.text, "");
    assert!(rt.state.value_commits.is_empty());
}

// ---- Geometry -----------------------------------------------------------------------------------

fn assert_rects_inside<S>(rt: &Runtime<S>, w: f32, h: f32, label: &str) {
    let mut outside = Vec::new();
    for e in rt.elements() {
        let r = e.rect;
        if r[0] < -0.5 || r[1] < -0.5 || r[0] + r[2] > w + 0.5 || r[1] + r[3] > h + 0.5 {
            outside.push(format!("{} {:?}", e.key.unwrap_or(e.path), r));
        }
    }
    assert!(outside.is_empty(), "{label}: {} node(s) outside {w}x{h}: {outside:?}", outside.len());
}

#[test]
fn both_inputs_fit_both_windows_in_dark_and_light() {
    for dark in [true, false] {
        for (w, h) in [(960.0, 640.0), (1440.0, 816.0)] {
            let token = token_rt(dark, (w, h), vec!["paid", "sent"], "pending", false);
            assert_rects_inside(&token, w, h, &format!("token dark={dark} {w}x{h}"));
            let value = value_rt(dark, (w, h), "a long filter value", false);
            assert_rects_inside(&value, w, h, &format!("value dark={dark} {w}x{h}"));
        }
    }
}
