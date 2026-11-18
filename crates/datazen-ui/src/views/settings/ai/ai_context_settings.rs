//! Port of `packages/frontend/src/windows/settings/ai/AiContextSettings.tsx`.
//!
//! The AI context settings section: a global-instructions textarea with a byte
//! budget, a prompt-template list with per-template and total byte budgets,
//! and an inline template editor (name, comma-separated database types,
//! content, a default checkbox) with React's three rejections.
//!
//! ## What maps where
//!
//! - `useAiStore` (`saveSettingsConfig`, `saving`) is a host concern, not
//!   Rune's: this component takes an [`AiContextState`] projection plus
//!   callbacks that mutate the owning state `S`, the pattern of
//!   `components/history/GlobalQueryHistoryDialog`. The host's save callback
//!   checks [`AiContextState::save_payload`], persists, records
//!   `saved_until = tx.clock() + 2.0` and calls `tx.wake_after(2s)` — the same
//!   deadline mechanism `ui/copyable.rs` uses for its 1.5s "copied" feedback.
//!   The view reads `saved` as a bool the caller computes from the deadline,
//!   so it never needs the clock.
//! - React's `crypto.randomUUID()` for a new template's id is host-side too:
//!   the `on_begin_create` callback generates the id and applies
//!   [`AiContextState::begin_create`].
//! - `useState`/`useEffect` become [`AiContextState`]: `from_config` mounts
//!   and `resync_from_config` is the effect that resets the two drafts when
//!   the config prop changes. React's effect does not close an open editor;
//!   neither does the resync.
//! - `byteLength` (`new TextEncoder().encode(value).length`) is a Rust
//!   `str`'s UTF-8 `len` ([`byte_len`]). `parseTypes` splits on commas, trims
//!   and drops empties — at apply time, where React re-parses on every
//!   keystroke; the visible text keeps what was typed until the editor
//!   confirms (React normalises the separator spacing live, Rune at confirm).
//! - React's `setTimeout(() => setSaved(false), 2000)` is the `saved_until`
//!   deadline; `cancel_editor` also keeps a stale `editor_error` exactly as
//!   React's `setEditing(null)` does (it is invisible once the editor closes
//!   and the next `begin_*` clears it).
//! - `SectionTitle`/`SettingHint` (settingsUi.tsx / SettingHint.tsx) are later
//!   tasks (T397/T379); until they land the titles render here in React's
//!   SectionTitle style (13/600 muted) and the hint rides the existing
//!   `ui/tooltip.rs` "?" trigger.
//!
//! Known deltas (documented, not silent): kit button sizes stand in for
//! Tailwind's pixel classes (h-6 → `Regular` 24pt icon buttons, h-7 → `Large`
//! 28pt, the h-8 save button → `Large` 28pt); the delete button renders plain
//! ghost rather than React's red-tinted ghost (`IconButtonVariant` has no
//! danger variant); the textareas' border does not tint on focus (the caret
//! marks focus); `resize-y` has no Rune counterpart; `uppercase
//! tracking-wider` renders without case or tracking transforms (the zh-CN
//! strings carry no case).
//!
//! ## Keys
//!
//! React `data-testid` values stay verbatim; unlabeled parts derive from the
//! component key (`key`, default `"ai-context-settings"`):
//!
//! | part | key |
//! |------|-----|
//! | root | `<key>` |
//! | instructions title / hint | `<key>-instructions-title`, `<key>-instructions-hint` |
//! | instructions textarea / counter | `ai-global-instructions`, `ai-global-instructions-count` |
//! | templates title / hint / add | `<key>-templates-title`, `<key>-templates-hint`, `<key>-add-template` |
//! | empty line | `ai-templates-empty` |
//! | template row / badge / sub-line | `ai-template-row-{id}`, `ai-template-row-{id}-default`, `ai-template-row-{id}-sub` |
//! | row edit / delete | `ai-template-row-{id}-edit`, `ai-template-row-{id}-delete` |
//! | total counter | `ai-templates-total-count` |
//! | editor box / name / types / content / counter | `ai-template-editor`, `ai-template-name`, `ai-template-database-types`, `ai-template-content`, `ai-template-content-count` |
//! | default checkbox / error / cancel / confirm | `<key>-default`, `<key>-editor-error`, `<key>-cancel`, `<key>-confirm` |
//! | save button / over-limit hint | `ai-context-save`, `<key>-over-limit` |
//!
//! All colours come from the kit palette; there are no raw colour literals
//! here. Icons come from `icons.rs` (`ICON_PENCIL` is this port's registry
//! addition, the lucide 0.487.0 pencil).

