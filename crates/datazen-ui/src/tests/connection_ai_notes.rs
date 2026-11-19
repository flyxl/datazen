//! T059: independent, real tests for `ConnectionAiNotes`.
//!
//! The observation channel is the raw runtime tree (`elements()` / `find()` /
//! rects) and the injected fake port, never the component's own helpers. The
//! timed `saved` feedback is pinned through `Runtime::wake_deadline()` exactly
//! as `copyable_timer_audit` pins the 1500ms copy feedback: deleting the
//! `tx.wake_after(2s)` in `AiNotesState::apply` makes
//! `rt.wake_deadline().expect(..)` panic, turning the transition test red.

use rune::{Cx, El, Env, Runtime, TextEdit, Tx};

use crate::components::connection::connection_ai_notes::{
    AiNotesState, NotesAction, NotesEvent, NotesPort, MAX_NOTE_BYTES, SAVED_FEEDBACK_MS,
};
use crate::i18n::t;

const CONNECTION_ID: &str = "conn-ai-1";

fn env(dark: bool) -> Env {
    Env { system_dark: dark, scale: 1.0 }
}

/// A host port that records requests and lets the test decide the answer. No
/// production fake exists: this one is test-only and reports honest counts.
#[derive(Default)]
struct FakePort {
    gets: Vec<(String, u64)>,
    sets: Vec<(String, String, u64)>,
    load: Option<Result<String, String>>,
    save: Option<Result<(), String>>,
}

// The injected fake is a real implementation of the component's host contract,
// so the load/save paths are exercised through the port, not a parallel shim.
impl NotesPort for FakePort {
    fn get(&mut self, connection_id: &str, epoch: u64) {
        self.gets.push((connection_id.to_owned(), epoch));
    }

    fn set(&mut self, connection_id: &str, content: &str, epoch: u64) {
        self.sets.push((connection_id.to_owned(), content.to_owned(), epoch));
    }
}

#[derive(Default)]
struct Probe {
    notes: Option<AiNotesState>,
    port: FakePort,
    actions: Vec<NotesAction>,
    /// Simulates the host removing the component from the tree (React unmount).
    show: bool,
}

impl Probe {
    fn new() -> Self {
        Self {
            notes: Some(AiNotesState::new(CONNECTION_ID)),
            show: true,
            ..Self::default()
        }
    }
}

fn apply_action(s: &mut Probe, action: NotesAction) {
    let record = action.clone();
    match action {
        NotesAction::Mount { connection_id } => {
            // The owner is the final guard against a duplicate layout effect.
            if s.notes.as_ref().expect("state").needs_mount() {
                let epoch = s.notes.as_mut().expect("state").on_mount(&connection_id);
                NotesPort::get(&mut s.port, &connection_id, epoch);
            }
        }
        NotesAction::Unmount => {
            s.notes.as_mut().expect("state").on_unmount();
        }
        NotesAction::Save { connection_id, content } => {
            let epoch = s.notes.as_mut().expect("state").begin_save();
            NotesPort::set(&mut s.port, &connection_id, &content, epoch);
        }
    }
    s.actions.push(record);
}

fn note(s: &Probe) -> &AiNotesState {
    s.notes.as_ref().expect("state")
}

fn view(s: &Probe, _cx: &mut Cx<Probe>) -> El {
    let widget = if s.show {
        let state = s.notes.as_ref().expect("state");
        crate::components::connection::connection_ai_notes::ConnectionAiNotes::<Probe>::new(state)
            .on_edit(rune::edit_access(|s: &mut Probe| &mut s.notes.as_mut().expect("state").content))
            .on_action(apply_action)
            .build(true)
    } else {
        rune::col().key("connection-ai-notes-unmounted").w_full().h_full()
    };
    rune_kit::root(widget, Vec::new())
}

/// The poll is where a real host would deliver async results and step the
/// saved deadline. It is set on every runtime in this file.
fn poll(s: &mut Probe, now: f64, tx: &mut Tx) -> bool {
    let mut dirty = false;
    if let Some(result) = s.port.load.take() {
        let epoch = s.port.gets.last().map_or(0, |entry| entry.1);
        dirty |= s.notes.as_mut().expect("state").apply(
            NotesEvent::Loaded { epoch, result },
            now,
            tx,
        );
    }
    if let Some(result) = s.port.save.take() {
        let epoch = s.port.sets.last().map_or(0, |entry| entry.2);
        dirty |= s.notes.as_mut().expect("state").apply(
            NotesEvent::Saved { epoch, result },
            now,
            tx,
        );
    }
    dirty |= s.notes.as_mut().expect("state").tick(now);
    dirty
}

