//! `AiChatPanel` input area — T009-S5, the fifth slice of T009.
//!
//! Ported from `packages/frontend/src/components/ai/AiChatPanel.tsx`
//! 777–949: the egress notice, the loaded-skills row, the auto-context
//! chips, the injection chips, the KV chip, the draft-conflict bar, and
//! the `AiInput` wiring. The header (S3), the message list (S4) and the
//! panel shell are other slices. This file has no `aiStore` wiring and
//! is not mounted in `app.rs`'s `Section` dispatch.
//!
//! | React source | Rune item |
//! |---|---|
//! | egress notice wrapper (777–781) | [`AiChatInputArea::egress_wrap`] |
//! | `AiEgressNotice` (778–781) | [`AiChatInputArea::egress_notice`] (VM placeholder, T105 owns the real component) |
//! | loaded skills (782–800) | [`AiChatInputArea::loaded_skills_row`] |
//! | auto-context chips (805–841) | [`AiChatInputArea::auto_context_row`] (chips are S1's `AutoContextChip`) |
//! | injection chips (844–881) | [`AiChatInputArea::injection_row`] |
//! | KV chip (885–895) | [`AiChatInputArea::kv_chip`] |
//! | draft conflict bar (898–932) | [`AiChatInputArea::conflict_bar`] |
//! | `AiInput` wiring (933–945) | [`AiChatInputArea::input_row`] (VM placeholder, T139 owns the real `AiInput`) |
//!
//! # State ownership
//!
//! Like S1–S4, the component is a pure render + event router: the caller
//! owns an [`AiChatInputAreaState`] and passes it in as `&state` each
//! frame; presses call back into the caller, which drives the state
//! machine. The draft text lives in the state's `TextEdit`; submit takes
//! it through [`AiChatInputAreaState::submit`], which owns the
//! `can_send` gate, so Enter and the send button cannot submit a blank
//! draft or one already streaming.
//!
//! # View-model boundary (aiStore slice T088 not done)
//!
//! * Egress: the notice renders from the VM's `set_egress` fields — the
//!   host summary, the database label, the relaxed flag (the settings
//!   store's `aiStrictEgress === false`), and the context-files flag
//!   (the `contextItems` split). T105 owns the real `AiEgressNotice`
//!   component and the store read; this slice renders the same keys and
//!   the same text so T105 swaps the builder, not the contract.
//! * `AiInput`: the field below is a wiring placeholder, not the T139
//!   component. It carries the `ai-input-field` key, the `chat.placeholder`
//!   placeholder, the submit / stop buttons and the `can_send` gate, and
//!   stores `db_session_id` / `database` only until T139 lands. The
//!   context tokens, the `@` picker, the multiline growth and the
//!   keyboard map (`useAiKeyboard`) are all T139's.
//! * `handleSend`'s send itself — `composeKvAiMessage`, the snapshot
//!   attach, the context split, clearing `contextItems`, the
//!   `claimForSend` grant — is the panel slice's job; `submit()` only
//!   takes the trimmed draft and clears the field. `handleStop`'s
//!   `cancelChat` is likewise only reported through `on_stop`.
//! * `resolveConflict`'s `onDraftConsumed` ack and the tab switch /
//!   focus are the panel slice's job; `resolve_conflict` returns the
//!   request id so the caller can ack it.
//! * Assistant message content is still S2's plain-text placeholder.
//!
//! # Key convention
//!
//! Keys are React's `data-testid` values verbatim where React has one:
//! `ai-egress-notice`, `ai-egress-summary`, `ai-egress-database`,
//! `ai-loaded-skills`, `ai-auto-context-chips`, `ai-auto-context-sql`,
//! `ai-auto-context-selection`, `ai-auto-context-error`,
//! `ai-auto-context-result`, `ai-injection-chips`,
//! `ai-injection-template`, `ai-injection-notes`,
//! `ai-injection-priority`, `ai-kv-context-chip`, `draft-conflict-bar`,
//! `draft-replace`, `draft-append`, `draft-cancel`, `ai-input-field`.
//! Internal keys (no React testid) are documented here so the panel slice
//! can find them: `ai-chat-input-area` (root),
//! `ai-chat-input-egress-wrap` (the `mb-2` wrapper holding the egress
//! notice and the skills row), `ai-egress-message-{i}` (the relaxed /
//! context lines), `ai-loaded-skills-label` (the `chat.skillsLoaded`
//! text), `ai-loaded-skill-{id-or-name}` (one chip per skill — React
//! keys these by `skill.id ?? skill.name`, with no testid),
//! `ai-kv-context-chip-label` (the `redis.ai.context.attached` text),
//! `ai-kv-context-chip-key` (the key name), `draft-conflict-bar-text`
//! (the `chat.draft.conflict` text), `ai-input` (the `AiInput` box),
//! `ai-input-text` (the draft field), `ai-input-submit` (the send
//! button), `ai-input-stop` (the stop button).
//!
//! # Deliberate boundaries
//!
//! * **The send button has no accessible name, exactly as in React.**
//!   `AiInput`'s submit button carries no `aria-label` or `title`, so
//!   the Rune button carries none either; the stop button keeps
//!   `chat.stop` as its help text, mirroring React's `title`.
//! * **The submit button is inline, not absolutely positioned.** React
//!   floats it `absolute bottom-1.5 right-1.5` over the field's
//!   bottom-right corner; Rune has no absolute overlay inside a row, so
//!   the button sits at the row's end, vertically centred. The field
//!   row keeps its `pr` room by giving the field `grow` instead.
//! * **The draft field is single-line for now.** React's `AiInput` is a
//!   growing multiline textarea; the placeholder uses the kit-backed
//!   single-line `TextField` at 36pt (React's field box is ~40pt: a
//!   20pt line inside `py-2`). Multiline growth is T139's.
//! * **The draft field stays editable while streaming.** React disables
//!   the textarea while streaming (`disabled={isStreaming}`), but
//!   `TextField` exposes no disabled state; the placeholder keeps the
//!   field editable and only the submit path is gated (`can_send` is
//!   false while streaming, and the send button is disabled). T139
//!   closes the gap when it owns the field.
//! * **Injection `title` tooltips ride `El::help`.** React puts
//!   `chat.injection.readOnlyHint` / `priorityHint` in the chips'
//!   `title` attribute; Rune has no title attribute on text, so the
//!   hints are the chips' hover help instead — same text, same trigger.
//! * **`data-key-name` has no Rune equivalent.** React tags the KV chip
//!   with `data-key-name={keyName}`; the key name itself renders as the
//!   chip's truncated mono text (`ai-kv-context-chip-key`), which is the
//!   observable channel for it.
//! * **Amber is React-verbatim.** The egress notice and the conflict bar
//!   use Tailwind's amber-500 (border /10 fill) and amber-200 text,
//!   kept in `ui/palette.rs` as `ai_amber_*`, the same way S3 kept the
//!   shield hues there. The conflict buttons use the kit `Warning`
//!   variant, the closest semantic match for React's amber-500/20
//!   buttons.

