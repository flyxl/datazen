//! Leaf subcomponents of `AiChatPanel` — T009-S1, the first slice of T009.
//!
//! Ported from `packages/frontend/src/components/ai/AiChatPanel.tsx`, only the
//! leaves that depend on neither `aiStore` nor the unported children
//! (`AiInput`, `AiMessageContent`, `AiSessionHistory`, `WorkflowPanel`):
//!
//! | React source | Rune item |
//! |---|---|
//! | `formatMcpToolDisplayName` (88–91) | [`format_mcp_tool_display_name`] |
//! | `hasSnapshotContent` (951–962) | [`has_snapshot_content`] + [`AiChatContextSnapshot`] |
//! | `AutoContextChip` (964–990) | [`AutoContextChip`] |
//! | `QuestionBlock` (1213–1321, exported) | [`QuestionBlock`] + [`QuestionBlockState`] |
//!
//! # State ownership
//!
//! The component is a pure render + event router, following
//! `components/history` and `components/filter`: the caller owns a
//! [`QuestionBlockState`] (answers, custom-text drafts, submitted flag) and
//! passes it in as `&state` each frame; presses call back into the caller,
//! which drives the state machine. The pure formatting
//! ([`QuestionBlockState::format_submission`]) is unit-testable without a
//! runtime.
//!
//! React's custom-answer input derives its value from the answer: it shows
//! the answer string only when that string is not an option id, otherwise
//! `''`. Rune keeps the draft in a separate [`rune::TextEdit`] per question
//! (a text field cannot take focus without one), so the state machine mirrors
//! the derivation both ways:
//!
//! * typing syncs the draft into the answer on the next accessor call — and
//!   always before submit, because [`QuestionBlockState::submit`] syncs every
//!   draft first (the runtime may not have called the edit accessor since the
//!   last keystroke, while React's `onChange` writes the answer per keystroke);
//! * pressing an option clears the draft, so the field shows `''` afterwards,
//!   exactly as React's derived value does.
//!
//! # Key convention
//!
//! Keys are React's `data-testid` values verbatim:
//! `question-block`, `question-block-item-{id}`,
//! `question-option-{question}-{option}`, `question-custom-input-{id}`,
//! `question-submit`; the chip renders under its caller-given `testId` with
//! the remove button at `{testId}-remove`.
//!
//! # Deliberate boundaries
//!
//! * `ChatBubble` and the panel itself are later slices: this file has no
//!   `aiStore` wiring and is not mounted in `app.rs`'s `Section` dispatch.
//!   Wiring is a later slice's job.
//! * `aria-pressed` has no Rune hook (`el/a11y.rs` exposes label /
//!   description / checked / selected, but no `pressed()`), and a Button's
//!   role is not in `a11y::roles::selectable_item`, so `focus_selected`
//!   does not surface as `selected` in the accessibility tree either
//!   (only `ListBoxOption` / `Tab` / `TreeItem` / … do). Option buttons
//!   therefore use `El::focus_selected` — the same channel `ui/tabs.rs`
//!   and `filter_connector_toggle.rs` use for a selected toggle — and the
//!   pressed state is observed through `ElementInfo::item_selected` plus
//!   the accent wash fill, both read off the laid-out element tree. The
//!   missing `aria-pressed` in the a11y tree is a known Rune gap, in the
//!   same class as the `aria-controls` gap noted in T0-11.
//! * The chip's remove label keeps React's English template
//!   (`remove {label}`), because the label itself is already translated text.
//! * After submit React returns `null` (unmounts); Rune keeps an empty keyed
//!   `question-block` shell with no children, so the parent's layout slot
//!   stays stable. The shell is not instantly zero-size: it folds away over
//!   successive frames (measured 288.5pt before submit, 274.5pt on the submit
//!   frame, then ~x0.74 per frame, under 0.5pt after ~30 frames, width 928pt
//!   throughout, no overflow or overlap), so the slot is eventually vacated.
//! * The submit button's Send glyph is [`crate::icons::ICON_SEND`], added to
//!   the registry with this slice.

// The panel view (a later T009 slice) is the production consumer; until it
// lands the component tests are the only callers, so the dead-code lint is
// silenced here rather than in a crate-wide allow.
#![allow(dead_code)]