fn runtime(probe: Probe, size: (f32, f32)) -> Runtime<Probe> {
    let mut rt = Runtime::new(probe, view, size, env(true));
    rt.poll = Some(poll);
    // `on_layout` reports are queued during a frame's layout and delivered on
    // the *next* frame. The first frame binds the mount handler; the second
    // delivers it, which stages the host `connectionNotesGet` (React's effect).
    rt.frame(0.0);
    rt.frame(0.0);
    rt
}

fn element(rt: &Runtime<Probe>, key: &str) -> rune::ElementInfo {
    rt.elements()
        .into_iter()
        .find(|e| e.key.as_deref() == Some(key))
        .unwrap_or_else(|| panic!("{key} is in the laid-out tree"))
}

fn text_of(rt: &Runtime<Probe>, key: &str) -> String {
    element(rt, key).text.unwrap_or_default()
}

fn label_of(rt: &Runtime<Probe>, key: &str) -> String {
    element(rt, key).label.unwrap_or_default()
}

fn click(rt: &mut Runtime<Probe>, key: &str) {
    let (x, y) = rt.find(key).unwrap_or_else(|| panic!("{key} is clickable"));
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    rt.frame(1.0 / 60.0);
}

/// Drive the mount effect + a successful load so the editor is usable.
fn mount_and_load(rt: &mut Runtime<Probe>, content: &str) {
    assert_eq!(rt.state.port.gets.len(), 1, "first layout staged one get");
    assert_eq!(rt.state.port.gets[0].0, CONNECTION_ID);
    rt.state.port.load = Some(Ok(content.to_owned()));
    rt.frame(0.05);
    assert_eq!(text_of(rt, "connection-notes-input"), content);
}

// ---------------------------------------------------------------------------
// 1. Load: success / error / cancelled-by-unmount.
// ---------------------------------------------------------------------------

#[test]
fn load_success_populates_the_multiline_value_and_clears_loading() {
    let mut rt = runtime(Probe::new(), (900.0, 520.0));
    assert_eq!(text_of(&rt, "connection-notes-input"), "", "empty before load");
    assert!(element(&rt, "connection-notes-input").focusable == false, "disabled while loading");
    assert_eq!(label_of(&rt, "connection-notes-save"), t("common.save"));

    mount_and_load(&mut rt, "line one\nline two");

    assert!(note(&rt.state).loading == false);
    // Multiline text stays in the value (the field is not single_line).
    assert_eq!(text_of(&rt, "connection-notes-input"), "line one\nline two");
    assert!(element(&rt, "connection-notes-input").focusable, "editable once loaded");
    // "line one\nline two" = 8 + 1 + 8 = 17 UTF-8 bytes.
    assert_eq!(text_of(&rt, "connection-notes-count"), "17 / 4096 bytes");
}

#[test]
fn load_error_shows_the_host_message_and_keeps_an_empty_value() {
    let mut rt = runtime(Probe::new(), (900.0, 520.0));
    rt.state.port.load = Some(Err("notes unavailable".to_owned()));
    rt.frame(0.05);

    assert!(note(&rt.state).loading == false);
    assert_eq!(text_of(&rt, "connection-notes-input"), "", "React keeps the old value on error");
    assert_eq!(text_of(&rt, "connection-notes-error"), "notes unavailable");
    assert_eq!(note(&rt.state).error, "notes unavailable");
}

#[test]
fn a_load_result_after_unmount_is_discarded() {
    let mut rt = runtime(Probe::new(), (900.0, 520.0));
    let stale_epoch = rt.state.port.gets[0].1;
    // React's effect cleanup: the component goes away before the promise settles.
    rt.state.show = false;
    rt.state.notes.as_mut().expect("state").on_unmount();
    rt.frame(0.0);
    rt.state.port.load = Some(Ok("late answer".to_owned()));
    rt.frame(0.05);

    assert_eq!(note(&rt.state).content.text, "", "stale result never wrote content");
    assert_eq!(note(&rt.state).error, "");
    assert_eq!(rt.state.port.gets.len(), 1);
    assert!(stale_epoch > 0, "mount minted a real epoch");
}

