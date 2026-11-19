//! T137: the AI input — coder tests.
//! Mounted probes plus the state machine's pure arms: the `@` trigger, the
//! token pop, the pick rewrite, the placeholder ternary, the send/stop gate.

use rune::{col, Cx, El, Env, Runtime};

use crate::components::ai::ai_egress_notice::{ContextItem, ContextKind};
use crate::components::ai::ai_input::{at_query, item_key, AiInput, AiInputState};
use crate::i18n::t;

const MIN: (f32, f32) = (960.0, 640.0);
const DESIGN: (f32, f32) = (1440.0, 816.0);

fn item(kind: ContextKind, id: &str) -> ContextItem {
    ContextItem {
        kind,
        id: id.to_owned(),
        name: id.to_owned(),
    }
}

#[derive(Clone)]
struct Scene {
    input: AiInputState,
    dark: bool,
    submits: usize,
    stops: usize,
    items_writes: Vec<Vec<ContextItem>>,
}

impl Default for Scene {
    fn default() -> Self {
        Self {
            input: AiInputState::new(),
            dark: true,
            submits: 0,
            stops: 0,
            items_writes: Vec::new(),
        }
    }
}

fn view(scene: &Scene, cx: &mut Cx<Scene>) -> El {
    let dark = cx.system_dark();
    let input = AiInput::<Scene>::new(&scene.input)
        .state_access(|s: &mut Scene| &mut s.input)
        .on_submit(|s: &mut Scene| s.submits += 1)
        .on_stop(|s: &mut Scene| s.stops += 1)
        .on_items_change(|s: &mut Scene, items| {
            s.items_writes.push(items.clone());
            s.input.set_items(items);
        })
        .build(dark);
    col().key("page").w_full().h_full().padding(24.0).child(input)
}

fn runtime(scene: Scene, size: (f32, f32), dark: bool) -> Runtime<Scene> {
    let mut rt = Runtime::new(scene, view, size, Env { system_dark: dark, scale: 1.0 });
    rt.frame(0.0);
    rt
}

fn has<S: 'static>(rt: &Runtime<S>, key: &str) -> bool {
    rt.elements().iter().any(|e| e.key.as_deref() == Some(key) && !e.exiting)
}

// ---- the `@` trigger --------------------------------------------------------

#[test]
fn the_at_trigger_matches_the_react_regex() {
    // `@word` right before the caret.
    assert_eq!(at_query("hello @ta", 10).as_deref(), Some("ta"));
    // A bare trailing `@`: the capture is empty, the picker still opens.
    assert_eq!(at_query("hello @", 7).as_deref(), Some(""));
    // No `@` at the end of the head.
    assert_eq!(at_query("@ta and more", 13), None);
    // The caret may sit mid-text; only the head counts.
    assert_eq!(at_query("@ta|x tail", 3).as_deref(), Some("ta"));
    assert_eq!(at_query("word word", 9), None);
    // The capture stops at whitespace; a second `@` starts a NEW capture
    // (React's `[^\s@]*` cannot cross it): `@a@b` captures `b`.
    assert_eq!(at_query("@a@b", 4).as_deref(), Some("b"));
    // A caret past the end is clamped.
    assert_eq!(at_query("@end", 99).as_deref(), Some("end"));
}

#[test]
fn sync_picker_opens_only_for_a_live_query() {
    let mut state = AiInputState::new();
    state.set_text("check @us");
    let query = state.sync_picker();
    assert_eq!(query.as_deref(), Some("us"));
    assert!(state.picker_open());

    // React's else branch closes the picker and clears the query.
    state.set_text("no trigger");
    assert_eq!(state.sync_picker(), None);
    assert!(!state.picker_open());
}

// ---- the token pop ----------------------------------------------------------

#[test]
fn backspace_at_the_start_pops_the_last_chip() {
    let mut scene = Scene::default();
    scene.input.set_items(vec![
        item(ContextKind::Table, "t1"),
        item(ContextKind::File, "f1"),
    ]);
    scene.input.set_text("");
    // The conditions React checks: caret at 0, selection empty, chips there.
    assert!(scene.input.pop_token_conditions());

    let popped = scene.input.pop_token();
    assert_eq!(
        popped.map(|items| items.len()),
        Some(1),
        "one chip survives the pop"
    );
    // The pop conditions hold while any chip remains at the caret.
    assert!(scene.input.pop_token_conditions());
    scene.input.pop_token();
    // An empty item list pops nothing, and the conditions drop with it.
    assert!(!scene.input.pop_token_conditions());
    assert_eq!(scene.input.pop_token(), None);
}