use std::collections::HashMap;
use std::rc::Rc;

use rune::{col, row, text, Color, El, Key, KeyEvent, TextEdit, Tx};
use rune_kit::{Button, ButtonVariant, ControlSize, IconButton, IconButtonVariant};
use serde_json::Value;

use crate::i18n::t;
use crate::icons::{ICON_SEND, ICON_X};
use crate::ui::field::TextField;
use crate::ui::kit::kit;

/// React's `formatMcpToolDisplayName`: a qualified tool name `a/b/c…` shows
/// from the third segment on; anything shorter shows as-is.
///
/// `split('/')` on `""` yields one empty segment in both JS and Rust, so the
/// empty name falls through to the as-is branch in both.
pub(crate) fn format_mcp_tool_display_name(qualified_name: &str) -> String {
    let parts: Vec<&str> = qualified_name.split('/').collect();
    if parts.len() >= 3 {
        parts[2..].join("/")
    } else {
        qualified_name.to_owned()
    }
}

/// React's `AiChatResultPreview` (`types/ai.ts:270`): the frozen result head
/// of an auto-context snapshot. Only the lengths matter to
/// [`has_snapshot_content`]; the rows are kept whole so a later slice can
/// show them without re-fetching.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct AiChatResultPreview {
    pub columns: Vec<String>,
    pub rows: Vec<Vec<Value>>,
}

/// React's `AiChatContextSnapshot` (`types/ai.ts:278`): the editor / result
/// facts the panel freezes on send. All fields are optional; a missing field
/// is a field the user removed or the host never supplied.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct AiChatContextSnapshot {
    pub editor_sql: Option<String>,
    pub selection: Option<String>,
    pub last_error: Option<String>,
    pub result_preview: Option<AiChatResultPreview>,
}

/// React's `hasSnapshotContent` (951–962): any field non-empty after trimming
/// counts, and a result preview counts when it has columns or rows.
pub(crate) fn has_snapshot_content(snapshot: Option<&AiChatContextSnapshot>) -> bool {
    let Some(snapshot) = snapshot else {
        return false;
    };
    snapshot
        .editor_sql
        .as_deref()
        .is_some_and(|value| !value.trim().is_empty())
        || snapshot
            .selection
            .as_deref()
            .is_some_and(|value| !value.trim().is_empty())
        || snapshot
            .last_error
            .as_deref()
            .is_some_and(|value| !value.trim().is_empty())
        || snapshot
            .result_preview
            .as_ref()
            .is_some_and(|preview| !preview.columns.is_empty() || !preview.rows.is_empty())
}

/// One removable auto-context chip: the label plus an X button.
///
/// React renders this inline in the panel (`AiChatPanel.tsx:964-990`) with the
/// caller passing `label`, `testId` and `onRemove`. The chip keeps that shape:
/// the panel (a later slice) maps `testId` back to the snapshot field.
pub(crate) struct AutoContextChip<S: 'static> {
    label: String,
    test_id: String,
    on_remove: Option<Rc<dyn Fn(&mut S, &str)>>,
}

impl<S: 'static> AutoContextChip<S> {
    pub(crate) fn new(label: impl Into<String>, test_id: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            test_id: test_id.into(),
            on_remove: None,
        }
    }

    /// Called with the chip's `testId` when the X is pressed; the caller maps
    /// it back to the snapshot field it removes.
    pub(crate) fn on_remove(mut self, f: impl Fn(&mut S, &str) + 'static) -> Self {
        self.on_remove = Some(Rc::new(f));
        self
    }

    pub(crate) fn build(self, dark: bool) -> El {
        let k = kit(dark);
        let label = self.label.clone();
        // React's aria-label template, verbatim: the label is already
        // translated text, so the template stays English.
        let remove_label = format!("remove {label}");
        let mut remove = IconButton::new(&ICON_X)
            .variant(IconButtonVariant::Ghost)
            .control_size(ControlSize::Small)
            .label(remove_label.clone())
            .help(remove_label)
            .build(k)
            .key(format!("{}-remove", self.test_id));
        if let Some(on_remove) = &self.on_remove {
            let press_id = self.test_id.clone();
            let key_id = self.test_id.clone();
            let press = Rc::clone(on_remove);
            let keyed = Rc::clone(on_remove);
            // Kit buttons fire on Space; Return passes buttons by (see
            // `FilterConditionChip::on_remove`), so Enter is wired explicitly
            // to keep keyboard activation at parity with React's `<button>`.
            remove = remove
                .on_press(move |state: &mut S| press(state, &press_id))
                .on_key(move |state: &mut S, event: &KeyEvent, _tx: &mut Tx| {
                    if event.key == Key::Enter {
                        keyed(state, &key_id);
                        true
                    } else {
                        false
                    }
                });
        }
        row()
            .key(self.test_id)
            .center_y()
            .gap(2.0)
            .radius(999.0)
            .pl(6.0)
            .pr(2.0)
            .py(2.0)
            .max_w(560.0)
            .min_w(0.0)
            .border(1.0, k.c.border_base)
            .fill(k.c.layer_01)
            .child(
                text(label)
                    .key("label")
                    .font(k.ty.text_xsmall)
                    .color(k.c.text_muted)
                    .truncate(),
            )
            .child(remove)
    }
}