// ---------------------------------------------------------------------------
// 2. UTF-8 byte count, the exact 4096/4097 threshold, and save blocking.
// ---------------------------------------------------------------------------

#[test]
fn byte_count_is_utf8_bytes_for_ascii_cjk_and_emoji() {
    let mut rt = runtime(Probe::new(), (900.0, 520.0));
    mount_and_load(&mut rt, "abc");
    assert_eq!(text_of(&rt, "connection-notes-count"), "3 / 4096 bytes");

    // "中" is three UTF-8 bytes, "😀" is four, "é" is two; chars would be 3.
    let mixed = "中😀é";
    assert_eq!(mixed.chars().count(), 3);
    assert_eq!(mixed.len(), 9);
    rt.update(|s, _tx| {
        s.notes.as_mut().expect("state").content = TextEdit::new(mixed.to_owned());
    });
    rt.frame(1.0 / 60.0);
    assert_eq!(text_of(&rt, "connection-notes-count"), "9 / 4096 bytes");
    assert!(!note(&rt.state).over_limit());
}

/// A fresh runtime whose load delivers `content` as the saved note.
fn runtime_with_notes(content: &str) -> Runtime<Probe> {
    let mut rt = runtime(Probe::new(), (900.0, 520.0));
    assert_eq!(rt.state.port.gets.len(), 1, "mount staged one get");
    rt.state.port.load = Some(Ok(content.to_owned()));
    rt.frame(0.05);
    rt
}

#[test]
fn the_over_limit_threshold_is_exactly_4096_bytes() {
    let mut rt = runtime_with_notes(&"a".repeat(MAX_NOTE_BYTES));
    assert!(!note(&rt.state).over_limit(), "4096 bytes is within the limit");
    assert_eq!(text_of(&rt, "connection-notes-count"), "4096 / 4096 bytes");
    assert!(!text_of(&rt, "connection-notes-count").contains(t("connection.notes.overLimit")));
    // Text colour springs from the previous value; let it settle before reading.
    for _ in 0..20 { rt.frame(1.0 / 60.0); }
    assert_eq!(
        element(&rt, "connection-notes-count").text_color,
        Some(crate::ui::kit::kit(true).c.text_muted)
    );
    assert!(element(&rt, "connection-notes-save").label.is_some());

    let mut rt = runtime_with_notes(&"a".repeat(MAX_NOTE_BYTES + 1));
    assert!(note(&rt.state).over_limit(), "4097 bytes is over the limit");
    let count = text_of(&rt, "connection-notes-count");
    assert_eq!(count, format!("4097 / 4096 bytes · {}", t("connection.notes.overLimit")));
    for _ in 0..20 { rt.frame(1.0 / 60.0); }
    assert_eq!(
        element(&rt, "connection-notes-count").text_color,
        Some(crate::ui::kit::kit(true).c.danger.fg),
        "over-limit count uses the danger token"
    );
    // The save button is disabled and no press handler was bound.
    let before = rt.state.port.sets.len();
    click(&mut rt, "connection-notes-save");
    assert_eq!(rt.state.port.sets.len(), before, "over-limit save is a no-op");
    assert!(note(&rt.state).saved_until.is_none());
}

#[test]
fn no_set_call_while_loading() {
    let mut rt = runtime(Probe::new(), (900.0, 520.0));
    assert!(note(&rt.state).loading, "load is still pending");
    let before = rt.state.port.sets.len();
    click(&mut rt, "connection-notes-save");
    assert_eq!(rt.state.port.sets.len(), before);
    assert_eq!(rt.state.actions.iter().filter(|a| matches!(a, NotesAction::Save { .. })).count(), 0);
    assert!(!note(&rt.state).saving);
}

// ---------------------------------------------------------------------------
// 3. Save: saving -> saved -> (deadline) -> save, error path, real wake.
// ---------------------------------------------------------------------------