// The panel view (a later T009 slice) is the production consumer; until it
// lands the component tests are the only callers, so the dead-code lint is
// silenced here rather than in a crate-wide allow.
#![allow(dead_code)]

use std::rc::Rc;

use rune::{col, row, text, El, Key, KeyEvent, TextEdit, Tx};
use rune_kit::{Button, ButtonVariant, ControlSize, IconButton, IconButtonVariant};

use super::ai_chat_input_area_state::{AiChatInputAreaState, AiLoadedSkill, ConflictResolution};
use super::ai_chat_panel::AutoContextChip;
use crate::i18n::t;
use crate::icons::{ICON_ARROW_UP, ICON_BOOK_OPEN, ICON_SQUARE};
use crate::ui::field::TextField;
use crate::ui::kit::kit;
use crate::ui::palette::{ai_amber_border, ai_amber_fill, ai_amber_text};

/// `AiChatPanel`'s input area (`AiChatPanel.tsx:777–949`). The caller owns
/// the [`AiChatInputAreaState`] and wires the callbacks; the area itself
/// keeps no state.
pub(crate) struct AiChatInputArea<'a, S: 'static> {
    state: &'a AiChatInputAreaState,
    on_input_edit: Option<Rc<dyn for<'b> Fn(&'b mut S) -> &'b mut TextEdit>>,
    on_submit: Option<Rc<dyn Fn(&mut S)>>,
    on_stop: Option<Rc<dyn Fn(&mut S)>>,
    on_remove_snapshot_field: Option<Rc<dyn Fn(&mut S, &str)>>,
    on_resolve_conflict: Option<Rc<dyn Fn(&mut S, ConflictResolution)>>,
    on_dismiss_conflict: Option<Rc<dyn Fn(&mut S)>>,
}