// ---- the pick rewrite -------------------------------------------------------

#[test]
fn apply_pick_appends_once_and_strips_the_at_word() {
    let mut state = AiInputState::new();
    state.set_text("join @us please");
    // React's caret is after the `@us` word: 8 + "please" behind it.
    state.set_caret(8);

    state.apply_pick(item(ContextKind::Table, "users"));

    assert_eq!(state.text(), "join please");
    assert_eq!(state.items().len(), 1);
    assert_eq!(item_key(&state.items()[0]), "Table:users");
    assert!(!state.picker_open());

    // The same pick twice does not duplicate the chip.
    state.set_text("again @x");
    state.set_caret(8);
    state.apply_pick(item(ContextKind::Table, "users"));
    assert_eq!(state.items().len(), 1);

    // An all-blank rewrite becomes an empty draft.
    state.set_text("@y  ");
    state.set_caret(2);
    state.apply_pick(item(ContextKind::Dir, "docs"));
    assert_eq!(state.text(), "");
    assert_eq!(state.items().len(), 2);
}

// ---- the render ------------------------------------------------------------

#[test]
fn chips_and_the_field_share_the_wrapping_line() {
    let mut scene = Scene::default();
    scene.input.set_items(vec![
        item(ContextKind::Table, "users"),
        item(ContextKind::Dir, "docs"),
        item(ContextKind::File, "notes"),
    ]);
    scene.input.set_text("hello");
    let rt = runtime(scene, DESIGN, true);

    assert!(has(&rt, "ai-input"));
    assert!(has(&rt, "ai-input-field"));
    assert!(has(&rt, "context-token-Table:users"));
    assert!(has(&rt, "context-token-Dir:docs"));
    assert!(has(&rt, "context-token-File:notes"));
    assert!(has(&rt, "ai-input-text"));
    assert!(has(&rt, "ai-input-submit"));
    // The names render with the `@` sigil.
    let name = rt
        .elements()
        .into_iter()
        .find(|e| e.key.as_deref() == Some("context-token-Table:users-name"))
        .and_then(|e| e.text);
    assert_eq!(name.as_deref(), Some("@users"));
}

#[test]
fn the_placeholder_follows_the_react_ternary() {
    // The ternary is a pure function of (placeholder prop, has context,
    // chips, draft); the kit keeps the drawn hint inside its text-input
    // state, out of the elements API, so the arms assert here.
    let empty = AiInputState::new();
    let base = AiInput::<Scene>::new(&empty);
    assert_eq!(
        base.placeholder_text(),
        None,
        "no placeholder prop, no context machinery: nothing"
    );

    let hinted = AiInput::<Scene>::new(&empty)
        .on_items_change(|_: &mut Scene, _| {});
    assert_eq!(hinted.placeholder_text().as_deref(), Some(t("context.placeholder")));

    let custom = AiInput::<Scene>::new(&empty)
        .placeholder("Generate SQL…");
    assert_eq!(custom.placeholder_text().as_deref(), Some("Generate SQL…"));

    // The custom text wins over the hint too.
    let custom_hinted = AiInput::<Scene>::new(&empty)
        .placeholder("Generate SQL…")
        .on_items_change(|_: &mut Scene, _| {});
    assert_eq!(custom_hinted.placeholder_text().as_deref(), Some("Generate SQL…"));

    // A draft or a chip suppresses the placeholder entirely.
    let mut drafted = AiInputState::new();
    drafted.set_text("draft");
    let drafted = AiInput::<Scene>::new(&drafted).placeholder("Generate SQL…");
    assert_eq!(drafted.placeholder_text(), None);

    let mut chipped = AiInputState::new();
    chipped.set_items(vec![item(ContextKind::Table, "users")]);
    let chipped = AiInput::<Scene>::new(&chipped)
        .on_items_change(|_: &mut Scene, _| {});
    assert_eq!(chipped.placeholder_text(), None);
}