#[test]
fn save_success_uses_a_real_two_second_wake_and_reverts_at_the_deadline() {
    let mut rt = runtime(Probe::new(), (900.0, 520.0));
    mount_and_load(&mut rt, "note");

    click(&mut rt, "connection-notes-save");
    assert_eq!(rt.state.port.sets.len(), 1, "one set staged");
    assert_eq!(rt.state.port.sets[0].1, "note");
    assert_eq!(rt.state.port.sets[0].0, CONNECTION_ID);
    assert!(note(&rt.state).saving);
    assert_eq!(label_of(&rt, "connection-notes-save"), t("settings.ai.saving"));
    assert!(!note(&rt.state).saved(), "not saved until the host answers");

    rt.state.port.save = Some(Ok(()));
    rt.frame(0.05);
    assert!(!note(&rt.state).saving);
    let until = note(&rt.state).saved_until.expect("success stamped a deadline");
    assert_eq!(label_of(&rt, "connection-notes-save"), t("common.success"));

    // THE pinned wake. If `tx.wake_after(Duration::from_millis(2000))` is
    // removed from AiNotesState::apply, wake_deadline() is None and this
    // expect panics => the mutation turns this test red.
    let deadline = rt
        .wake_deadline()
        .expect("a successful save must schedule a real timed wake for the saved label");
    let delta = deadline - until;
    assert!(
        delta.abs() <= 0.01,
        "wake must land on the saved deadline: deadline={deadline} until={until} delta={delta}"
    );
    assert_eq!(SAVED_FEEDBACK_MS, 2000, "React's setTimeout is 2000ms");

    // Just before the deadline the label is still the success one.
    rt.frame(1.90);
    assert_eq!(label_of(&rt, "connection-notes-save"), t("common.success"));

    // At/after the deadline the poll's tick clears it and the frame repaints.
    rt.frame(0.20);
    assert!(note(&rt.state).saved_until.is_none(), "deadline was consumed");
    assert_eq!(label_of(&rt, "connection-notes-save"), t("common.save"));
}

#[test]
fn save_error_shows_the_message_and_never_enters_saved() {
    let mut rt = runtime(Probe::new(), (900.0, 520.0));
    mount_and_load(&mut rt, "note");

    click(&mut rt, "connection-notes-save");
    assert_eq!(label_of(&rt, "connection-notes-save"), t("settings.ai.saving"));
    rt.state.port.save = Some(Err("write rejected".to_owned()));
    rt.frame(0.05);

    assert!(!note(&rt.state).saving);
    assert!(note(&rt.state).saved_until.is_none());
    assert_eq!(label_of(&rt, "connection-notes-save"), t("common.save"));
    assert_eq!(text_of(&rt, "connection-notes-error"), "write rejected");
    assert_eq!(note(&rt.state).error, "write rejected");
}

// ---------------------------------------------------------------------------
// 4. Keys, editability and rect-based geometry at both required sizes.
// ---------------------------------------------------------------------------

#[test]
fn the_four_required_keys_are_in_the_tree_and_the_textarea_edits() {
    let mut rt = runtime(Probe::new(), (900.0, 520.0));
    mount_and_load(&mut rt, "");

    for key in [
        "connection-ai-notes",
        "connection-notes-input",
        "connection-notes-count",
        "connection-notes-save",
    ] {
        let e = element(&rt, key);
        assert!(!e.exiting, "{key} is live");
        assert!(e.rect[2] > 0.0 && e.rect[3] > 0.0, "{key} has a non-zero rect: {:?}", e.rect);
    }
    assert!(element(&rt, "connection-notes-input").focusable, "textarea takes focus");
    assert_eq!(element(&rt, "connection-notes-input").role, "textbox");

    // Click into the textarea and type; the binding writes back into state and
    // the byte count follows (an independent path, not a helper assertion).
    let (x, y) = rt.find("connection-notes-input").expect("textarea is findable");
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    rt.paste("typed");
    rt.frame(1.0 / 60.0);
    assert_eq!(note(&rt.state).content.text, "typed");
    assert_eq!(text_of(&rt, "connection-notes-input"), "typed");
    assert_eq!(text_of(&rt, "connection-notes-count"), "5 / 4096 bytes");
}

fn assert_inside(rt: &Runtime<Probe>, key: &str, size: (f32, f32)) {
    let e = element(rt, key);
    let [x, y, w, h] = e.rect;
    assert!(x >= -0.5 && y >= -0.5, "{key} starts inside the window: {:?}", e.rect);
    assert!(
        x + w <= size.0 + 0.5 && y + h <= size.1 + 0.5,
        "{key} ends inside {size:?}: {:?}",
        e.rect
    );
}