use std::rc::Rc;
use std::time::Duration;

use rune::{col, row, text, text_input, El, TextEdit, TextStyle, Tx};
use rune_kit::{Button, ButtonVariant, ControlSize, IconButton, IconButtonVariant, Kit};

use crate::i18n::t;
use crate::icons::{ICON_PENCIL, ICON_PLUS, ICON_TRASH};
use crate::ui::field::TextField;
use crate::ui::toggle::Checkbox;
use crate::ui::tooltip::Tooltip;

/// React `MAX_GLOBAL_BYTES`: 4 * 1024.
#[allow(dead_code)] // reusable port; wired by the settings AI section track (T170)
pub(crate) const MAX_GLOBAL_BYTES: usize = 4 * 1024;
/// React `MAX_TEMPLATE_BYTES`: 8 * 1024.
#[allow(dead_code)] // reusable port; wired by the settings AI section track (T170)
pub(crate) const MAX_TEMPLATE_BYTES: usize = 8 * 1024;
/// React `MAX_TOTAL_TEMPLATE_BYTES`: 16 * 1024.
#[allow(dead_code)] // reusable port; wired by the settings AI section track (T170)
pub(crate) const MAX_TOTAL_TEMPLATE_BYTES: usize = 16 * 1024;
/// React's saved-feedback timeout (`setTimeout(..., 2000)`), for the host's
/// `tx.wake_after` and `saved_until` deadline.
#[allow(dead_code)] // reusable port; wired by the settings AI section track (T170)
pub(crate) const SAVED_FEEDBACK: Duration = Duration::from_millis(2000);

/// React `text-[9px]` (the default badge).
#[allow(dead_code)] // reusable port; wired by the settings AI section track (T170)
const TEXT_9: TextStyle = TextStyle::new(9.0, 400.0);
/// React `text-[10px]` (a template row's sub-line).
#[allow(dead_code)] // reusable port; wired by the settings AI section track (T170)
const TEXT_10: TextStyle = TextStyle::new(10.0, 400.0);
/// React `text-[11px]` (counters, the error line, the over-limit hint).
#[allow(dead_code)] // reusable port; wired by the settings AI section track (T170)
const TEXT_11: TextStyle = TextStyle::new(11.0, 400.0);
/// React `text-xs` at default weight (the editor's content textarea).
#[allow(dead_code)] // reusable port; wired by the settings AI section track (T170)
const TEXT_12: TextStyle = TextStyle::new(12.0, 400.0);

/// React `byteLength`: `new TextEncoder().encode(value).length`. A Rust
/// `str` is UTF-8, so its byte length is that count.
#[allow(dead_code)] // reusable port; wired by the settings AI section track (T170)
pub(crate) fn byte_len(value: &str) -> usize {
    value.len()
}

/// React `parseTypes`: comma-separated input → trimmed, non-empty segments.
#[allow(dead_code)] // reusable port; wired by the settings AI section track (T170)
pub(crate) fn parse_types(value: &str) -> Vec<String> {
    value
        .split(',')
        .map(str::trim)
        .filter(|item| !item.is_empty())
        .map(str::to_owned)
        .collect()
}

/// `PromptTemplate` from `packages/frontend/src/types/ai.ts`. The kernel's
/// `datazen::ai::domain::types::PromptTemplate` is the same shape, but
/// `datazen-ui` does not depend on that crate; the host wiring converts at
/// its seam (the field names and serde casing line up one to one).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[allow(dead_code)] // reusable port; wired by the settings AI section track
pub(crate) struct PromptTemplateVm {
    pub(crate) id: String,
    pub(crate) name: String,
    pub(crate) content: String,
    /// Empty means the template is eligible for every database type.
    pub(crate) database_types: Vec<String>,
    pub(crate) is_default: bool,
}

/// The editor's three rejections, translated at render exactly where React
/// calls `setEditorError(t(...))`.
#[allow(dead_code)] // reusable port; wired by the settings AI section track (T170)
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum EditorError {
    /// `templateNameRequired`
    NameRequired,
    /// `templateContentRequired`
    ContentRequired,
    /// `templateTooLarge`, with `{max}` = `MAX_TEMPLATE_BYTES`
    ContentTooLarge,
}