/// One answer option of a question (`AiQuestionOption`, `types/ai.ts:180`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct AiQuestionOption {
    pub id: String,
    pub label: String,
}

impl AiQuestionOption {
    pub(crate) fn new(id: impl Into<String>, label: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
        }
    }
}

/// One assistant question (`AiQuestion`, `types/ai.ts:185`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct AiQuestion {
    pub id: String,
    pub prompt: String,
    pub options: Vec<AiQuestionOption>,
    pub allow_multiple: bool,
}

impl AiQuestion {
    pub(crate) fn new(id: impl Into<String>, prompt: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            prompt: prompt.into(),
            options: Vec::new(),
            allow_multiple: false,
        }
    }

    pub(crate) fn option(mut self, id: impl Into<String>, label: impl Into<String>) -> Self {
        self.options.push(AiQuestionOption::new(id, label));
        self
    }

    pub(crate) fn allow_multiple(mut self, allow_multiple: bool) -> Self {
        self.allow_multiple = allow_multiple;
        self
    }
}

/// One question's answer, mirroring React's `string | string[]` state:
/// a single-select answer is the option id or free text, a multi-select
/// answer is the list of chosen option ids.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Answer {
    Single(String),
    Multiple(Vec<String>),
}

/// The mutable state behind a [`QuestionBlock`]: answers, per-question
/// custom-text drafts, and the submitted flag with its payload.
///
/// The caller owns one of these (in `AppState` once the panel lands; in the
/// probe state in tests) and hands `&state` to the builder each frame.
/// Behaviour mirrors `QuestionBlock`'s React hooks exactly; see
/// [`QuestionBlockState::toggle`] and [`QuestionBlockState::submit`].
#[derive(Clone, Debug, Default)]
pub(crate) struct QuestionBlockState {
    answers: HashMap<String, Answer>,
    submitted: bool,
    payload: Option<String>,
    custom: HashMap<String, TextEdit>,
    /// The draft text last mirrored into `answers`, per question. The sync
    /// only writes when the draft moved, so a cleared draft does not wipe an
    /// option the user picked after typing.
    custom_synced: HashMap<String, String>,
}

impl QuestionBlockState {
    /// React's initial answers: `[]` for multi-select, `''` otherwise.
    pub(crate) fn new(questions: &[AiQuestion]) -> Self {
        let mut state = Self::default();
        for question in questions {
            state.answers.insert(
                question.id.clone(),
                if question.allow_multiple {
                    Answer::Multiple(Vec::new())
                } else {
                    Answer::Single(String::new())
                },
            );
            state
                .custom
                .insert(question.id.clone(), TextEdit::new(String::new()));
            state
                .custom_synced
                .insert(question.id.clone(), String::new());
        }
        state
    }

    /// The current answer for a question, if the question is known.
    pub(crate) fn answer(&self, question_id: &str) -> Option<&Answer> {
        self.answers.get(question_id)
    }

    /// Whether the block was submitted (React's `submitted` state).
    pub(crate) fn submitted(&self) -> bool {
        self.submitted
    }

    /// The formatted payload of the submit, if one happened.
    pub(crate) fn payload(&self) -> Option<&str> {
        self.payload.as_deref()
    }