#[test]
fn every_key_stays_inside_both_required_windows() {
    for size in [(960.0, 640.0), (1440.0, 816.0)] {
        let mut rt = runtime(Probe::new(), size);
        mount_and_load(&mut rt, "a short note");
        for key in [
            "connection-ai-notes",
            "connection-notes-title",
            "connection-notes-hint",
            "connection-notes-input",
            "connection-notes-count",
            "connection-notes-save",
        ] {
            assert_inside(&rt, key, size);
        }
        let input = element(&rt, "connection-notes-input");
        assert!(input.rect[3] >= 79.5, "textarea honours min-h-[80px]: {:?}", input.rect);
    }
}

// ---------------------------------------------------------------------------
// 5. Independent-audit additions: light-mode geometry, precise deadline,
//    no-set-while-saving, 4-byte CJK, and error-element-only-on-error.
// ---------------------------------------------------------------------------

/// Same tree as `view` but built for the light palette. The coder's harness
/// hardcodes `build(true)`/`env(true)`, so light mode was never exercised.
fn view_light(s: &Probe, _cx: &mut Cx<Probe>) -> El {
    let widget = if s.show {
        let state = s.notes.as_ref().expect("state");
        crate::components::connection::connection_ai_notes::ConnectionAiNotes::<Probe>::new(state)
            .on_edit(rune::edit_access(|s: &mut Probe| &mut s.notes.as_mut().expect("state").content))
            .on_action(apply_action)
            .build(false)
    } else {
        rune::col().key("connection-ai-notes-unmounted").w_full().h_full()
    };
    rune_kit::root(widget, Vec::new())
}

fn runtime_light(probe: Probe, size: (f32, f32)) -> Runtime<Probe> {
    let mut rt = Runtime::new(probe, view_light, size, env(false));
    rt.poll = Some(poll);
    rt.frame(0.0);
    rt.frame(0.0);
    rt
}

fn has_key(rt: &Runtime<Probe>, key: &str) -> bool {
    rt.elements().iter().any(|e| e.key.as_deref() == Some(key) && !e.exiting)
}

#[test]
fn light_palette_renders_every_key_inside_both_required_windows() {
    // Prove the light branch actually rendered by checking a light-palette
    // text colour, not just that rects happen to fit.
    let light_title = crate::ui::kit::kit(false).c.text_muted;
    let dark_title = crate::ui::kit::kit(true).c.text_muted;
    assert_ne!(light_title, dark_title, "kit exposes distinct palettes");

    for size in [(960.0, 640.0), (1440.0, 816.0)] {
        let mut rt = runtime_light(Probe::new(), size);
        mount_and_load(&mut rt, "a short note");
        for key in [
            "connection-ai-notes",
            "connection-notes-title",
            "connection-notes-hint",
            "connection-notes-input",
            "connection-notes-count",
            "connection-notes-save",
        ] {
            assert_inside(&rt, key, size);
        }
        assert_eq!(
            element(&rt, "connection-notes-title").text_color,
            Some(light_title),
            "title uses the LIGHT palette"
        );
        assert!(element(&rt, "connection-notes-input").rect[3] >= 79.5);
    }
}

#[test]
fn saved_label_reverts_exactly_at_the_pinned_deadline() {
    let mut rt = runtime(Probe::new(), (900.0, 520.0));
    mount_and_load(&mut rt, "note");
    click(&mut rt, "connection-notes-save");
    rt.state.port.save = Some(Ok(()));
    rt.frame(0.05);

    let until = note(&rt.state).saved_until.expect("success stamped a deadline");
    let deadline = rt.wake_deadline().expect("successful save schedules a real wake");
    assert!((deadline - until).abs() <= 0.01, "wake pinned to deadline");
    // Step to 10ms before the deadline: still the success label.
    let before = until - rt.clock - 0.01;
    assert!(before > 0.0, "there is room before the deadline");
    rt.frame(before);
    assert_eq!(label_of(&rt, "connection-notes-save"), t("common.success"));
    assert!(note(&rt.state).saved_until.is_some(), "not expired before deadline");
    // Cross the deadline: the poll's tick consumes it and the frame repaints.
    rt.frame(0.02);
    assert!(note(&rt.state).saved_until.is_none(), "expired at the deadline");
    assert_eq!(label_of(&rt, "connection-notes-save"), t("common.save"));
}

