//! Port of `packages/frontend/src/components/connection/ConnectionAiNotes.tsx`.
//!
//! A local "connection notes" editor: load the saved note via the host, edit it
//! in a multiline box, show a live UTF-8 byte count and save it back. Saving
//! swaps the button label to `settings.ai.saving`, then to a transient
//! `common.success` for two seconds, then back to `common.save` — exactly the
//! three React branches on line 85 of the source.
//!
//! ## Host boundary (the injected port)
//!
//! React awaits `aiCommands.connectionNotesGet/Set(connectionId)`. Rune views
//! are immediate mode and cannot await IPC: a press can only ask a host port to
//! stage work, and the host delivers the answer later by applying a
//! [`NotesEvent`] to the caller-owned [`AiNotesState`]. The component therefore
//! owns **no** fake command. [`NotesPort`] is the contract a real host adapter
//! implements; the test suite injects a recording fake. The mount load, the
//! save and the result delivery all carry an `epoch` so a result that arrives
//! after an unmount is discarded (React's `cancelled` flag in its effect
//! cleanup). See leftover L1 in the module footer for the production binding.
//!
//! ## Timed feedback
//!
//! A successful save stamps `saved_until = tx.clock() + 2.0` into the state and
//! schedules `tx.wake_after(2s)`. The caller's poll calls [`AiNotesState::tick`]
//! with the runtime clock; at the deadline `tick` clears `saved_until` (and
//! reports the change) so the next frame paints `common.save` again. This is a
//! real deadline, not a boolean that never expires — the wake is pinned by the
//! independent audit in `tests/connection_ai_notes.rs`.
//!
//! ## Keys (React `data-testid`, kept verbatim)
//!
//! | part      | key                       |
//! |-----------|---------------------------|
//! | root      | `connection-ai-notes`     |
//! | title     | `connection-notes-title`  |
//! | hint      | `connection-notes-hint`   |
//! | textarea  | `connection-notes-input`  |
//! | count     | `connection-notes-count`  |
//! | save      | `connection-notes-save`   |
//! | error     | `connection-notes-error`  |
//!
//! All colour, type and radius values come from `kit(dark)`; no colour literal
//! or glyph is authored here.

use std::rc::Rc;
use std::time::Duration;

use rune::{col, row, text, text_input, El, TextEdit, TextCase, Tx};
use rune_kit::{Button, ButtonVariant, ControlSize};

use crate::i18n::t;
use crate::ui::kit::kit;

/// React line 6: `const MAX_NOTE_BYTES = 4 * 1024`.
#[allow(dead_code)] // public VM surface; consumed by this module's build and the tests
pub const MAX_NOTE_BYTES: usize = 4 * 1024;

/// React line 48: `setTimeout(() => setSaved(false), 2000)`.
#[allow(dead_code)] // public VM surface; consumed by AiNotesState::apply
pub const SAVED_FEEDBACK_MS: u64 = 2000;

const ROOT_KEY: &str = "connection-ai-notes";
const INPUT_KEY: &str = "connection-notes-input";
const COUNT_KEY: &str = "connection-notes-count";
const SAVE_KEY: &str = "connection-notes-save";

/// The host capability behind `aiCommands.connectionNotesGet/Set`.
///
/// Both calls are *staged*: the host records the request and delivers its
/// answer later with [`NotesEvent::Loaded`] / [`NotesEvent::Saved`], keyed by
/// `epoch`. No default implementation is provided — inventing a command that
/// pretends to persist notes would be a lie; the real binding is leftover L1.
#[allow(dead_code)] // host-facing trait; implemented by the wiring in leftover L1
pub trait NotesPort {
    /// React `aiCommands.connectionNotesGet(connectionId)`.
    fn get(&mut self, connection_id: &str, epoch: u64);
    /// React `aiCommands.connectionNotesSet(connectionId, content)`.
    fn set(&mut self, connection_id: &str, content: &str, epoch: u64);
}

