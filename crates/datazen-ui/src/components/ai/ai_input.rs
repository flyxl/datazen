//! T137: the AI input — port of
//! `packages/frontend/src/components/ai/AiInput.tsx` (295 lines).
//!
//! The chat/NL2SQL input: `@`-context chips sharing one wrapping line with
//! the text, the send / stop control, and the `@` picker trigger state.
//! The React component is controlled (`value` / `onChange` props); the port
//! keeps the same split — the host owns an [`AiInputState`] (the draft
//! `TextEdit`, the context items, the picker flags), renders through
//! [`AiInput`], and mutates through the callbacks. A single
//! [`AiInput::state_access`] wire installs the field's edit binding, the
//! post-edit picker sync (React's `handleChange`) and the Backspace token
//! pop (React's `handleKeyDown`) in one call.
//!
//! ## Keys
//!
//! | part | key |
//! |------|-----|
//! | root | `ai-input` |
//! | field box | `ai-input-field` (React `data-testid`) |
//! | one chip | `context-token-<kind>:<id>` (React `data-testid="context-token"`, React key `<kind>:<id>`) |
//! | chip icon | `context-token-<kind>:<id>-icon` |
//! | chip name | `context-token-<kind>:<id>-name` |
//! | text field | `ai-input-text` |
//! | send | `ai-input-submit` |
//! | stop | `ai-input-stop` |
//!
//! These are the keys the T009-S5 placeholder documented, so swapping the
//! placeholder for this component keeps the tree stable.
//!
//! ## Deliberate deviations from the React source
//!
//! * **The ContextPicker is T217.** The `@` query and open flag live here
//!   ([`AiInputState`]); the picker's element (and its keyboard forwarding,
//!   BUG-06) lands with T217 and mounts from the host's layer, anchored
//!   around this component.
//! * **The send / stop control is a trailing sibling**, not absolutely
//!   positioned at the field's bottom-right corner (rune has no absolute
//!   layout); it sits right of the wrapping line, vertically centered, the
//!   same slot the T009-S5 placeholder uses.
//! * **`rows` collapses to the single kit line.** The kit text input is
//!   single-line; React's `rows` / `min-h` textarea growth has no rune
//!   surface yet. The chip wrapping line itself wraps exactly like React's.
//! * **The gap is uniform 6pt** where React splits `gap-x-1.5 gap-y-1`
//!   (6 / 4); rune's `gap` is one number. Horizontal matches exactly.
//! * `focus-within:border-accent` has no rune surface; the kit field owns
//!   its focus ring. The chip hit-keeping (`onMouseDown` re-focus) is
//!   unnecessary in rune: the chips are not inputs that steal focus.
//! * `lucide-react`'s `Table2` / `Folder` / `File` map to the registry's
//!   `ICON_TABLE` / `ICON_FOLDER` / `ICON_FILE` (12pt, React's `h-3 w-3`,
//!   0.8 opacity); `Square` / `ArrowUp` to `ICON_SQUARE` / `ICON_ARROW_UP`.

use std::rc::Rc;

use rune::{col, icon, row, text, El, KeyEvent, Key, TextEdit, Tx};
use rune_kit::{ControlSize, IconButton, IconButtonVariant};

use super::ai_egress_notice::{ContextItem, ContextKind};
use crate::i18n::t;
use crate::icons::{ICON_ARROW_UP, ICON_FILE, ICON_FOLDER, ICON_SQUARE, ICON_TABLE};
use crate::ui::field::TextField;
use crate::ui::kit::kit;

/// React's `itemKey`: `<kind>:<id>`.
pub fn item_key(item: &ContextItem) -> String {
    format!("{:?}:{}", item.kind, item.id)
}

/// React's `@([^\s@]*)$` at the caret: the picker query, when the text
/// just before the caret ends in an `@`-word. `caret` is a byte offset into
/// `text` (rune's `TextEdit::focus`). A bare trailing `@` matches too — the
/// capture is empty, the picker opens on the unfiltered list.
pub fn at_query(text: &str, caret: usize) -> Option<String> {
    let head = &text[..caret.min(text.len())];
    let mut word = String::new();
    for ch in head.chars().rev() {
        if ch == '@' {
            return Some(word);
        }
        if ch.is_whitespace() {
            return None; // the segment ended before an '@': no trigger
        }
        word.insert(0, ch);
    }
    None // ran out of text without an '@'
}