#[allow(dead_code)] // reusable port; wired by the settings AI section track (T170)
impl EditorError {
    pub(crate) fn message(&self) -> String {
        match self {
            EditorError::NameRequired => t("settings.ai.templateNameRequired").to_owned(),
            EditorError::ContentRequired => t("settings.ai.templateContentRequired").to_owned(),
            EditorError::ContentTooLarge => t("settings.ai.templateTooLarge")
                .replace("{max}", &MAX_TEMPLATE_BYTES.to_string()),
        }
    }
}

/// The open editor's draft: the template being edited (identity and flags)
/// plus its three fields as `TextEdit`s the shell can focus.
#[allow(dead_code)] // reusable port; wired by the settings AI section track (T170)
#[derive(Clone, Debug)]
pub(crate) struct TemplateEditor {
    pub(crate) template: PromptTemplateVm,
    pub(crate) name: TextEdit,
    pub(crate) types: TextEdit,
    pub(crate) content: TextEdit,
}

#[allow(dead_code)] // reusable port; wired by the settings AI section track (T170)
impl TemplateEditor {
    /// React `newTemplate()`: blank fields, a fresh id (the caller supplies
    /// it; `crypto.randomUUID()` is a host concern), nothing default.
    fn blank(id: impl Into<String>) -> Self {
        Self {
            template: PromptTemplateVm {
                id: id.into(),
                name: String::new(),
                content: String::new(),
                database_types: Vec::new(),
                is_default: false,
            },
            name: TextEdit::new(String::new()),
            types: TextEdit::new(String::new()),
            content: TextEdit::new(String::new()),
        }
    }

    /// React `beginEdit`: the row's template cloned into the editor, its
    /// types joined back into editable text.
    fn from_template(template: PromptTemplateVm) -> Self {
        let types = template.database_types.join(", ");
        Self {
            name: TextEdit::new(template.name.clone()),
            types: TextEdit::new(types),
            content: TextEdit::new(template.content.clone()),
            template,
        }
    }
}

/// Every `useState` of the React component, kept in the owning state `S`.
#[allow(dead_code)] // reusable port; wired by the settings AI section track (T170)
#[derive(Clone, Debug)]
pub(crate) struct AiContextState {
    pub(crate) instructions: TextEdit,
    pub(crate) templates: Vec<PromptTemplateVm>,
    pub(crate) editing: Option<TemplateEditor>,
    pub(crate) editing_is_new: bool,
    pub(crate) editor_error: Option<EditorError>,
    /// When the "saved" label expires (`tx.clock() + 2.0`); `None` means no
    /// save has landed. Read via [`Self::saved`].
    pub(crate) saved_until: Option<f64>,
}

impl Default for AiContextState {
    fn default() -> Self {
        Self {
            instructions: TextEdit::new(String::new()),
            templates: Vec::new(),
            editing: None,
            editing_is_new: false,
            editor_error: None,
            saved_until: None,
        }
    }
}

#[allow(dead_code)] // reusable port; wired by the settings AI section track (T170)
impl AiContextState {
    /// Mount: the drafts start from the config prop's two fields.
    pub(crate) fn from_config(instructions: &str, templates: &[PromptTemplateVm]) -> Self {
        Self {
            instructions: TextEdit::new(instructions.to_owned()),
            templates: templates.to_vec(),
            ..Self::default()
        }
    }

    /// The `useEffect` on `[config.globalInstructions, config.promptTemplates]`:
    /// reset the two drafts. An open editor stays open, as in React.
    pub(crate) fn resync_from_config(&mut self, instructions: &str, templates: &[PromptTemplateVm]) {
        self.instructions = TextEdit::new(instructions.to_owned());
        self.templates = templates.to_vec();
    }

    pub(crate) fn instruction_bytes(&self) -> usize {
        byte_len(&self.instructions.text)
    }

    pub(crate) fn template_bytes(template: &PromptTemplateVm) -> usize {
        byte_len(&template.content)
    }

    pub(crate) fn total_template_bytes(&self) -> usize {
        self.templates.iter().map(|t| byte_len(&t.content)).sum()
    }

    /// React `overLimit`: any of the three byte budgets exceeded.
    pub(crate) fn over_limit(&self) -> bool {
        self.instruction_bytes() > MAX_GLOBAL_BYTES
            || self.total_template_bytes() > MAX_TOTAL_TEMPLATE_BYTES
            || self.templates.iter().any(|t| byte_len(&t.content) > MAX_TEMPLATE_BYTES)
    }

