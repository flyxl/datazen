//! T166 independent audit: `FilterTokenInput` + `FilterValueInput`.
//!
//! Probes React-documented branches the coder's suite does not: empty Enter /
//! comma, Backspace with a non-empty draft or no tokens, duplicate-token
//! removal by position, unchanged-value Enter, many-token geometry, and the
//! removal of T056's local input stand-ins.

use rune::{Cx, El, Env, Key, KeyEvent, Mods, Runtime, TextEdit};
use rune_kit::root;

use crate::components::filter::{FilterTokenInput, FilterValueInput};
use crate::i18n::t;

#[derive(Default)]
struct Probe {
    dark: bool,
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
        .on_commit(|st: &mut Probe, value: String| st.value_commits.push(value))
        .on_enter_complete(|st: &mut Probe| st.completes += 1);
    root(widget.build(s.dark), Vec::new())
}

fn token_rt(tokens: Vec<&str>, draft: &str, size: (f32, f32)) -> Runtime<Probe> {
    let mut state = Probe {
        dark: true,
        tokens: tokens.into_iter().map(str::to_owned).collect(),
        token_draft: TextEdit::new(draft.to_owned()),
        ..Probe::default()
    };
    state.dark = true;
    let mut rt = Runtime::new(
        state,
        token_view,
        size,
        Env {
            system_dark: true,
            scale: 1.0,
        },
    );
    rt.frame(0.0);
    rt
}

fn value_rt(draft: &str) -> Runtime<Probe> {
    let mut state = Probe {
        dark: true,
        value: TextEdit::new(draft.to_owned()),
        ..Probe::default()
    };
    state.dark = true;
    let mut rt = Runtime::new(
        state,
        value_view,
        (960.0, 640.0),
        Env {
            system_dark: true,
            scale: 1.0,
        },
    );
    rt.frame(0.0);
    rt
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

/// React `commitText` trims and refuses an empty token for both Enter and
/// comma; neither path may commit or complete.
#[test]
fn empty_enter_and_comma_do_not_commit_or_complete() {
    let mut rt = token_rt(vec!["paid"], "   ", (960.0, 640.0));
    click(&mut rt, "edge-token-field");
    press(&mut rt, Key::Enter);
    press(&mut rt, Key::Text(",".to_owned()));
    assert!(rt.state.token_commits.is_empty(), "whitespace-only never commits");
    assert_eq!(rt.state.completes, 0, "no completion for empty commits");
    assert_eq!(rt.state.token_draft.text, "   ", "empty drafts are not cleared");
}

/// Backspace only owns deletion when the field is empty; with text present it
/// must fall through to normal text deletion and leave the token list alone.
#[test]
fn backspace_with_text_or_no_tokens_does_not_delete_a_chip() {
    let mut rt = token_rt(vec!["paid", "sent"], "draft", (960.0, 640.0));
    click(&mut rt, "edge-token-field");
    press(&mut rt, Key::Backspace);
    assert!(rt.state.token_commits.is_empty(), "non-empty field keeps the chips");

    let mut rt = token_rt(vec![], "", (960.0, 640.0));
    click(&mut rt, "edge-token-field");
    press(&mut rt, Key::Backspace);
    assert!(rt.state.token_commits.is_empty(), "no tokens to delete");
}

/// Removal is by array position, not by string value: duplicate token strings
/// must keep the correct remaining copies.
#[test]
fn removing_duplicate_tokens_is_by_position() {
    let mut rt = token_rt(vec!["dup", "x", "dup"], "", (960.0, 640.0));
    click(&mut rt, "edge-token-2-remove");
    assert_eq!(rt.state.token_commits, vec![vec!["dup".to_owned(), "x".to_owned()]]);

    let mut rt = token_rt(vec!["dup", "x", "dup"], "", (960.0, 640.0));
    click(&mut rt, "edge-token-0-remove");
    assert_eq!(rt.state.token_commits, vec![vec!["x".to_owned(), "dup".to_owned()]]);
}

/// React `flush` commits only when the value changed; an Enter on a draft that
/// equals the current value must not emit a redundant commit, though a
/// non-empty value still completes.
#[test]
fn value_enter_with_unchanged_draft_does_not_recommit() {
    let mut rt = value_rt("hello");
    click(&mut rt, "edge-value");
    press(&mut rt, Key::Enter);
    assert!(
        rt.state.value_commits.is_empty(),
        "React flush skips an unchanged value, got {:?}",
        rt.state.value_commits
    );
    assert_eq!(rt.state.completes, 1, "a non-empty value still completes");
}

/// A comma-containing token survives the round trip (the list is never joined
/// into a comma string).
#[test]
fn token_containing_comma_is_committed_verbatim() {
    let mut rt = token_rt(vec![], "a,b", (960.0, 640.0));
    click(&mut rt, "edge-token-field");
    press(&mut rt, Key::Enter);
    assert_eq!(rt.state.token_commits, vec![vec!["a,b".to_owned()]]);
    assert_eq!(rt.state.token_draft.text, "");
}

/// T056's editor must delegate to the real ports, not keep local chip/field
/// input logic (the local stand-ins were removed in T166).
#[test]
fn editor_no_longer_carries_local_input_stand_ins() {
    let src = include_str!("../components/filter/filter_condition_editor.rs");
    for removed in [
        "fn single_value",
        "fn token_value",
        "fn token_chip",
        "fn commit_token",
        "fn remove_token",
    ] {
        assert!(!src.contains(removed), "{removed} local stand-in must be removed");
    }
    assert!(src.contains("FilterTokenInput::<S>::new"), "editor mounts the real token port");
    assert!(src.contains("FilterValueInput::<S>::new"), "editor mounts the real value port");
}

/// Many long chips must not push any laid out node outside either window, in
/// both palettes (React wraps the chip list; Rune must still fit).
#[test]
fn many_token_chips_stay_inside_both_windows_both_palettes() {
    for dark in [true, false] {
        for (w, h) in [(960.0, 640.0), (1440.0, 816.0)] {
            let mut state = Probe {
                dark,
                tokens: (0..8).map(|i| format!("token-number-{i}-with-a-long-name")).collect(),
                token_draft: TextEdit::new("pending".to_owned()),
                ..Probe::default()
            };
            state.dark = dark;
            let mut rt = Runtime::new(
                state,
                token_view,
                (w, h),
                Env {
                    system_dark: dark,
                    scale: 1.0,
                },
            );
            rt.frame(0.0);
            let mut outside = Vec::new();
            for e in rt.elements() {
                let r = e.rect;
                if r[0] < -0.5 || r[1] < -0.5 || r[0] + r[2] > w + 0.5 || r[1] + r[3] > h + 0.5 {
                    outside.push(format!("{} {:?}", e.key.unwrap_or(e.path), r));
                }
            }
            assert!(
                outside.is_empty(),
                "many tokens dark={dark} {w}x{h}: {} node(s) outside: {outside:?}",
                outside.len()
            );
        }
    }
}