/// What the view asks its owner to do. The owner calls the [`NotesPort`] and
/// applies the answer as a [`NotesEvent`].
#[allow(dead_code)] // VM surface; wired by the caller (leftover L1) and the tests
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NotesAction {
    /// The component was laid out for `connection_id` for the first time.
    /// React's `useEffect` on `[connectionId]`.
    Mount { connection_id: String },
    /// The component went away: later results with the old epoch are stale.
    Unmount,
    /// A validated save press (not loading, not over the byte limit).
    Save { connection_id: String, content: String },
}

/// A host result delivered back to the state. `epoch` ties it to the mount
/// that asked for it; a mismatched delivery is a no-op.
#[allow(dead_code)] // VM surface; delivered by the caller (leftover L1) and the tests
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NotesEvent {
    /// The `connectionNotesGet` promise settled.
    Loaded { epoch: u64, result: Result<String, String> },
    /// The `connectionNotesSet` promise settled.
    Saved { epoch: u64, result: Result<(), String> },
}

/// Caller-owned state slice: React's five `useState` values plus the two
/// pieces of bookkeeping immediate mode needs (`epoch` for cancelled work and
/// `mounted_for` so the mount effect fires once per connection).
#[allow(dead_code)] // VM surface; mounted into AppState by leftover L1
#[derive(Clone, Debug)]
pub struct AiNotesState {
    pub connection_id: String,
    /// The textarea's value. `TextEdit::text` is a `String`, so `.len()` is the
    /// UTF-8 byte count React gets from `new TextEncoder().encode(content)`.
    pub content: TextEdit,
    pub loading: bool,
    pub saving: bool,
    /// Absolute clock deadline for the transient `common.success` label;
    /// `None` means "not currently saved".
    pub saved_until: Option<f64>,
    pub error: String,
    /// Bumped on mount/unmount; stale results are ignored.
    pub epoch: u64,
    mounted_for: Option<String>,
}

#[allow(dead_code)] // test/initial-state convenience; wired by leftover L1
impl AiNotesState {
    pub fn new(connection_id: impl Into<String>) -> Self {
        Self {
            connection_id: connection_id.into(),
            content: TextEdit::new(String::new()),
            // React initialises `loading` to true (line 11).
            loading: true,
            saving: false,
            saved_until: None,
            error: String::new(),
            epoch: 0,
            mounted_for: None,
        }
    }

    /// Whether the component should emit a mount load for `self.connection_id`.
    pub fn needs_mount(&self) -> bool {
        self.mounted_for.as_deref() != Some(self.connection_id.as_str())
    }

    /// React's effect body: fresh load, cleared error, new epoch, then the
    /// caller stages `port.get`. Returns the epoch the result must carry.
    pub fn on_mount(&mut self, connection_id: &str) -> u64 {
        self.connection_id = connection_id.to_owned();
        self.mounted_for = Some(connection_id.to_owned());
        self.epoch = self.epoch.wrapping_add(1);
        self.content = TextEdit::new(String::new());
        self.loading = true;
        self.saving = false;
        self.saved_until = None;
        self.error.clear();
        self.epoch
    }

    /// React's effect cleanup (`cancelled = true`). Bumping the epoch makes a
    /// late result harmless; the fields stay as they are until a new mount.
    pub fn on_unmount(&mut self) {
        self.epoch = self.epoch.wrapping_add(1);
        self.mounted_for = None;
    }

    /// React's `handleSave` prologue: clear the error and mark saving, returning
    /// the epoch the save result must carry. Call only after the guards in
    /// [`AiNotesState::can_save`] passed.
    pub fn begin_save(&mut self) -> u64 {
        self.error.clear();
        self.saving = true;
        self.epoch
    }

    /// React's `overLimit` plus the button's `disabled={saving || loading || overLimit}`.
    pub fn can_save(&self) -> bool {
        !self.over_limit() && !self.loading && !self.saving
    }

    /// `new TextEncoder().encode(content).length` — `String::len()` is UTF-8
    /// bytes, so a 4-byte emoji counts four and a 3-byte CJK ideograph three.
    pub fn byte_len(&self) -> usize {
        self.content.text.len()
    }

    pub fn over_limit(&self) -> bool {
        self.byte_len() > MAX_NOTE_BYTES
    }

    /// Whether the transient success label is showing.
    pub fn saved(&self) -> bool {
        self.saved_until.is_some()
    }