/// The state the host owns for one input (the React local state plus the
/// controlled value): the draft, the context chips, and the `@` picker flags.
#[derive(Clone, Debug, Default)]
pub struct AiInputState {
    input: TextEdit,
    items: Vec<ContextItem>,
    /// Whether the `@` picker is open (React's `showPicker`).
    picker_open: bool,
}

impl AiInputState {
    pub fn new() -> Self {
        Self::default()
    }

    /// The draft buffer, for the field's edit binding.
    pub fn input(&self) -> &TextEdit {
        &self.input
    }

    /// The draft buffer, mutable, for the edit accessor.
    pub fn input_edit(&mut self) -> &mut TextEdit {
        &mut self.input
    }

    /// The draft text (React's `value`).
    pub fn text(&self) -> &str {
        &self.input.text
    }

    /// Replace the draft (React's controlled value write).
    pub fn set_text(&mut self, value: impl Into<String>) {
        self.input = TextEdit::new(value.into());
    }

    /// Place the caret (React's `selectionStart`): an empty selection at the
    /// byte offset. The `@` trigger reads it.
    pub fn set_caret(&mut self, byte: usize) {
        self.input.anchor = byte;
        self.input.focus = byte;
    }

    /// The context chips (React's `contextItems`).
    pub fn items(&self) -> &[ContextItem] {
        &self.items
    }

    /// React's `onContextItemsChange` write end: replace the chips.
    pub fn set_items(&mut self, items: Vec<ContextItem>) {
        self.items = items;
    }

    /// React's `showPicker`.
    pub fn picker_open(&self) -> bool {
        self.picker_open
    }

    /// React's `setShowPicker(false)` (a pick, an Escape, a click-away).
    pub fn close_picker(&mut self) {
        self.picker_open = false;
    }

    /// React's `handleChange` picker arm: the `@` query at the caret, or
    /// None — which also closes the picker, exactly as the React else-branch
    /// (`setShowPicker(false); setPickerQuery('')`) does.
    pub fn sync_picker(&mut self) -> Option<String> {
        let query = at_query(&self.input.text, self.input.focus);
        self.picker_open = query.is_some();
        query
    }

    /// React's Backspace token pop: with the caret at 0 (selection empty)
    /// and chips present, Backspace (or Delete on an empty draft) drops the
    /// last chip instead of touching the text. Answers whether a pop
    /// happened; the caller (the field's key handler) consumes the key.
    pub fn pop_token_conditions(&self) -> bool {
        !self.items.is_empty() && self.input.anchor == 0 && self.input.focus == 0
    }

    /// Drop the last chip (React's `contextItems.slice(0, -1)`).
    pub fn pop_token(&mut self) -> Option<Vec<ContextItem>> {
        if self.items.is_empty() {
            return None;
        }
        self.items.pop();
        Some(self.items.clone())
    }

    /// React's `handleSelect`: append the picked item when it is new, then
    /// rewrite the draft so the `@`-word that opened the picker disappears
    /// (the text before the last `@` joins the text after the caret with its
    /// leading blanks stripped; an all-blank result becomes empty).
    pub fn apply_pick(&mut self, item: ContextItem) {
        if !self
            .items
            .iter()
            .any(|existing| item_key(existing) == item_key(&item))
        {
            self.items.push(item);
        }
        let caret = self.input.focus.min(self.input.text.len());
        let before = &self.input.text[..caret];
        let after = &self.input.text[caret..];
        if let Some(at) = before.rfind('@') {
            let trimmed_after = after.trim_start();
            let joined = format!("{}{}", &before[..at], trimmed_after);
            let value = if joined.trim_start().is_empty() {
                String::new()
            } else {
                joined
            };
            self.input = TextEdit::new(value);
        }
        self.picker_open = false;
    }
}

/// The input. Renders the state; every interaction is a callback that
/// mutates `S`.
pub struct AiInput<'a, S: 'static> {
    state: &'a AiInputState,
    placeholder: Option<String>,
    disabled: bool,
    hide_submit: bool,
    loading: bool,
    state_access: Option<Rc<dyn for<'b> Fn(&'b mut S) -> &'b mut AiInputState>>,
    on_submit: Option<Rc<dyn Fn(&mut S) + 'static>>,
    on_stop: Option<Rc<dyn Fn(&mut S) + 'static>>,
    on_items_change: Option<Rc<dyn Fn(&mut S, Vec<ContextItem>) + 'static>>,
}