    /// The saved label shows while the deadline has not passed.
    pub(crate) fn saved(&self, now: f64) -> bool {
        self.saved_until.is_some_and(|until| until > now)
    }

    /// React `beginCreate`: a blank template under the given id, fresh editor.
    pub(crate) fn begin_create(&mut self, id: impl Into<String>) {
        self.editing = Some(TemplateEditor::blank(id));
        self.editing_is_new = true;
        self.editor_error = None;
    }

    /// React `beginEdit`: clone the row's template into the editor. An
    /// unknown id is a no-op (React cannot click a row that is not there).
    pub(crate) fn begin_edit(&mut self, id: &str) {
        if let Some(template) = self.templates.iter().find(|t| t.id == id).cloned() {
            self.editing = Some(TemplateEditor::from_template(template));
            self.editing_is_new = false;
            self.editor_error = None;
        }
    }

    /// The cancel press: `setEditing(null)` — React leaves a stale
    /// `editorError` in state (invisible with the editor closed; the next
    /// `begin_*` clears it), and so does this.
    pub(crate) fn cancel_editor(&mut self) {
        self.editing = None;
    }

    /// React `applyEditor`: validate the draft, then push it into the list
    /// (or replace the row with the same id) and close. `false` keeps the
    /// editor open with `editor_error` naming the rejection. A closed editor
    /// (React's `if (!editing) return`) is also `false`, changing nothing.
    pub(crate) fn apply_editor(&mut self) -> bool {
        let Some(editor) = self.editing.as_ref() else {
            return false;
        };
        let name = editor.name.text.trim().to_owned();
        if name.is_empty() {
            self.editor_error = Some(EditorError::NameRequired);
            return false;
        }
        if editor.content.text.trim().is_empty() {
            self.editor_error = Some(EditorError::ContentRequired);
            return false;
        }
        if byte_len(&editor.content.text) > MAX_TEMPLATE_BYTES {
            self.editor_error = Some(EditorError::ContentTooLarge);
            return false;
        }
        let mut template = editor.template.clone();
        template.name = name;
        template.database_types = parse_types(&editor.types.text);
        template.content = editor.content.text.clone();
        if self.editing_is_new {
            self.templates.push(template);
        } else if let Some(slot) = self.templates.iter_mut().find(|t| t.id == template.id) {
            *slot = template;
        }
        self.editing = None;
        self.editor_error = None;
        true
    }

    /// React `removeTemplate`: drop the row with the id.
    pub(crate) fn remove_template(&mut self, id: &str) {
        self.templates.retain(|t| t.id != id);
    }

    /// The default-checkbox press: flip the draft's `isDefault`.
    pub(crate) fn toggle_default(&mut self) {
        if let Some(editor) = self.editing.as_mut() {
            editor.template.is_default = !editor.template.is_default;
        }
    }

    /// React `handleSave`'s guard: the press is a no-op while `overLimit`.
    /// The payload is what `saveSettingsConfig` receives for this
    /// component's slice of the config (the spread keeps the rest).
    pub(crate) fn save_payload(&self) -> Option<(&str, &[PromptTemplateVm])> {
        if self.over_limit() {
            return None;
        }
        Some((&self.instructions.text, &self.templates))
    }
}

type Act<S> = Rc<dyn Fn(&mut S) + 'static>;
type IdAct<S> = Rc<dyn Fn(&mut S, String) + 'static>;
type SaveAct<S> = Rc<dyn Fn(&mut S, &mut Tx) + 'static>;
type Field<S> = Rc<dyn for<'b> Fn(&'b mut S) -> &'b mut TextEdit + 'static>;

/// Builder for the AI context settings section, generic over the owning
/// state `S` so every callback writes the state that holds the
/// [`AiContextState`].
#[allow(dead_code)] // reusable port; wired by the settings AI section track (T170)
pub(crate) struct AiContextSettings<'a, S: 'static> {
    key: &'a str,
    state: &'a AiContextState,
    saving: bool,
    /// Caller-computed from `state.saved(now)`; the view never reads a clock.
    saved: bool,
    begin_create: Option<Act<S>>,
    begin_edit: Option<IdAct<S>>,
    remove_template: Option<IdAct<S>>,
    apply_editor: Option<Act<S>>,
    cancel_editor: Option<Act<S>>,
    toggle_default: Option<Act<S>>,
    save: Option<SaveAct<S>>,
    instructions_edit: Option<Field<S>>,
    editor_name: Option<Field<S>>,
    editor_types: Option<Field<S>>,
    editor_content: Option<Field<S>>,
}