impl<'a, S: 'static> AiChatInputArea<'a, S> {
    pub(crate) fn new(state: &'a AiChatInputAreaState) -> Self {
        Self {
            state,
            on_input_edit: None,
            on_submit: None,
            on_stop: None,
            on_remove_snapshot_field: None,
            on_resolve_conflict: None,
            on_dismiss_conflict: None,
        }
    }

    /// The draft field's edit accessor, e.g. `|state| state.area.input_edit()`.
    pub(crate) fn on_input_edit(
        mut self,
        f: impl for<'b> Fn(&'b mut S) -> &'b mut TextEdit + 'static,
    ) -> Self {
        self.on_input_edit = Some(Rc::new(f));
        self
    }

    /// Called when the draft is submitted (Enter in the field, or the
    /// send button); the caller drives
    /// [`AiChatInputAreaState::submit`] and forwards the returned text to
    /// `sendMessage`. `submit()` returns `None` for a blank draft or
    /// while streaming, so the caller need not re-check the gate.
    pub(crate) fn on_submit(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_submit = Some(Rc::new(f));
        self
    }

    /// Called when the stop button is pressed; the caller owns
    /// `handleStop` (`cancelChat`). Only rendered while streaming.
    pub(crate) fn on_stop(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_stop = Some(Rc::new(f));
        self
    }

    /// Called with the snapshot field name (`editorSql`, `selection`,
    /// `lastError`, `resultPreview`) when an auto-context chip's remove
    /// button is pressed; the caller drives
    /// [`AiChatInputAreaState::remove_snapshot_field`].
    pub(crate) fn on_remove_snapshot_field(mut self, f: impl Fn(&mut S, &str) + 'static) -> Self {
        self.on_remove_snapshot_field = Some(Rc::new(f));
        self
    }

    /// Called with the strategy when a conflict-bar button is pressed;
    /// the caller drives
    /// [`AiChatInputAreaState::resolve_conflict`] and acks the returned
    /// request id via `onDraftConsumed`.
    pub(crate) fn on_resolve_conflict(
        mut self,
        f: impl Fn(&mut S, ConflictResolution) + 'static,
    ) -> Self {
        self.on_resolve_conflict = Some(Rc::new(f));
        self
    }

    /// Called when the conflict bar's cancel button is pressed; the
    /// caller drives [`AiChatInputAreaState::dismiss_conflict`].
    pub(crate) fn on_dismiss_conflict(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_dismiss_conflict = Some(Rc::new(f));
        self
    }

    pub(crate) fn build(self, dark: bool) -> El {
        let state = self.state;
        let k = kit(dark);
        let mut root = col()
            .key("ai-chat-input-area")
            .w_full()
            .min_w(0.0)
            // React's input wrapper: `shrink-0 border-t border-edge p-2`.
            .border_sides([1.0, 0.0, 0.0, 0.0], k.c.border_base)
            .padding(8.0)
            .gap(8.0);
        if state.show_egress_notice() || state.show_loaded_skills() {
            root = root.child(self.egress_wrap(dark));
        }
        if state.show_auto_context() {
            root = root.child(self.auto_context_row(dark));
        }
        if state.show_injection() {
            root = root.child(self.injection_row(dark));
        }
        if state.show_kv_chip() {
            root = root.child(self.kv_chip(dark));
        }
        if state.show_conflict_bar() {
            root = root.child(self.conflict_bar(dark));
        }
        root.child(self.input_row(dark))
    }

    /// The `mb-2` wrapper (`AiChatPanel.tsx:777–781`): the egress notice
    /// above the loaded-skills row, 4pt apart (`mt-1`).
    fn egress_wrap(&self, dark: bool) -> El {
        let mut wrap = col()
            .key("ai-chat-input-egress-wrap")
            .w_full()
            .min_w(0.0)
            .gap(4.0);
        if self.state.show_egress_notice() {
            wrap = wrap.child(self.egress_notice(dark));
        }
        if self.state.show_loaded_skills() {
            wrap = wrap.child(self.loaded_skills_row(dark));
        }
        wrap
    }

    /// The `AiEgressNotice` VM placeholder (`AiEgressNotice.tsx`): the
    /// summary line, the database line, then the relaxed / context
    /// messages in React's order. T105 owns the real component and the
    /// settings-store read; this renders the same keys and text from the
    /// VM fields so the swap changes the builder, not the contract.
    fn egress_notice(&self, dark: bool) -> El {
        let k = kit(dark);
        let state = self.state;
        let mut notice = col()
            .key("ai-egress-notice")
            .w_full()
            .min_w(0.0)
            .gap(2.0)
            .px(8.0)
            .py(6.0)
            .radius(k.radius.sm)
            .border(1.0, ai_amber_border(dark))
            .fill(ai_amber_fill(dark));
        if let Some(summary) = state.egress_summary_text() {
            notice = notice.child(
                text(summary.to_string())
                    .key("ai-egress-summary")
                    .font(k.ty.text_xsmall_medium)
                    .color(ai_amber_text(dark))
                    .wrap(),
            );
        }
        if let Some(database) = state.egress_database_label() {
            notice = notice.child(
                text(database)
                    .key("ai-egress-database")
                    .font(k.ty.text_xsmall)
                    .color(k.alpha(ai_amber_text(dark), 0.70))
                    .wrap(),
            );
        }
        for (index, message) in state.egress_messages().into_iter().enumerate() {
            notice = notice.child(
                text(message)
                    .key(format!("ai-egress-message-{index}"))
                    .font(k.ty.text_xsmall)
                    .color(ai_amber_text(dark))
                    .wrap(),
            );
        }
        notice
    }

    /// The loaded-skills row (`AiChatPanel.tsx:782–800`): a BookOpen
    /// glyph, the `chat.skillsLoaded` count, and one chip per skill.
    /// React's `text-[11px]` renders at the kit's 12pt `text_xsmall`
    /// (the kit type scale has no 11pt step, as in S3/S4).
    fn loaded_skills_row(&self, dark: bool) -> El {
        let k = kit(dark);
        let state = self.state;
        let count = state.loaded_skills().len().to_string();
        let label = t("chat.skillsLoaded").replacen("{count}", &count, 1);
        let mut row_el = row()
            .key("ai-loaded-skills")
            .w_full()
            .min_w(0.0)
            .center_y()
            .flex_wrap()
            .gap(6.0)
            .child(
                rune::icon(&ICON_BOOK_OPEN, 12.0)
                    .color(k.c.text_muted)
                    .key("ai-loaded-skills-icon"),
            )
            .child(
                text(label)
                    .key("ai-loaded-skills-label")
                    .font(k.ty.text_xsmall)
                    .color(k.c.text_muted),
            );
        for skill in state.loaded_skills() {
            row_el = row_el.child(self.skill_chip(dark, skill));
        }
        row_el
    }

    /// One loaded-skill chip (`AiChatPanel.tsx:793–799`): the mono name
    /// in a bordered chip. React keys it by `skill.id ?? skill.name`
    /// with no testid; the Rune key carries that value under the
    /// `ai-loaded-skill-` prefix so the panel slice can find it.
    fn skill_chip(&self, dark: bool, skill: &AiLoadedSkill) -> El {
        let k = kit(dark);
        row()
            .key(format!("ai-loaded-skill-{}", skill.key()))
            .center_y()
            .px(6.0)
            .py(2.0)
            .radius(k.radius.sm)
            .border(1.0, k.c.border_base)
            .fill(k.c.layer_01)
            .child(
                text(skill.name.clone())
                    .key(format!("ai-loaded-skill-{}-label", skill.key()))
                    .font(k.ty.mono)
                    .color(k.c.text_muted),
            )
    }

    /// The auto-context chips (`AiChatPanel.tsx:805–841`): one S1
    /// `AutoContextChip` per snapshot field that is present, in React's
    /// order (sql, selection, error, result). The chip's remove button
    /// reports its testid; the builder maps it back to the snapshot
    /// field name the state machine removes.
    fn auto_context_row(&self, dark: bool) -> El {
        let mut chips = row()
            .key("ai-auto-context-chips")
            .w_full()
            .min_w(0.0)
            .center_y()
            .flex_wrap()
            .gap(6.0);
        let snapshot = self.state.snapshot();
        let present = [
            (
                snapshot
                    .and_then(|s| s.editor_sql.as_deref())
                    .is_some_and(|v| !v.trim().is_empty()),
                t("chat.autoContext.sql"),
                "ai-auto-context-sql",
                "editorSql",
            ),
            (
                snapshot
                    .and_then(|s| s.selection.as_deref())
                    .is_some_and(|v| !v.trim().is_empty()),
                t("chat.autoContext.selection"),
                "ai-auto-context-selection",
                "selection",
            ),
            (
                snapshot
                    .and_then(|s| s.last_error.as_deref())
                    .is_some_and(|v| !v.trim().is_empty()),
                t("chat.autoContext.error"),
                "ai-auto-context-error",
                "lastError",
            ),
            (
                snapshot
                    .and_then(|s| s.result_preview.as_ref())
                    .is_some_and(|p| !p.columns.is_empty() || !p.rows.is_empty()),
                t("chat.autoContext.result"),
                "ai-auto-context-result",
                "resultPreview",
            ),
        ];
        for (shown, label, test_id, field) in present {
            if !shown {
                continue;
            }
            let mut chip = AutoContextChip::<S>::new(label, test_id);
            if let Some(on_remove) = &self.on_remove_snapshot_field {
                let remove = Rc::clone(on_remove);
                let field = field.to_string();
                chip = chip.on_remove(move |state: &mut S, _test_id: &str| {
                    remove(state, &field);
                });
            }
            chips = chips.child(chip.build(dark));
        }
        chips
    }

    /// The injection chips (`AiChatPanel.tsx:844–881`): the template chip
    /// (`chat.injection.template`), the connection-notes chip
    /// (`chat.injection.notes`), and — only together with a template —
    /// the priority chip (`chat.injection.priority`). The `title`
    /// tooltips ride `El::help` (see the module docs).
    fn injection_row(&self, dark: bool) -> El {
        let injection = self.state.injection();
        let mut chips = row()
            .key("ai-injection-chips")
            .w_full()
            .min_w(0.0)
            .center_y()
            .flex_wrap()
            .gap(6.0);
        if let Some(template) = injection
            .and_then(|i| i.template_name.as_deref())
            .map(str::trim)
            .filter(|name| !name.is_empty())
        {
            let label = t("chat.injection.template").replacen("{name}", template, 1);
            chips = chips.child(self.injection_chip(
                dark,
                "ai-injection-template",
                label,
                t("chat.injection.readOnlyHint"),
            ));
        }
        if injection.is_some_and(|i| i.has_connection_notes) {
            chips = chips.child(self.injection_chip(
                dark,
                "ai-injection-notes",
                t("chat.injection.notes").to_string(),
                t("chat.injection.readOnlyHint"),
            ));
        }
        if injection.is_some_and(|i| i.shows_priority()) {
            chips = chips.child(self.injection_chip(
                dark,
                "ai-injection-priority",
                t("chat.injection.priority").to_string(),
                t("chat.injection.priorityHint"),
            ));
        }
        chips
    }

    /// One injection chip: a pill with the label and the hint as hover
    /// help. React's `text-[10px]` renders at the kit's 12pt
    /// `text_xsmall` (no 10pt step in the kit type scale).
    fn injection_chip(&self, dark: bool, key: &str, label: String, hint: &str) -> El {
        let k = kit(dark);
        text(label)
            .key(key.to_string())
            .font(k.ty.text_xsmall)
            .color(k.c.text_muted)
            .px(8.0)
            .py(2.0)
            .radius(999.0)
            .border(1.0, k.c.border_base)
            .fill(k.c.layer_01)
            .help(hint.to_string())
    }

    /// The KV chip (`AiChatPanel.tsx:885–895`): the
    /// `redis.ai.context.attached` label plus the key name in mono,
    /// truncated. `data-key-name` has no Rune equivalent; the key-name
    /// text is the observable channel (see the module docs).
    fn kv_chip(&self, dark: bool) -> El {
        let k = kit(dark);
        let key_name = self.state.kv_key_name().unwrap_or_default();
        row()
            .key("ai-kv-context-chip")
            .w_full()
            .min_w(0.0)
            .center_y()
            .gap(6.0)
            .px(8.0)
            .py(4.0)
            .radius(k.radius.sm)
            .border(1.0, k.c.border_base)
            .fill(k.c.layer_01)
            .child(
                text(t("redis.ai.context.attached").to_string())
                    .key("ai-kv-context-chip-label")
                    .font(k.ty.text_xsmall)
                    .color(k.c.text_muted),
            )
            .child(
                text(key_name.to_string())
                    .key("ai-kv-context-chip-key")
                    .font(k.ty.mono)
                    .color(k.c.text_muted)
                    .truncate()
                    .min_w(0.0)
                    .shrink(),
            )
    }

    /// The draft-conflict bar (`AiChatPanel.tsx:898–932`): the
    /// `chat.draft.conflict` text plus replace / append / cancel. The
    /// buttons use the kit `Warning` variant, the closest semantic match
    /// for React's amber-500/20 buttons.
    fn conflict_bar(&self, dark: bool) -> El {
        let k = kit(dark);
        let mut bar = row()
            .key("draft-conflict-bar")
            .w_full()
            .min_w(0.0)
            .center_y()
            .gap(8.0)
            .px(8.0)
            .py(6.0)
            .radius(k.radius.sm)
            .border(1.0, ai_amber_border(dark))
            .fill(ai_amber_fill(dark))
            .child(
                text(t("chat.draft.conflict").to_string())
                    .key("draft-conflict-bar-text")
                    .font(k.ty.text_xsmall)
                    .color(ai_amber_text(dark))
                    .truncate()
                    .min_w(0.0)
                    .shrink(),
            );
        bar = bar.child(self.conflict_button(
            dark,
            "draft-replace",
            t("chat.draft.replace"),
            self.resolve_action(ConflictResolution::Replace),
        ));
        bar = bar.child(self.conflict_button(
            dark,
            "draft-append",
            t("chat.draft.append"),
            self.resolve_action(ConflictResolution::Append),
        ));
        bar = bar.child(self.conflict_button(
            dark,
            "draft-cancel",
            t("common.cancel"),
            self.on_dismiss_conflict.clone(),
        ));
        bar
    }

    /// The resolve callback wrapped for one strategy, as a plain
    /// `Fn(&mut S)` the button wiring can call. `None` when the caller
    /// wired no resolve callback.
    fn resolve_action(&self, strategy: ConflictResolution) -> Option<Rc<dyn Fn(&mut S)>> {
        let action = self.on_resolve_conflict.as_ref()?;
        let action = Rc::clone(action);
        Some(Rc::new(move |state: &mut S| action(state, strategy)))
    }

    /// One conflict-bar button. Kit buttons fire on Space; Enter is wired
    /// explicitly, as on S1's chip and option buttons.
    fn conflict_button(
        &self,
        dark: bool,
        key: &str,
        label: &str,
        action: Option<Rc<dyn Fn(&mut S)>>,
    ) -> El {
        let k = kit(dark);
        let button = Button::new(label)
            .variant(ButtonVariant::Warning)
            .control_size(ControlSize::Small)
            .help(label.to_string())
            .build(k)
            .key(key.to_string());
        wire_button(button, &action)
    }

    /// The `AiInput` wiring placeholder (`AiInput.tsx`, wired from
    /// `AiChatPanel.tsx:933–945`): the `ai-input-field` row holding the
    /// draft field and the submit / stop button. See the module docs for
    /// what T139 still owns.
    fn input_row(&self, dark: bool) -> El {
        let k = kit(dark);
        let state = self.state;
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
            .radius(k.radius.md);
        let mut text_field = TextField::new("ai-input-text", state.input())
            .height(36.0)
            .pad_x(8.0)
            .grow();
        if let Some(placeholder) = state.input_placeholder() {
            text_field = text_field.placeholder(placeholder);
        }
        if let Some(accessor) = &self.on_input_edit {
            let accessor = Rc::clone(accessor);
            text_field = text_field.on_edit(move |state: &mut S| accessor(state));
        }
        if let Some(on_submit) = &self.on_submit {
            let submit = Rc::clone(on_submit);
            text_field = text_field.on_submit(move |state: &mut S, _tx: &mut Tx| {
                submit(state);
            });
        }
        field = field.child(text_field.build(dark));
        field = field.child(self.submit_button(dark));
        col().key("ai-input").w_full().min_w(0.0).child(field)
    }

    /// The send / stop button (`AiInput.tsx:257–289`): while streaming
    /// (and `on_stop` wired) the Square stop button, otherwise the
    /// ArrowUp send button, disabled unless the draft can send. React's
    /// send button has no accessible name, so neither does this one;
    /// the stop button keeps `chat.stop` as its help text.
    fn submit_button(&self, dark: bool) -> El {
        let k = kit(dark);
        let state = self.state;
        if state.is_streaming() && self.on_stop.is_some() {
            let mut stop = IconButton::new(&ICON_SQUARE)
                .variant(IconButtonVariant::Neutral)
                .control_size(ControlSize::Small)
                .help(t("chat.stop"))
                .build(k)
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
            .disabled(!state.can_send())
            .build(k)
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

/// Wire Space (kit `on_press`) and Enter on a built button element, the way
/// S1's chip and option buttons do — kit buttons fire on Space, React's
/// `<button>` fires on Enter too.
fn wire_button<S: 'static>(el: El, action: &Option<Rc<dyn Fn(&mut S)>>) -> El {
    let Some(action) = action else {
        return el;
    };
    let press = Rc::clone(action);
    let keyed = Rc::clone(action);
    el.on_press(move |state: &mut S| press(state)).on_key(
        move |state: &mut S, event: &KeyEvent, _tx: &mut Tx| {
            if event.key == Key::Enter {
                keyed(state);
                true
            } else {
                false
            }
        },
    )
}