impl<'a, S: 'static> AiInput<'a, S> {
    #[allow(dead_code)] // reusable component; wired by the chat/NL2SQL host tasks
    pub fn new(state: &'a AiInputState) -> Self {
        Self {
            state,
            placeholder: None,
            disabled: false,
            hide_submit: false,
            loading: false,
            state_access: None,
            on_submit: None,
            on_stop: None,
            on_items_change: None,
        }
    }

    /// React's `placeholder` prop.
    #[allow(dead_code)] // reusable component; wired by the chat/NL2SQL host tasks
    pub fn placeholder(mut self, placeholder: impl Into<String>) -> Self {
        self.placeholder = Some(placeholder.into());
        self
    }

    /// React's `disabled` prop.
    #[allow(dead_code)] // reusable component; wired by the chat/NL2SQL host tasks
    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }

    /// React's `isLoading` prop.
    #[allow(dead_code)] // reusable component; wired by the chat/NL2SQL host tasks
    pub fn loading(mut self, loading: bool) -> Self {
        self.loading = loading;
        self
    }

    /// React's `hideSubmit` prop.
    #[allow(dead_code)] // reusable component; wired by the chat/NL2SQL host tasks
    pub fn hide_submit(mut self, hide: bool) -> Self {
        self.hide_submit = hide;
        self
    }

    /// The one wire that binds the field to the host's `AiInputState`: it
    /// installs the edit binding (the draft `TextEdit`), the post-edit
    /// picker sync (React's `handleChange`), and the Backspace token pop
    /// (React's `handleKeyDown` — the `@` picker's keyboard forwarding
    /// arrives with T217).
    #[allow(dead_code)] // reusable component; wired by the chat/NL2SQL host tasks
    pub fn state_access(
        mut self,
        access: impl for<'b> Fn(&'b mut S) -> &'b mut AiInputState + Clone + 'static,
    ) -> Self {
        self.state_access = Some(Rc::new(access));
        self
    }

    /// React's `onSubmit`.
    #[allow(dead_code)] // reusable component; wired by the chat/NL2SQL host tasks
    pub fn on_submit(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_submit = Some(Rc::new(f));
        self
    }

    /// React's `onStop`.
    #[allow(dead_code)] // reusable component; wired by the chat/NL2SQL host tasks
    pub fn on_stop(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_stop = Some(Rc::new(f));
        self
    }

    /// React's `onContextItemsChange`: present means the `@` machinery is
    /// live (React's `hasContext`).
    #[allow(dead_code)] // reusable component; wired by the chat/NL2SQL host tasks
    pub fn on_items_change(
        mut self,
        f: impl Fn(&mut S, Vec<ContextItem>) + 'static,
    ) -> Self {
        self.on_items_change = Some(Rc::new(f));
        self
    }

    /// React's `canSend`: a non-blank draft on a live run state.
    fn can_send(&self) -> bool {
        !self.state.text().trim().is_empty() && !self.loading
    }

    /// React's placeholder ternary: shown while no chips and no text, the
    /// caller's text first, else the `@` hint when the context machinery is
    /// live. (The kit keeps the drawn hint inside its text-input state, out
    /// of the elements API, so tests assert the ternary here.)
    #[allow(dead_code)] // asserted by tests; the drawn hint itself is not observable
    pub fn placeholder_text(&self) -> Option<String> {
        let has_context = self.on_items_change.is_some();
        if !self.state.items.is_empty() || !self.state.text().is_empty() {
            return None;
        }
        match (&self.placeholder, has_context) {
            (Some(placeholder), _) => Some(placeholder.clone()),
            (None, true) => Some(t("context.placeholder").to_owned()),
            (None, false) => None,
        }
    }

    /// The input, mounted (React renders it unconditionally).
    pub fn build(self, dark: bool) -> El {
        let k = kit(dark);
        let state = self.state;

        // The chips + text on one wrapping line.
        let mut wrap = row()
            .key("ai-input-wrap")
            .flex_wrap()
            .items_center()
            .gap(6.0)
            .grow(1.0)
            .min_w(0.0);

        for item in state.items() {
            wrap = wrap.child(chip(item, &k));
        }

        // React's `value` / `onChange` are required props: without the state
        // access wire the draft has no edit source, so the field renders
        // inert (the kit's rule: a bound field needs on_edit, a disabled one
        // needs none) — the same "dead control" React never produces.
        let inert = self.disabled || self.state_access.is_none();
        let mut field_text = TextField::new("ai-input-text", state.input())
            .grow()
            .min_width(96.0)
            .disabled(inert);
        if let Some(placeholder) = self.placeholder_text() {
            field_text = field_text.placeholder(placeholder);
        }
        if let Some(access) = self.state_access.clone() {
            // The edit binding: the kit asks for the draft's TextEdit.
            let edit_access = Rc::clone(&access);
            field_text = field_text.on_edit(move |s: &mut S| edit_access(s).input_edit());
            // React's handleChange picker arm, after each committed edit.
            let sync_access = Rc::clone(&access);
            field_text = field_text.on_change(move |s: &mut S, _tx: &mut Tx| {
                sync_access(s).sync_picker();
            });
            // React's handleKeyDown token pop: Backspace (or Delete on an
            // empty draft) at caret 0 with chips present drops the last chip.
            let on_items_change = self.on_items_change.clone();
            let empty_draft = state.text().is_empty();
            let key_access = Rc::clone(&access);
            field_text = field_text.on_key(
                move |s: &mut S, event: &KeyEvent, _tx: &mut Tx| {
                    if event.key != Key::Backspace
                        && (event.key != Key::Delete || !empty_draft)
                    {
                        return false;
                    }
                    let input = key_access(s);
                    if !input.pop_token_conditions() {
                        return false;
                    }
                    if let Some(popped) = input.pop_token() {
                        if let Some(on_items_change) = &on_items_change {
                            on_items_change(s, popped);
                        }
                    }
                    true // React's preventDefault: the key is consumed.
                },
            );
        }
        wrap = wrap.child(field_text.build(dark));

        // The field box: the React border/surface wrapper.
        let mut field = row()
            .key("ai-input-field")
            .w_full()
            .min_w(0.0)
            .center_y()
            .gap(6.0)
            .px(12.0)
            .py(8.0)
            .border(1.0, k.c.border_base)
            .fill(k.c.layer_01)
            .radius(k.radius.md)
            .child(wrap);
        if self.disabled {
            field = field.opacity(0.5);
        }

        // The send / stop control (a trailing sibling of the wrapping line;
        // see the deviations).
        if !self.hide_submit {
            field = field.child(self.send_or_stop(dark));
        }

        col()
            .key("ai-input")
            .w_full()
            .min_w(0.0)
            .child(field)
    }

    /// React's stop button while streaming with a stop wired, else the send
    /// button (inert unless the draft can send).
    fn send_or_stop(&self, dark: bool) -> El {
        let k = kit(dark);
        if self.loading && self.on_stop.is_some() {
            let mut stop = IconButton::new(&ICON_SQUARE)
                .variant(IconButtonVariant::Neutral)
                .control_size(ControlSize::Small)
                .help(t("chat.stop"))
                .build(&k)
                .key("ai-input-stop");
            if let Some(on_stop) = &self.on_stop {
                let press = Rc::clone(on_stop);
                let keyed = Rc::clone(on_stop);
                stop = stop.on_press(move |state: &mut S| press(state)).on_key(
                    move |state: &mut S, event: &KeyEvent, _tx: &mut Tx| {
                        if event.key == Key::Enter {
                            keyed(state);
                            true
                        } else {
                            false
                        }
                    },
                );
            }
            return stop;
        }

        let mut send = IconButton::new(&ICON_ARROW_UP)
            .variant(IconButtonVariant::Submit)
            .control_size(ControlSize::Small)
            .disabled(!self.can_send())
            .build(&k)
            .key("ai-input-submit");
        if let Some(on_submit) = &self.on_submit {
            let press = Rc::clone(on_submit);
            let keyed = Rc::clone(on_submit);
            send = send.on_press(move |state: &mut S| press(state)).on_key(
                move |state: &mut S, event: &KeyEvent, _tx: &mut Tx| {
                    if event.key == Key::Enter {
                        keyed(state);
                        true
                    } else {
                        false
                    }
                },
            );
        }
        send
    }
}

/// One context chip: the kind icon next to `@<name>`, on the accent wash.
fn chip(item: &ContextItem, k: &rune_kit::Kit) -> El {
    let key = format!("context-token-{}", item_key(item));
    let source = match item.kind {
        ContextKind::Table => &ICON_TABLE,
        ContextKind::Dir => &ICON_FOLDER,
        ContextKind::File => &ICON_FILE,
    };
    row()
        .key(key.clone())
        .items_center()
        .gap(4.0)
        .px(6.0)
        .py(2.0)
        .radius(k.radius.md)
        .fill(k.alpha(k.c.bg_accent, 0.15))
        .child(
            icon(source, 12.0)
                .key(format!("{key}-icon"))
                .color(k.c.text_accent)
                .opacity(0.8),
        )
        .child(
            text(format!("@{}", item.name))
                .key(format!("{key}-name"))
                .font(k.ty.text_xsmall)
                .color(k.c.text_accent)
                .truncate(),
        )
}