#[allow(dead_code)] // reusable port; wired by the settings AI section track (T170)
impl<'a, S: 'static> AiContextSettings<'a, S> {
    pub(crate) fn new(key: &'a str, state: &'a AiContextState) -> Self {
        Self {
            key,
            state,
            saving: false,
            saved: false,
            begin_create: None,
            begin_edit: None,
            remove_template: None,
            apply_editor: None,
            cancel_editor: None,
            toggle_default: None,
            save: None,
            instructions_edit: None,
            editor_name: None,
            editor_types: None,
            editor_content: None,
        }
    }

    /// The store's `saving` flag (disables the save button).
    pub(crate) fn saving(mut self, saving: bool) -> Self {
        self.saving = saving;
        self
    }

    /// Whether the 2s saved deadline is still ahead (the save button's label).
    pub(crate) fn saved(mut self, saved: bool) -> Self {
        self.saved = saved;
        self
    }

    /// "Add template": the host generates the new id (`crypto.randomUUID()`)
    /// and applies `begin_create(id)`.
    pub(crate) fn on_begin_create(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.begin_create = Some(Rc::new(f));
        self
    }

    pub(crate) fn on_begin_edit(mut self, f: impl Fn(&mut S, String) + 'static) -> Self {
        self.begin_edit = Some(Rc::new(f));
        self
    }

    pub(crate) fn on_remove_template(mut self, f: impl Fn(&mut S, String) + 'static) -> Self {
        self.remove_template = Some(Rc::new(f));
        self
    }

    pub(crate) fn on_apply_editor(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.apply_editor = Some(Rc::new(f));
        self
    }

    pub(crate) fn on_cancel_editor(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.cancel_editor = Some(Rc::new(f));
        self
    }

    pub(crate) fn on_toggle_default(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.toggle_default = Some(Rc::new(f));
        self
    }

    /// The save press: check `save_payload`, persist, then record
    /// `saved_until = tx.clock() + 2.0` and `tx.wake_after(SAVED_FEEDBACK)`.
    pub(crate) fn on_save(mut self, f: impl Fn(&mut S, &mut Tx) + 'static) -> Self {
        self.save = Some(Rc::new(f));
        self
    }

    /// The instructions textarea's `TextEdit` in `S`.
    pub(crate) fn instructions_edit(
        mut self,
        f: impl for<'b> Fn(&'b mut S) -> &'b mut TextEdit + Clone + 'static,
    ) -> Self {
        self.instructions_edit = Some(Rc::new(f));
        self
    }

    /// The editor's name field in `S`.
    pub(crate) fn editor_name(
        mut self,
        f: impl for<'b> Fn(&'b mut S) -> &'b mut TextEdit + Clone + 'static,
    ) -> Self {
        self.editor_name = Some(Rc::new(f));
        self
    }

    /// The editor's database-types field in `S`.
    pub(crate) fn editor_types(
        mut self,
        f: impl for<'b> Fn(&'b mut S) -> &'b mut TextEdit + Clone + 'static,
    ) -> Self {
        self.editor_types = Some(Rc::new(f));
        self
    }

    /// The editor's content field in `S`.
    pub(crate) fn editor_content(
        mut self,
        f: impl for<'b> Fn(&'b mut S) -> &'b mut TextEdit + Clone + 'static,
    ) -> Self {
        self.editor_content = Some(Rc::new(f));
        self
    }

    /// Build the section. The two section hints are tooltips, which live in
    /// the floating `layer` the caller composes into its overlay (the modal
    /// layer pattern).
    pub(crate) fn build(self, dark: bool, layer: &mut Vec<El>) -> El {
        let k = crate::ui::kit::kit(dark);
        let state = self.state;
        let over_limit = state.over_limit();

        // --- Global instructions (React: space-y-2) ---
        let instructions = col()
            .key("ai-context-instructions")
            .w_full()
            .gap(8.0)
            .child(section_title(
                &k,
                layer,
                "ai-context-instructions-title",
                t("settings.ai.instructionsTitle"),
                t("settings.ai.instructionsHint"),
            ))
            .child(textarea(
                "ai-global-instructions",
                &state.instructions,
                rune::tok::type_::BODY,
                t("settings.ai.instructionsPlaceholder"),
                88.0,
                self.instructions_edit.as_ref(),
                &k,
            ))
            .child(counter_line(
                "ai-global-instructions-count",
                state.instruction_bytes(),
                MAX_GLOBAL_BYTES,
                true,
                &k,
            ));

        // --- Prompt templates (React: space-y-2) ---
        let mut add = Button::new(t("settings.ai.addTemplate"))
            .variant(ButtonVariant::Neutral)
            .control_size(ControlSize::Large)
            .icon(&ICON_PLUS);
        if let Some(begin_create) = &self.begin_create {
            let begin_create = Rc::clone(begin_create);
            add = add.on_press(move |s: &mut S| begin_create(s));
        }
        let mut templates = col()
            .key("ai-context-templates")
            .w_full()
            .gap(8.0)
            .child(
                row()
                    .key("ai-context-templates-head")
                    .w_full()
                    .justify_between()
                    .center_y()
                    .child(section_title(
                        &k,
                        layer,
                        "ai-context-templates-title",
                        t("settings.ai.templatesTitle"),
                        t("settings.ai.templatesHint"),
                    ))
                    .child(add.build(&k).key("ai-context-add-template")),
            );
        if state.templates.is_empty() {
            templates = templates.child(
                text(t("settings.ai.templatesEmpty"))
                    .key("ai-templates-empty")
                    .font(rune::tok::type_::LABEL)
                    .color(k.c.text_muted),
            );
        }
        let mut list = col().key("ai-context-template-list").w_full().gap(6.0);
        for template in &state.templates {
            list = list.child(template_row(
                self.key,
                template,
                &k,
                &self.begin_edit,
                &self.remove_template,
            ));
        }
        templates = templates
            .child(list)
            .child(counter_line(
                "ai-templates-total-count",
                state.total_template_bytes(),
                MAX_TOTAL_TEMPLATE_BYTES,
                true,
                &k,
            ));

        // --- The inline editor, only while a draft is open ---
        let editor = state.editing.as_ref().map(|editor| {
            editor_box(
                self.key,
                state,
                editor,
                &self,
                &k,
            )
        });

        // --- The save row ---
        let save_label = if self.saving {
            t("settings.ai.saving")
        } else if self.saved {
            t("common.success")
        } else {
            t("settings.ai.saveContext")
        };
        let mut save = Button::new(save_label)
            .variant(ButtonVariant::Submit)
            .control_size(ControlSize::Large)
            .disabled(self.saving || over_limit);
        if let Some(save_act) = &self.save {
            let save_act = Rc::clone(save_act);
            save = save.on_press_tx(move |s: &mut S, tx: &mut Tx| save_act(s, tx));
        }
        let mut save_row = row()
            .key("ai-context-save-row")
            .w_full()
            .gap(8.0)
            .center_y()
            .child(save.build(&k).key("ai-context-save"));
        if over_limit {
            save_row = save_row.child(
                text(t("settings.ai.overLimitHint"))
                    .key("ai-context-over-limit")
                    .font(TEXT_11)
                    .color(k.c.danger.fg),
            );
        }

        let mut root = col()
            .key(self.key)
            .w_full()
            .gap(20.0)
            .child(instructions)
            .child(templates);
        if let Some(editor) = editor {
            root = root.child(editor);
        }
        root.child(save_row)
    }
}

