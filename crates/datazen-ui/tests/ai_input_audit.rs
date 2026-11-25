//! T137 independent audit: the AI input, fresh vectors.
//!
//! The coder tests cover one scene per rule; this audit attacks from the
//! other side: unicode `@` queries, a mid-word caret pick, the Backspace key
//! path driven through the mounted field (not the state methods), stop
//! presses counted, the inert field without the state wire, and both
//! windows' geometry.

use rune::{col, Cx, El, Env, KeyEvent, Key, Mods, Runtime};

use datazen_ui::components::ai::ai_egress_notice::{ContextItem, ContextKind};
use datazen_ui::components::ai::ai_input::{at_query, item_key, AiInput, AiInputState};

const MIN: (f32, f32) = (960.0, 640.0);
const DESIGN: (f32, f32) = (1440.0, 816.0);

fn item(kind: ContextKind, id: &str, name: &str) -> ContextItem {
    ContextItem {
        kind,
        id: id.to_owned(),
        name: name.to_owned(),
    }
}

#[derive(Clone)]
struct Scene {
    input: AiInputState,
    submits: usize,
    stops: usize,
    items_writes: Vec<Vec<ContextItem>>,
}

impl Default for Scene {
    fn default() -> Self {
        Self {
            input: AiInputState::new(),
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
    col().key("audit-page").w_full().h_full().padding(24.0).child(input)
}

fn runtime(scene: Scene, size: (f32, f32), dark: bool) -> Runtime<Scene> {
    let mut rt = Runtime::new(scene, view, size, Env { system_dark: dark, scale: 1.0 });
    rt.frame(0.0);
    rt
}

fn has<S: 'static>(rt: &Runtime<S>, key: &str) -> bool {
    rt.elements().iter().any(|e| e.key.as_deref() == Some(key) && !e.exiting)
}

fn element<S: 'static>(rt: &Runtime<S>, key: &str) -> rune::ElementInfo {
    rt.elements()
        .into_iter()
        .find(|e| e.key.as_deref() == Some(key) && !e.exiting)
        .unwrap_or_else(|| panic!("{key}: is laid out"))
}

fn click(rt: &mut Runtime<Scene>, key: &str) {
    let (x, y) = rt.find(key).unwrap_or_else(|| panic!("{key}: is pressable"));
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    rt.frame(0.0);
}

fn press_key(rt: &mut Runtime<Scene>, key: Key) {
    rt.key(KeyEvent { key, mods: Mods::default() });
    rt.frame(0.0);
}

fn focus_field(rt: &mut Runtime<Scene>) {
    click(rt, "ai-input-text");
    // The focusable element is the kit's inner "input" child; the wrapper
    // is the hit target.
    for _ in 0..5 {
        if element(rt, "input").focused {
            return;
        }
        press_key(rt, Key::Tab);
    }
    assert!(
        element(rt, "input").focused,
        "the field takes the caret"
    );
}

#[test]
fn at_query_survives_unicode_and_symbol_carets() {
    // A CJK draft: the capture is the word before the caret, byte-safe.
    let text = "查询 @客户表";
    let caret = text.len();
    assert_eq!(at_query(text, caret).as_deref(), Some("客户表"));
    // Caret in the middle of the word: only the head counts.
    let head_cut = "查询 @客户".len();
    assert_eq!(at_query(text, head_cut).as_deref(), Some("客户"));
    // Symbols are ordinary capture characters, like React's `[^\s@]`.
    assert_eq!(at_query("@a.b-c_d", 8).as_deref(), Some("a.b-c_d"));
    // Consecutive `@`: the last one owns the capture.
    assert_eq!(at_query("@@x", 3).as_deref(), Some("x"));
    // A trailing space kills the trigger.
    assert_eq!(at_query("@x ", 3), None);
    // An empty head never triggers.
    assert_eq!(at_query("", 0), None);
}

#[test]
fn a_pick_with_a_mid_word_caret_strips_only_the_at_word() {
    let mut state = AiInputState::new();
    // The caret sits inside `@users`: `join @us|ers more`.
    state.set_text("join @users more");
    state.set_caret("join @us".len());

    state.apply_pick(item(ContextKind::Table, "t-users", "users"));

    // Before-the-@ + from-the-caret, blanks stripped: `join ers more`.
    assert_eq!(state.text(), "join ers more");
    assert_eq!(state.items().len(), 1);
    // The chip answers the React key shape.
    assert_eq!(item_key(&state.items()[0]), "Table:t-users");
}