    /// The custom-text draft for a question, for tests and for the field's
    /// edit accessor.
    pub(crate) fn custom_text(&self, question_id: &str) -> &str {
        self.custom
            .get(question_id)
            .map(|edit| edit.text.as_str())
            .unwrap_or("")
    }

    /// React's `toggleOption`: multi-select adds or removes the option,
    /// single-select picks it or clears it when pressed again.
    ///
    /// Pressing an option also clears the custom draft, matching React's
    /// derived input value (the input shows `''` once the answer is an
    /// option id or a list).
    pub(crate) fn toggle(&mut self, question_id: &str, option_id: &str, allow_multiple: bool) {
        if allow_multiple {
            match self.answers.entry(question_id.to_string()) {
                std::collections::hash_map::Entry::Occupied(mut entry) => match entry.get_mut() {
                    Answer::Multiple(ids) => {
                        if let Some(position) = ids.iter().position(|id| id == option_id) {
                            ids.remove(position);
                        } else {
                            ids.push(option_id.to_string());
                        }
                    }
                    answer @ Answer::Single(_) => {
                        *answer = Answer::Multiple(vec![option_id.to_string()]);
                    }
                },
                std::collections::hash_map::Entry::Vacant(entry) => {
                    entry.insert(Answer::Multiple(vec![option_id.to_string()]));
                }
            }
        } else {
            let already = matches!(
                self.answers.get(question_id),
                Some(Answer::Single(current)) if current == option_id
            );
            self.answers.insert(
                question_id.to_string(),
                Answer::Single(if already {
                    String::new()
                } else {
                    option_id.to_string()
                }),
            );
        }
        self.custom
            .insert(question_id.to_string(), TextEdit::new(String::new()));
        self.custom_synced
            .insert(question_id.to_string(), String::new());
    }

    /// Mirror one question's draft into its answer, when the draft moved.
    /// React's `setTextAnswer` overwrites the answer on every keystroke —
    /// including with `''` when the user erases the text — so the sync writes
    /// the draft verbatim, not just when it is non-empty.
    fn sync_custom(&mut self, question_id: &str) {
        let current = self
            .custom
            .get(question_id)
            .map(|edit| edit.text.clone())
            .unwrap_or_default();
        if self.custom_synced.get(question_id).map(String::as_str) != Some(current.as_str()) {
            self.answers
                .insert(question_id.to_string(), Answer::Single(current.clone()));
            self.custom_synced.insert(question_id.to_string(), current);
        }
    }

    /// The edit accessor the custom-text field binds: returns the question's
    /// draft buffer, mirroring its current text into the answer first.
    pub(crate) fn custom_edit(&mut self, question_id: &str) -> &mut TextEdit {
        self.sync_custom(question_id);
        self.custom
            .entry(question_id.to_string())
            .or_insert_with(|| TextEdit::new(String::new()))
    }

    /// React's `handleSubmit`: collect the non-empty answers, format them,
    /// and mark the block submitted. The block is marked submitted only when
    /// at least one answer is non-empty — an all-empty block stays
    /// unsubmitted and reports nothing.
    ///
    /// Drafts are synced first (see the module docs): without this, a custom
    /// answer typed since the last accessor call would be missing from the
    /// payload.
    pub(crate) fn submit(&mut self, questions: &[AiQuestion]) -> Option<String> {
        for question in questions {
            self.sync_custom(&question.id);
        }
        let payload = Self::format_submission(questions, &self.answers)?;
        self.submitted = true;
        self.payload = Some(payload.clone());
        Some(payload)
    }