/// A textarea: the raw multi-line input with React's `textarea` chrome
/// (`min-h w-full resize-y rounded border bg-surface px-2 py-1.5`). Plain
/// Return breaks the line, as a textarea does (Rune's input submits on
/// Return and breaks on Shift+Return by default, so the submit inserts the
/// newline).
fn textarea<S: 'static>(
    key: &str,
    edit: &TextEdit,
    style: TextStyle,
    placeholder: &str,
    min_h: f32,
    access: Option<&Field<S>>,
    k: &Kit,
) -> El {
    let mut field = text_input(edit, style)
        .key(key)
        .w_full()
        .min_h(min_h)
        .px(8.0)
        .py(6.0)
        .radius(k.radius.sm)
        .border(1.0, k.c.border_base)
        .fill(k.c.bg_base)
        .placeholder(placeholder.to_owned(), k.c.text_faint)
        .caret(k.c.text_base, k.selection());
    if let Some(access) = access {
        let for_edit = Rc::clone(access);
        let for_submit = Rc::clone(access);
        field = field
            .on_edit(move |s: &mut S| for_edit(s))
            .on_submit(move |s: &mut S, _| for_submit(s).insert("\n"));
    }
    field
}

/// React's section heading: 13/600 muted, with the SettingHint "?" trigger
/// riding the existing tooltip base while settingsUi.tsx is unported.
fn section_title(k: &Kit, layer: &mut Vec<El>, key: &str, title: &str, hint: &str) -> El {
    let mut heading = row()
        .key(key)
        .w_full()
        .gap(4.0)
        .center_y()
        .child(text(title.to_owned()).font(rune::tok::type_::TITLE).color(k.c.text_muted));
    if !hint.is_empty() {
        let mark = row()
            .key(format!("{key}-mark"))
            .size(16.0, 16.0)
            .center()
            .radius(8.0)
            .border(1.0, k.c.border_base)
            .child(text("?").font(TEXT_11).color(k.c.text_muted));
        heading = heading.child(
            Tooltip::new(format!("{key}-hint"))
                .trigger(mark)
                .text(hint.to_owned())
                .build(k, layer),
        );
    }
    heading
}