#[test]
fn a_second_press_while_saving_never_stages_another_set() {
    let mut rt = runtime(Probe::new(), (900.0, 520.0));
    mount_and_load(&mut rt, "note");
    click(&mut rt, "connection-notes-save");
    assert_eq!(rt.state.port.sets.len(), 1);
    assert!(note(&rt.state).saving);

    // The button is disabled and has no press handler while saving.
    click(&mut rt, "connection-notes-save");
    assert_eq!(rt.state.port.sets.len(), 1, "no set() while a save is in flight");
    assert_eq!(
        rt.state.actions.iter().filter(|a| matches!(a, NotesAction::Save { .. })).count(),
        1,
        "only one Save action was raised"
    );
}

#[test]
fn a_four_byte_cjk_and_an_emoji_each_count_as_four_bytes() {
    let mut rt = runtime(Probe::new(), (900.0, 520.0));
    mount_and_load(&mut rt, "");
    // "𠀀" is U+20000, a four-byte UTF-8 CJK ideograph; "😀" is four bytes too.
    assert_eq!("𠀀".len(), 4);
    assert_eq!("😀".len(), 4);
    assert_eq!("𠀀".chars().count(), 1, "a char count would report 1, not 4");
    rt.update(|s, _tx| {
        s.notes.as_mut().expect("state").content = TextEdit::new("𠀀😀".to_owned());
    });
    rt.frame(1.0 / 60.0);
    assert_eq!(text_of(&rt, "connection-notes-count"), "8 / 4096 bytes");
    assert!(!text_of(&rt, "connection-notes-count").contains(t("connection.notes.overLimit")));
}

#[test]
fn the_error_paragraph_is_rendered_only_on_error() {
    let mut rt = runtime(Probe::new(), (900.0, 520.0));
    mount_and_load(&mut rt, "note");
    assert!(!has_key(&rt, "connection-notes-error"), "no error element on a clean load");

    click(&mut rt, "connection-notes-save");
    assert_eq!(rt.state.port.sets.len(), 1, "save was staged");
    // Deliver the host answer *after* the press: poll runs before input handlers
    // within a frame, so a result queued before the press would be consumed early.
    rt.state.port.save = Some(Err("boom".to_owned()));
    rt.frame(0.05);
    assert!(has_key(&rt, "connection-notes-error"), "error element appears after a failed save");
    assert_eq!(text_of(&rt, "connection-notes-error"), "boom");
}

/// The coder only checked rects with a short note in dark mode. The longest
/// dynamic rows — the over-limit count with its suffix and a long error
/// paragraph — must also fit at both required sizes in both palettes.
#[test]
fn over_limit_count_and_error_rows_fit_both_sizes_in_both_palettes() {
    for dark in [true, false] {
        let view_fn: fn(&Probe, &mut Cx<Probe>) -> El = if dark { view } else { view_light };
        for size in [(960.0, 640.0), (1440.0, 816.0)] {
            // The over-limit count row (count + suffix + button) must not overflow.
            let mut rt = Runtime::new(Probe::new(), view_fn, size, env(dark));
            rt.poll = Some(poll);
            rt.frame(0.0);
            rt.frame(0.0);
            assert_eq!(rt.state.port.gets.len(), 1);
            rt.state.port.load = Some(Ok("a".repeat(MAX_NOTE_BYTES + 1)));
            rt.frame(0.05);
            assert!(note(&rt.state).over_limit());
            for key in ["connection-ai-notes", "connection-notes-count", "connection-notes-save"] {
                assert_inside(&rt, key, size);
            }
            // The colour springs from the previous value; let it settle.
            for _ in 0..20 { rt.frame(1.0 / 60.0); }
            assert_eq!(
                element(&rt, "connection-notes-count").text_color,
                Some(crate::ui::kit::kit(dark).c.danger.fg),
            );

            // A long error paragraph must wrap and stay inside the window too.
            let mut rt = Runtime::new(Probe::new(), view_fn, size, env(dark));
            rt.poll = Some(poll);
            rt.frame(0.0);
            rt.frame(0.0);
            rt.state.port.load = Some(Ok("note".to_owned()));
            rt.frame(0.05);
            click(&mut rt, "connection-notes-save");
            rt.state.port.save = Some(Err(
                "the host rejected this note because the backend is temporarily unavailable".to_owned(),
            ));
            rt.frame(0.05);
            assert!(has_key(&rt, "connection-notes-error"));
            assert_inside(&rt, "connection-notes-error", size);
        }
    }
}