    /// The pure submission format, mirroring `handleSubmit` line by line:
    /// one `[{id}] {prompt}\n{labels or text}` line per answered question,
    /// joined with `"\n\n"`. Multi-select labels join with `", "`. An answer
    /// that names no option (free text, or a stale id) formats as itself.
    /// Empty answers — an empty multi list, or a blank-or-whitespace single
    /// string — are skipped; all empty yields `None`.
    pub(crate) fn format_submission(
        questions: &[AiQuestion],
        answers: &HashMap<String, Answer>,
    ) -> Option<String> {
        let mut lines: Vec<String> = Vec::new();
        for question in questions {
            match answers.get(&question.id) {
                Some(Answer::Multiple(ids)) if question.allow_multiple && !ids.is_empty() => {
                    let labels: Vec<String> = ids
                        .iter()
                        .map(|id| {
                            question
                                .options
                                .iter()
                                .find(|option| &option.id == id)
                                .map(|option| option.label.clone())
                                .unwrap_or_else(|| id.clone())
                        })
                        .collect();
                    lines.push(format!(
                        "[{}] {}\n{}",
                        question.id,
                        question.prompt,
                        labels.join(", ")
                    ));
                }
                Some(Answer::Single(text)) if !text.trim().is_empty() => {
                    let shown = question
                        .options
                        .iter()
                        .find(|option| &option.id == text)
                        .map(|option| option.label.clone())
                        .unwrap_or_else(|| text.clone());
                    lines.push(format!("[{}] {}\n{}", question.id, question.prompt, shown));
                }
                _ => {}
            }
        }
        if lines.is_empty() {
            None
        } else {
            Some(lines.join("\n\n"))
        }
    }
}

/// The exported `QuestionBlock` (AiChatPanel.tsx:1213–1321): one prompt per
/// question with option toggle buttons, a custom-answer text field, and a
/// submit button. The caller owns the [`QuestionBlockState`] and wires the
/// three callbacks; the block itself keeps no state.
pub(crate) struct QuestionBlock<'a, S: 'static> {
    questions: Vec<AiQuestion>,
    state: &'a QuestionBlockState,
    on_toggle: Option<Rc<dyn Fn(&mut S, &str, &str, bool)>>,
    on_custom_edit: Option<Rc<dyn for<'b, 'c> Fn(&'b mut S, &'c str) -> &'b mut TextEdit>>,
    on_submit: Option<Rc<dyn Fn(&mut S)>>,
}