#[test]
fn the_send_button_gates_on_a_non_blank_draft_and_the_run_state() {
    // A blank draft cannot send.
    let rt = runtime(Scene::default(), DESIGN, true);
    let send = rt
        .elements()
        .into_iter()
        .find(|e| e.key.as_deref() == Some("ai-input-submit"))
        .unwrap();
    assert!(!send.focusable);

    // A live draft can.
    let mut scene = Scene::default();
    scene.input.set_text("hello");
    let rt = runtime(scene, DESIGN, true);
    let send = rt
        .elements()
        .into_iter()
        .find(|e| e.key.as_deref() == Some("ai-input-submit"))
        .unwrap();
    assert!(send.focusable);

    // While loading, the send button gives way to the stop button.
    let mut scene = Scene::default();
    scene.input.set_text("hello");
    let mut rt = Runtime::new(
        scene,
        |scene: &Scene, cx: &mut Cx<Scene>| {
            let dark = cx.system_dark();
            let input = AiInput::<Scene>::new(&scene.input)
                .on_stop(|s: &mut Scene| s.stops += 1)
                .loading(true)
                .build(dark);
            col().key("page").w_full().h_full().padding(24.0).child(input)
        },
        DESIGN,
        Env { system_dark: true, scale: 1.0 },
    );
    rt.frame(0.0);
    assert!(has(&rt, "ai-input-stop"));
    assert!(!has(&rt, "ai-input-submit"));
}

#[test]
fn hide_submit_hides_the_control_but_not_the_field() {
    let mut scene = Scene::default();
    scene.input.set_text("hello");
    let mut rt = Runtime::new(
        scene,
        |scene: &Scene, cx: &mut Cx<Scene>| {
            let dark = cx.system_dark();
            let input = AiInput::<Scene>::new(&scene.input)
                .hide_submit(true)
                .build(dark);
            col().key("page").w_full().h_full().padding(24.0).child(input)
        },
        DESIGN,
        Env { system_dark: true, scale: 1.0 },
    );
    rt.frame(0.0);
    assert!(has(&rt, "ai-input-field"));
    assert!(!has(&rt, "ai-input-submit"));
    assert!(!has(&rt, "ai-input-stop"));
}

#[test]
fn a_disabled_input_paints_at_half_opacity_and_inert() {
    let mut scene = Scene::default();
    scene.input.set_text("hello");
    let mut rt = Runtime::new(
        scene,
        |scene: &Scene, cx: &mut Cx<Scene>| {
            let dark = cx.system_dark();
            let input = AiInput::<Scene>::new(&scene.input)
                .disabled(true)
                .build(dark);
            col().key("page").w_full().h_full().padding(24.0).child(input)
        },
        DESIGN,
        Env { system_dark: true, scale: 1.0 },
    );
    rt.frame(0.0);
    let field = rt
        .elements()
        .into_iter()
        .find(|e| e.key.as_deref() == Some("ai-input-field"))
        .unwrap();
    assert_eq!(field.opacity, 0.5);
    // React's disabled textarea: the field never takes focus.
    let text = rt
        .elements()
        .into_iter()
        .find(|e| e.key.as_deref() == Some("ai-input-text"))
        .unwrap();
    assert!(!text.focusable);
}

#[test]
fn the_input_fits_both_windows_in_both_themes() {
    for dark in [true, false] {
        for (w, h) in [MIN, DESIGN] {
            let mut scene = Scene::default();
            scene.input.set_items(vec![
                item(ContextKind::Table, "users"),
                item(ContextKind::File, "a-long-file-name.png"),
            ]);
            scene.input.set_text("a draft with several words in it");
            let rt = runtime(scene, (w, h), dark);
            let outside: Vec<String> = rt
                .elements()
                .iter()
                .filter(|e| {
                    let r = e.rect;
                    !e.virtual_extent
                        && (r[0] < -0.5
                            || r[1] < -0.5
                            || r[0] + r[2] > w + 0.5
                            || r[1] + r[3] > h + 0.5)
                })
                .map(|e| format!("{:?}", e.key.clone().unwrap_or(e.path.clone())))
                .collect();
            assert!(
                outside.is_empty(),
                "dark={dark} {w}x{h}: out of window: {outside:?}"
            );
        }
    }
}

#[test]
fn the_placeholder_hint_names_the_at_trigger() {
    // The i18n hint is the one React renders for the context machinery.
    assert!(t("context.placeholder").contains('@'));
}