    /// Apply a host result. Returns `true` when visible state changed. The
    /// success branch records the real deadline and arms a runtime wake.
    pub fn apply(&mut self, event: NotesEvent, now: f64, tx: &mut Tx) -> bool {
        match event {
            NotesEvent::Loaded { epoch, result } => {
                if epoch != self.epoch || self.mounted_for.is_none() {
                    return false;
                }
                self.loading = false;
                match result {
                    Ok(value) => self.content = TextEdit::new(value),
                    // React only calls setContent on success; on failure it
                    // keeps the old (empty on first load) value and shows error.
                    Err(message) => self.error = message,
                }
                true
            }
            NotesEvent::Saved { epoch, result } => {
                if epoch != self.epoch {
                    return false;
                }
                self.saving = false;
                match result {
                    Ok(()) => {
                        self.saved_until = Some(now + SAVED_FEEDBACK_MS as f64 / 1000.0);
                        tx.wake_after(Duration::from_millis(SAVED_FEEDBACK_MS));
                    }
                    Err(message) => self.error = message,
                }
                true
            }
        }
    }

    /// The deadline check half of the timed feedback. Called from the app poll;
    /// returns `true` when it actually cleared an expired deadline so the poll
    /// can ask for a redraw.
    pub fn tick(&mut self, now: f64) -> bool {
        if self.saved_until.is_some_and(|until| now >= until) {
            self.saved_until = None;
            true
        } else {
            false
        }
    }
}

type ActionFn<S> = Rc<dyn Fn(&mut S, NotesAction) + 'static>;
type EditFn<S> = Rc<dyn for<'b> Fn(&'b mut S) -> &'b mut TextEdit + 'static>;

/// Builder for the ported component. `S` owns the state slice; the widget never
/// does. `edit` is an accessor for the textarea's `TextEdit` (kept inside the
/// state slice) so rune can route typed keystrokes back into state.
#[allow(dead_code)] // reusable component; mounted by leftover L1 and the tests
pub struct ConnectionAiNotes<'a, S: 'static> {
    state: &'a AiNotesState,
    edit: Option<EditFn<S>>,
    on_action: Option<ActionFn<S>>,
}