impl<'a, S: 'static> QuestionBlock<'a, S> {
    pub(crate) fn new(questions: Vec<AiQuestion>, state: &'a QuestionBlockState) -> Self {
        Self {
            questions,
            state,
            on_toggle: None,
            on_custom_edit: None,
            on_submit: None,
        }
    }

    /// Called with `(question id, option id, allow_multiple)` when an option
    /// button is pressed; the caller drives
    /// [`QuestionBlockState::toggle`].
    pub(crate) fn on_toggle(mut self, f: impl Fn(&mut S, &str, &str, bool) + 'static) -> Self {
        self.on_toggle = Some(Rc::new(f));
        self
    }

    /// The per-question edit accessor for the custom-answer fields, e.g.
    /// `|state, id| state.block.custom_edit(id)`; see
    /// [`QuestionBlockState::custom_edit`].
    pub(crate) fn on_custom_edit(
        mut self,
        f: impl for<'b, 'c> Fn(&'b mut S, &'c str) -> &'b mut TextEdit + 'static,
    ) -> Self {
        self.on_custom_edit = Some(Rc::new(f));
        self
    }

    /// Called when the submit button is pressed; the caller drives
    /// [`QuestionBlockState::submit`] with the same questions.
    pub(crate) fn on_submit(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_submit = Some(Rc::new(f));
        self
    }

    fn option_selected(&self, question: &AiQuestion, option_id: &str) -> bool {
        match self.state.answers.get(&question.id) {
            Some(Answer::Multiple(ids)) => ids.iter().any(|id| id == option_id),
            Some(Answer::Single(current)) => current == option_id,
            None => false,
        }
    }

    pub(crate) fn build(self, dark: bool) -> El {
        // React returns null once submitted; the Rune shell keeps the key
        // with no children so the parent's slot stays stable, folding away
        // over successive frames (see the module docs) rather than
        // vanishing instantly.
        if self.state.submitted {
            return col().key("question-block").w_full().min_w(0.0);
        }
        let k = kit(dark);
        let accent = k.c.text_accent;
        let muted = k.c.text_muted;

        let mut block = col()
            .key("question-block")
            .w_full()
            .min_w(0.0)
            .gap(12.0)
            .padding(10.0)
            .border(1.0, k.alpha(accent, 0.20))
            .fill(k.alpha(k.c.bg_accent, 0.05))
            .radius(k.radius.md);

        for question in &self.questions {
            let mut item = col()
                .key(format!("question-block-item-{}", question.id))
                .w_full()
                .min_w(0.0)
                .gap(6.0);
            item = item.child(
                text(question.prompt.clone())
                    .key(format!("question-block-item-{}-prompt", question.id))
                    .font(k.ty.text_small_medium)
                    .color(k.c.text_muted)
                    .wrap(),
            );

            if !question.options.is_empty() {
                let mut options = row()
                    .key(format!("question-block-item-{}-options", question.id))
                    .gap(6.0)
                    .flex_wrap()
                    .min_w(0.0);
                for option in &question.options {
                    let selected = self.option_selected(question, &option.id);
                    let option_label = option.label.clone();
                    let mut button = Button::new(option.label.clone())
                        .variant(ButtonVariant::Ghost)
                        .control_size(ControlSize::Small)
                        .label_with(move |font, _ink| {
                            text(option_label.clone()).font(font).color(if selected {
                                accent
                            } else {
                                muted
                            })
                        })
                        .build(k)
                        // No `aria-pressed` hook exists in Rune, and a
                        // Button is not an a11y `selectable_item`; the
                        // selected state rides `focus_selected` (see module
                        // docs) and is observed via `item_selected` + fill.
                        .focus_selected(selected)
                        .key(format!("question-option-{}-{}", question.id, option.id))
                        .fill(if selected {
                            k.alpha(accent, 0.15)
                        } else {
                            Color::TRANSPARENT
                        })
                        .border(1.0, if selected { accent } else { k.c.border_base });
                    if let Some(on_toggle) = &self.on_toggle {
                        let press_qid = question.id.clone();
                        let press_oid = option.id.clone();
                        let key_qid = question.id.clone();
                        let key_oid = option.id.clone();
                        let allow_multiple = question.allow_multiple;
                        let press = Rc::clone(on_toggle);
                        let keyed = Rc::clone(on_toggle);
                        // Kit buttons fire on Space; Enter is wired
                        // explicitly, as on the chip's remove button.
                        button = button
                            .on_press(move |state: &mut S| {
                                press(state, &press_qid, &press_oid, allow_multiple)
                            })
                            .on_key(move |state: &mut S, event: &KeyEvent, _tx: &mut Tx| {
                                if event.key == Key::Enter {
                                    keyed(state, &key_qid, &key_oid, allow_multiple);
                                    true
                                } else {
                                    false
                                }
                            });
                    }
                    options = options.child(button);
                }
                item = item.child(options);
            }

            // The custom-answer field. Its value is the draft in state; the
            // state machine keeps the draft and the answer in sync (see the
            // module docs), so the field shows what React's derived value
            // would show.
            let input = match self.state.custom.get(&question.id) {
                Some(edit) => {
                    let mut field =
                        TextField::new(format!("question-custom-input-{}", question.id), edit)
                            .height(28.0)
                            .pad_x(8.0)
                            .placeholder(t("chat.questions.customAnswer"));
                    if let Some(accessor) = &self.on_custom_edit {
                        let accessor = Rc::clone(accessor);
                        let question_id = question.id.clone();
                        field = field.on_edit(move |state: &mut S| accessor(state, &question_id));
                    }
                    field.build(dark)
                }
                // Defensive: state built without this question's draft
                // (unreachable via `QuestionBlockState::new`). Keep the
                // keyed slot so the layout does not shift.
                None => row()
                    .key(format!("question-custom-input-{}", question.id))
                    .h(28.0)
                    .w_full()
                    .min_w(0.0)
                    .child(
                        text(t("chat.questions.customAnswer"))
                            .font(k.ty.text_xsmall)
                            .color(k.c.text_faint),
                    ),
            };
            item = item.child(input);
            block = block.child(item);
        }

        // React's primary button is kit `Contrast` here, the variant the
        // dialog footers already use for their confirm action.
        let mut submit = Button::new(t("chat.questions.submit"))
            .variant(ButtonVariant::Contrast)
            .control_size(ControlSize::Small)
            .icon(&ICON_SEND)
            .help(t("chat.questions.submit"))
            .build(k)
            .key("question-submit");
        if let Some(on_submit) = &self.on_submit {
            let press = Rc::clone(on_submit);
            let keyed = Rc::clone(on_submit);
            submit = submit.on_press(move |state: &mut S| press(state)).on_key(
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
        block.child(submit)
    }
}