/// `{bytes} / {max} bytes`, muted or danger, with the over-limit hint
/// appended when `suffix_on_over` (the two page counters append it; the
/// editor's content counter does not).
fn counter_line(key: &str, bytes: usize, max: usize, suffix_on_over: bool, k: &Kit) -> El {
    let mut label = format!("{bytes} / {max} bytes");
    if bytes > max && suffix_on_over {
        label.push_str(" · ");
        label.push_str(t("settings.ai.overLimitHint"));
    }
    text(label)
        .key(key)
        .font(TEXT_11)
        .color(if bytes > max { k.c.danger.fg } else { k.c.text_muted })
}

/// One template row: name (plus the default badge) and the types · bytes
/// sub-line on the left, edit and delete icon buttons on the right.
fn template_row<S: 'static>(
    _root: &str,
    template: &PromptTemplateVm,
    k: &Kit,
    begin_edit: &Option<IdAct<S>>,
    remove_template: &Option<IdAct<S>>,
) -> El {
    let id = template.id.clone();
    let bytes = AiContextState::template_bytes(template);

    let mut name = row()
        .key(format!("ai-template-row-{id}-name"))
        .gap(6.0)
        .center_y()
        .min_w(0.0)
        .child(
            text(template.name.clone())
                .font(rune::tok::type_::LABEL)
                .color(k.c.text_base)
                .truncate(),
        );
    if template.is_default {
        name = name.child(
            row()
                .key(format!("ai-template-row-{id}-default"))
                .px(4.0)
                .radius(k.radius.xs)
                .fill(k.alpha(k.c.bg_accent, 0.15))
                .child(
                    text(t("settings.ai.defaultBadge"))
                        .font(TEXT_9)
                        .color(k.c.text_accent),
                ),
        );
    }

    let types = if template.database_types.is_empty() {
        t("settings.ai.allDatabaseTypes").to_owned()
    } else {
        template.database_types.join(", ")
    };
    let sub = row()
        .key(format!("ai-template-row-{id}-sub"))
        .gap(4.0)
        .min_w(0.0)
        .child(
            text(types)
                .font(TEXT_10)
                .color(k.c.text_muted)
                .truncate(),
        )
        .child(text("·").font(TEXT_10).color(k.c.text_muted))
        .child(
            text(format!("{bytes} / {MAX_TEMPLATE_BYTES} bytes"))
                .font(TEXT_10)
                .color(if bytes > MAX_TEMPLATE_BYTES {
                    k.c.danger.fg
                } else {
                    k.c.text_muted
                }),
        );

    let mut edit = IconButton::new(&ICON_PENCIL)
        .variant(IconButtonVariant::Ghost)
        .control_size(ControlSize::Regular)
        .label(t("common.edit"));
    if let Some(begin_edit) = begin_edit {
        let begin_edit = Rc::clone(begin_edit);
        let edit_id = id.clone();
        edit = edit.on_press(move |s: &mut S| begin_edit(s, edit_id.clone()));
    }
    let mut delete = IconButton::new(&ICON_TRASH)
        .variant(IconButtonVariant::Ghost)
        .control_size(ControlSize::Regular)
        .label(t("common.delete"));
    if let Some(remove_template) = remove_template {
        let remove_template = Rc::clone(remove_template);
        let delete_id = id.clone();
        delete = delete.on_press(move |s: &mut S| remove_template(s, delete_id.clone()));
    }

    row()
        .key(format!("ai-template-row-{id}"))
        .w_full()
        .gap(8.0)
        .center_y()
        .px(8.0)
        .py(6.0)
        .radius(k.radius.sm)
        .border(1.0, k.alpha(k.c.border_base, 0.7))
        .fill(k.c.bg_base)
        .child(
            col()
                .key(format!("ai-template-row-{id}-main"))
                .gap(2.0)
                .grow(1.0)
                .min_w(0.0)
                .child(name)
                .child(sub),
        )
        .child(edit.build(k).key(format!("ai-template-row-{id}-edit")))
        .child(delete.build(k).key(format!("ai-template-row-{id}-delete")))
}