#[test]
fn backspace_through_the_mounted_field_pops_the_last_chip() {
    let mut scene = Scene::default();
    scene.input.set_items(vec![
        item(ContextKind::Dir, "d1", "docs"),
        item(ContextKind::File, "f1", "notes.txt"),
    ]);
    // Caret 0 on an empty draft, both chips live.
    let mut rt = runtime(scene, DESIGN, true);
    focus_field(&mut rt);

    press_key(&mut rt, Key::Backspace);

    // The host's write arrived: one chip survives.
    assert_eq!(rt.state.items_writes.len(), 1);
    assert_eq!(rt.state.items_writes[0].len(), 1);
    assert_eq!(rt.state.input.items().len(), 1);

    // A second Backspace pops the last chip too.
    press_key(&mut rt, Key::Backspace);
    assert_eq!(rt.state.input.items().len(), 0);
    assert_eq!(rt.state.items_writes.len(), 2);

    // With no chips left, Backspace is an ordinary key: nothing is written.
    press_key(&mut rt, Key::Backspace);
    assert_eq!(rt.state.items_writes.len(), 2);
}

#[test]
fn the_stop_button_presses_reach_the_host() {
    let mut scene = Scene::default();
    scene.input.set_text("hello");
    let mut rt = Runtime::new(
        scene,
        |scene: &Scene, cx: &mut Cx<Scene>| {
            let dark = cx.system_dark();
            let input = AiInput::<Scene>::new(&scene.input)
                .state_access(|s: &mut Scene| &mut s.input)
                .on_stop(|s: &mut Scene| s.stops += 1)
                .loading(true)
                .build(dark);
            col().key("audit-page").w_full().h_full().padding(24.0).child(input)
        },
        DESIGN,
        Env { system_dark: true, scale: 1.0 },
    );
    rt.frame(0.0);

    click(&mut rt, "ai-input-stop");
    click(&mut rt, "ai-input-stop");
    assert_eq!(rt.state.stops, 2);
}

#[test]
fn a_field_without_the_state_wire_is_inert() {
    // React's controlled value / onChange are required; the port renders
    // the same dead control inert rather than focusable-but-dead.
    let empty = AiInputState::new();
    let mut rt = Runtime::new(
        (empty, false),
        |(input, _): &(AiInputState, bool), cx: &mut Cx<(AiInputState, bool)>| {
            let dark = cx.system_dark();
            let input = AiInput::<(AiInputState, bool)>::new(input).build(dark);
            col().key("audit-page").w_full().h_full().padding(24.0).child(input)
        },
        DESIGN,
        Env { system_dark: true, scale: 1.0 },
    );
    rt.frame(0.0);
    assert!(!element(&rt, "ai-input-text").focusable);
}

#[test]
fn three_chips_of_every_kind_render_their_names() {
    let mut scene = Scene::default();
    scene.input.set_items(vec![
        item(ContextKind::Table, "t1", "users"),
        item(ContextKind::Dir, "d1", "schema"),
        item(ContextKind::File, "f1", "notes.txt"),
    ]);
    scene.input.set_text("draft");
    let rt = runtime(scene, DESIGN, true);
    for (key, name) in [
        ("context-token-Table:t1", "@users"),
        ("context-token-Dir:d1", "@schema"),
        ("context-token-File:f1", "@notes.txt"),
    ] {
        assert!(has(&rt, key), "{key} is mounted");
        let text = element(&rt, format!("{key}-name").as_str()).text;
        assert_eq!(text.as_deref(), Some(name));
        assert!(has(&rt, format!("{key}-icon").as_str()));
    }
}

#[test]
fn the_input_fits_both_windows_in_both_themes_with_a_long_draft() {
    for dark in [true, false] {
        for (w, h) in [MIN, DESIGN] {
            let mut scene = Scene::default();
            scene
                .input
                .set_items(vec![item(ContextKind::File, "f1", "a-very-long-file-name.csv")]);
            scene.input.set_text(
                "a draft long enough to wrap several times across the field width",
            );
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