#[allow(dead_code)] // reusable component; mounted by leftover L1 and the tests
impl<'a, S: 'static> ConnectionAiNotes<'a, S> {
    pub fn new(state: &'a AiNotesState) -> Self {
        Self {
            state,
            edit: None,
            on_action: None,
        }
    }

    /// Accessor for the multiline `TextEdit` the textarea edits. Without it the
    /// field cannot take a caret (rune focuses a text input only through `on_edit`).
    pub fn on_edit(
        mut self,
        f: impl for<'b> Fn(&'b mut S) -> &'b mut TextEdit + 'static,
    ) -> Self {
        self.edit = Some(Rc::new(f));
        self
    }

    pub fn on_action(mut self, f: impl Fn(&mut S, NotesAction) + 'static) -> Self {
        self.on_action = Some(Rc::new(f));
        self
    }

    pub fn build(self, dark: bool) -> El {
        let k = kit(dark);
        let state = self.state;
        let bytes = state.byte_len();
        let over = state.over_limit();
        let loading = state.loading;
        let saved = state.saved();

        // React line 57: mt-6 border-t border-edge/60 pt-4.
        let mut root = col()
            .key(ROOT_KEY)
            .w_full()
            .min_w(0.0)
            .border_sides([1.0, 0.0, 0.0, 0.0], k.alpha(k.c.text_faint, 0.6))
            .border_layout()
            // React uses `mt-6 pt-4`; the kit root forces this page to full
            // height, so an outer margin would push it past the window bottom.
            // Keep the spacing as top padding inside the measured box instead.
            .pt(24.0)
            .gap(4.0);

        // React line 58: 11px semibold uppercase muted title.
        root = root.child(
            text(t("connection.notes.title"))
                .key("connection-notes-title")
                .font(k.ty.text_xsmall_medium)
                .text_case(TextCase::Uppercase)
                .color(k.c.text_muted),
        );
        // React line 61: 11px muted hint.
        root = root.child(
            text(t("connection.notes.hint"))
                .key("connection-notes-hint")
                .font(k.ty.text_xsmall)
                .color(k.c.text_muted)
                .wrap()
                .min_w(0.0),
        );

        // React lines 62-69: a multiline, resizable textarea. Rune text fields
        // are multiline by default (only `.single_line()` pins them), so no
        // single-line call here. `min_h(80)` mirrors `min-h-[80px]`. While
        // loading it is disabled, like React's `disabled={loading}`.
        let mut input = text_input(&state.content, k.ty.text_small)
            .key(INPUT_KEY)
            .w_full()
            .min_w(0.0)
            .min_h(80.0)
            .px(8.0)
            .py(6.0)
            .radius(k.radius.sm)
            .fill(k.c.surface_base)
            .border(1.0, k.c.border_base)
            .placeholder(t("connection.notes.placeholder"), k.alpha(k.c.text_faint, 0.7))
            .disabled(loading);
        // Rune focuses a text input through its `on_edit` accessor, and
        // `El::disabled` does not remove that accessor. So bind it only while
        // enabled: a loading textarea is genuinely unfocusable, like React's.
        if !loading {
            if let Some(edit) = self.edit.clone() {
                input = input.on_edit(rune::edit_access(move |s: &mut S| edit(s)));
            }
        }
        root = root.child(input);

        // React lines 70-87: the count on the left, the save button on the right.
        let mut count = format!("{} / {} bytes", bytes, MAX_NOTE_BYTES);
        if over {
            count.push_str(" · ");
            count.push_str(t("connection.notes.overLimit"));
        }
        let save_label = if state.saving {
            t("settings.ai.saving")
        } else if saved {
            t("common.success")
        } else {
            t("common.save")
        };

        let mut save = Button::new(save_label)
            .variant(ButtonVariant::Neutral)
            .control_size(ControlSize::Small)
            .disabled(!state.can_save());
        // React's `handleSave` re-checks `overLimit` even though the button is
        // disabled. Mirror that by not binding a press at all when the guards
        // fail, so no path can request a save while loading/saving/over-limit.
        if state.can_save() {
            if let Some(on_action) = self.on_action.clone() {
                let cid = state.connection_id.clone();
                let content = state.content.text.clone();
                save = save.on_press(move |s: &mut S| {
                    on_action(s, NotesAction::Save {
                        connection_id: cid.clone(),
                        content: content.clone(),
                    });
                });
            }
        }
        let save = save.build(&k).key(SAVE_KEY);

        root = root.child(
            row()
                .w_full()
                .min_w(0.0)
                .items_center()
                .gap(8.0)
                .flex_wrap()
                .child(
                    text(count)
                        .key(COUNT_KEY)
                        .font(k.ty.text_xsmall)
                        .color(if over { k.c.danger.fg } else { k.c.text_muted })
                        .min_w(0.0)
                        .wrap(),
                )
                .child(save),
        );

        // React line 88: a danger-coloured paragraph, shown only on error.
        if !state.error.is_empty() {
            root = root.child(
                text(state.error.clone())
                    .key("connection-notes-error")
                    .font(k.ty.text_xsmall)
                    .color(k.c.danger.fg)
                    .min_w(0.0)
                    .wrap_anywhere(),
            );
        }

        // React's `useEffect` on mount. Rune has no mount hook on a view, so
        // the first layout of the root emits one `Mount`; the owner stages the
        // host get. Guarded by `needs_mount()` at build time, so it is emitted
        // once per connection rather than on every layout pass.
        if state.needs_mount() {
            if let Some(on_action) = self.on_action.clone() {
                let cid = state.connection_id.clone();
                root = root.on_layout_tx(move |s: &mut S, _rect: [f32; 4], _tx: &mut Tx| {
                    on_action(s, NotesAction::Mount { connection_id: cid.clone() });
                });
            }
        }

        root
    }
}

// Leftover L1 (removal stage: when `crates/datazen-ui-api::WorkspaceApi`
// grows `connectionNotesGet` / `connectionNotesSet` and `AppState` carries the
// `AiNotesState` slice): bind `NotesPort` to that host adapter, deliver
// `NotesEvent` from `app_poll`, and call `AiNotesState::tick` there with the
// poll clock. Until then the port stays injectable and the component is not
// mounted in production; no fake command pretends to persist notes.