/// The inline template editor: name, database types, content, byte counter,
/// the default checkbox, the rejection line and the cancel/confirm footer.
fn editor_box<'a, S: 'static>(
    _root: &str,
    state: &AiContextState,
    editor: &TemplateEditor,
    cmp: &AiContextSettings<'a, S>,
    k: &Kit,
) -> El {
    let mut name = TextField::new("ai-template-name", &editor.name)
        .height(24.0)
        .w_full()
        .border(1.0, k.c.border_base)
        .fill(k.c.bg_base)
        .placeholder(t("settings.ai.templateNamePlaceholder"));
    if let Some(access) = &cmp.editor_name {
        let access = Rc::clone(access);
        name = name.on_edit(move |s: &mut S| access(s));
    }
    let mut types = TextField::new("ai-template-database-types", &editor.types)
        .height(24.0)
        .w_full()
        .border(1.0, k.c.border_base)
        .fill(k.c.bg_base)
        .placeholder(t("settings.ai.templateDatabaseTypesPlaceholder"));
    if let Some(access) = &cmp.editor_types {
        let access = Rc::clone(access);
        types = types.on_edit(move |s: &mut S| access(s));
    }

    let mut default = Checkbox::new("ai-context-default", editor.template.is_default)
        .label(t("settings.ai.setDefault"));
    if let Some(toggle) = &cmp.toggle_default {
        let toggle = Rc::clone(toggle);
        default = default.on_toggle(move |s: &mut S| toggle(s));
    }

    let mut confirm = Button::new(t("common.confirm"))
        .variant(ButtonVariant::Neutral)
        .control_size(ControlSize::Large);
    if let Some(apply) = &cmp.apply_editor {
        let apply = Rc::clone(apply);
        confirm = confirm.on_press(move |s: &mut S| apply(s));
    }
    let mut cancel = Button::new(t("common.cancel"))
        .variant(ButtonVariant::Ghost)
        .control_size(ControlSize::Large);
    if let Some(cancel_act) = &cmp.cancel_editor {
        let cancel_act = Rc::clone(cancel_act);
        cancel = cancel.on_press(move |s: &mut S| cancel_act(s));
    }

    let mut box_el = col()
        .key("ai-template-editor")
        .w_full()
        .gap(8.0)
        .px(8.0)
        .py(8.0)
        .radius(k.radius.sm)
        .border(1.0, k.c.border_base)
        .fill(k.alpha(k.c.layer_01, 0.4))
        .child(name.build(k.dark))
        .child(types.build(k.dark))
        .child(textarea(
            "ai-template-content",
            &editor.content,
            TEXT_12,
            t("settings.ai.templateContentPlaceholder"),
            100.0,
            cmp.editor_content.as_ref(),
            k,
        ))
        .child(counter_line(
            "ai-template-content-count",
            byte_len(&editor.content.text),
            MAX_TEMPLATE_BYTES,
            false,
            k,
        ))
        .child(default.build(k));
    if let Some(error) = &state.editor_error {
        box_el = box_el.child(
            text(error.message())
                .key("ai-context-editor-error")
                .font(TEXT_11)
                .color(k.c.danger.fg),
        );
    }
    box_el.child(
        row()
            .key("ai-template-editor-foot")
            .w_full()
            .gap(6.0)
            .justify_end()
            .child(cancel.build(k).key("ai-context-cancel"))
            .child(confirm.build(k).key("ai-context-confirm")),
    )
}
