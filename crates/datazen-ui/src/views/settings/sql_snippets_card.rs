//! Port of `packages/frontend/src/windows/settings/SqlSnippetsCard.tsx`
//! (T426) — Settings → the SQL snippets card: the header's
//! add/export/import row, the status line, the custom snippet list
//! (edit/delete per row) and the builtin snippet list (duplicate per
//! row), over the T417 edit dialog and the confirm hook.
//!
//! This file also absorbs `lib/settingsExport.ts` (its only UI
//! consumer; the file has no shard line of its own): the safe
//! whitelist export [`export_editor_settings`] and the validating
//! import [`import_editor_settings`] are ported verbatim below.
//!
//! ## What maps where
//!
//! - The `settings` prop is [`EditorSettingsExportInput`] — the
//!   `AppSettings` fields the card reads: `sqlSnippets` (the custom
//!   list) and the whitelist fields the export copies. The host
//!   syncs it through [`SqlSnippetsCardState::resync`] (React's
//!   `useMemo` recomputes per render; the mirror re-reads per store
//!   change).
//! - The `dialogState` triple (`open`/`snippet`/`isDuplicate`) is the
//!   card state's `dialog_snippet`/`dialog_is_duplicate` plus the
//!   T417 draft's `open`; [`SqlSnippetsCardState::begin_add`] /
//!   [`Self::begin_edit`] / [`Self::begin_duplicate`] are the three
//!   `setDialogState` call sites (each seeds the draft the way the
//!   dialog's `useEffect` does) and [`Self::close_dialog`] is the
//!   `onClose` reset.
//! - `existingPrefixes` (the `useMemo` over builtin + custom minus
//!   the own non-duplicate prefix) is [`Self::existing_prefixes`].
//! - `handleSaveSnippet` is [`Self::save_snippet`]: the T417
//!   validation, then replace-by-id-or-append, then the close; the
//!   returned list is the `onUpdateSnippets` payload (the host
//!   persists). A duplicate saves under a fresh id; an edit keeps
//!   its id.
//! - `handleDeleteSnippet` is [`Self::request_delete`] +
//!   [`Self::resolve_confirm`]: the warning confirm with the
//!   `{prefix}` message; OK filters the id out and answers the
//!   updated list for `onUpdateSnippets`, every other answer keeps
//!   the list.
//! - `showStatus` + the 3s `statusTimerRef` are the
//!   `status_message`/`status_timer` pair: [`Self::show_status`]
//!   stamps the message and hands out the timer id (a newer status
//!   supersedes the old timer — `clearTimeout`), and
//!   [`Self::status_fired`] clears only the live id (the unmount
//!   cleanup is the host's teardown).
//! - `handleExport`/`handleImport` split at the clipboard seam: the
//!   buttons fire the host slots, the host builds the JSON through
//!   [`Self::export_json`] (its `exportedAt` argument is React's
//!   `new Date().toISOString()` — the clock is the host's), writes
//!   or reads the clipboard, and reports through
//!   [`Self::note_export_ok`] / [`Self::note_export_error`] /
//!   [`Self::note_import_ok`] / [`Self::note_import_invalid`]
//!   ([`Self::import_text`] runs the pure parse and raises the
//!   invalid status itself, matching the React `catch`).
//! - `resolveSnippetDesc` is [`resolve_snippet_desc`] — the identity
//!   dance (`translated === descriptionKey ? descriptionKey :
//!   translated`) is `t()`'s own fallback, kept verbatim.
//!
//! ## Keys
//!
//! The custom rows key `sql-snippets-item-{id}` with the edit
//! `sql-snippets-edit-{id}` and the delete keeping React's
//! `data-testid` verbatim (`delete-snippet-{id}`); the builtin rows
//! key `sql-snippets-builtin-item-{id}` with the duplicate
//! `sql-snippets-duplicate-{id}`. The dialog rides T417's keys.
//!
//! Known deltas: the empty state's `border-dashed` renders solid
//! (Rune has no dash — the established ruling); the template
//! preview's CSS `truncate` is a single-line ellipsis that Rune does
//! not shape (the newline-folded text renders, the clip is the
//! layout's); the rows' `title` tooltips ride the host help system
//! (not ported); a snippet kept by the import filter without a
//! `descriptionKey` carries the empty string (React leaves
//! `undefined` — the typed field's nearest spelling); the exported
//! `customKeymap` object's key order is the JSON writer's (React
//! preserves the source order — cosmetic).

// No production consumer yet (the settings content shell T390 mounts
// it through its `sql_snippets_card` slot); this crate's tests are
// the callers meanwhile.
#![allow(dead_code)]

use std::rc::Rc;

use rune::{col, icon, row, text, El, TextEdit, TextStyle};
use rune_kit::{Button, ButtonVariant, ControlSize, Kit};
use serde_json::{json, Map, Value};

use crate::components::sql_editor::snippets::builtin::builtin_sql_snippets;
use crate::components::sql_editor::snippets::types::SqlSnippetItem;
use crate::hooks::confirm_dialog::{ConfirmDialogHost, ConfirmDialogState, ConfirmOptionsVm};
use crate::i18n::t;
use crate::icons::{
    ICON_CODE, ICON_COPY, ICON_DOWNLOAD, ICON_PENCIL, ICON_PLUS, ICON_TRASH, ICON_UPLOAD,
};
use crate::ui::badge::{Badge, BadgeTone};
use crate::ui::confirm_dialog::ConfirmKind;
use crate::views::settings::snippet_edit_dialog::{
    SnippetEditDialog, SnippetEditDialogState, SnippetSaveError,
};

/// `text-xs` — the status line and the descriptions.
const TEXT_XS: TextStyle = TextStyle::new(12.0, 400.0);
/// `text-xs font-medium` — a snippet's description.
const TEXT_XS_MEDIUM: TextStyle = TextStyle::new(12.0, 500.0);
/// `text-xs font-semibold uppercase tracking-wider` — the card title.
const TEXT_XS_SEMIBOLD: TextStyle = TextStyle::new(12.0, 600.0);
/// `text-[11px] font-medium uppercase tracking-wider` — the group
/// headers.
const TEXT_11_MEDIUM: TextStyle = TextStyle::new(11.0, 500.0);

/// `resolveSnippetDesc`: the translated description, or the key
/// itself when the table lacks it (`t()`'s own fallback — the React
/// ternary spells the identity out; the port keeps it).
pub(crate) fn resolve_snippet_desc(description_key: &str) -> &str {
    let translated = t(description_key);
    if translated == description_key {
        description_key
    } else {
        translated
    }
}

/// The `AppSettings` fields the card reads: the custom snippets plus
/// the whitelist fields the export copies (the `settings` prop —
/// the host syncs it from its store; every field is optional exactly
/// as the React `?` fields).
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct EditorSettingsExportInput {
    pub(crate) editor_font_size: Option<f64>,
    pub(crate) editor_font_family: Option<String>,
    /// `'unquoted' | 'always' | 'both'`.
    pub(crate) completion_quote_policy: Option<String>,
    pub(crate) completion_include_table_prefix: Option<bool>,
    /// `'default' | 'dbeaver' | 'navicat'`.
    pub(crate) keymap_preset: Option<String>,
    pub(crate) custom_keymap: Vec<(String, String)>,
    /// The format options ride the wire opaque (the value's own
    /// schema is the format engine's).
    pub(crate) sql_format_options: Option<Value>,
    /// `'current_statement' | 'entire_script' | 'largest_statement'
    /// | 'ask'`.
    pub(crate) sql_execution_strategy: Option<String>,
    /// `settings.sqlSnippets ?? []`.
    pub(crate) sql_snippets: Vec<SqlSnippetItem>,
}

/// `exportEditorSettings`: the safe whitelist export — the editor
/// block (only the fields the input carries, each absent when
/// unset), the snippets, `version: 1` and the host-stamped
/// `exportedAt`, pretty-printed the way `JSON.stringify(…, null, 2)`
/// prints. No credential, connection or driver field can appear:
/// the builder copies only the keys below.
pub(crate) fn export_editor_settings(input: &EditorSettingsExportInput, exported_at: &str) -> String {
    let mut editor = Map::new();
    if let Some(size) = input.editor_font_size {
        editor.insert("fontSize".to_owned(), json!(size));
    }
    if let Some(family) = &input.editor_font_family {
        editor.insert("fontFamily".to_owned(), json!(family));
    }
    if let Some(include) = input.completion_include_table_prefix {
        editor.insert("completionIncludeTablePrefix".to_owned(), json!(include));
    }
    if let Some(policy) = &input.completion_quote_policy {
        editor.insert("completionQuotePolicy".to_owned(), json!(policy));
    }
    if let Some(preset) = &input.keymap_preset {
        editor.insert("keymapPreset".to_owned(), json!(preset));
    }
    if !input.custom_keymap.is_empty() {
        // `Partial<Record<string, string>>` — a later pair wins the
        // key, the way the object literal's duplicate key would.
        let mut keymap = Map::new();
        for (action, shortcut) in &input.custom_keymap {
            keymap.insert(action.clone(), json!(shortcut));
        }
        editor.insert("customKeymap".to_owned(), Value::Object(keymap));
    }
    if let Some(options) = &input.sql_format_options {
        editor.insert("sqlFormatOptions".to_owned(), options.clone());
    }
    if let Some(strategy) = &input.sql_execution_strategy {
        editor.insert("sqlExecutionStrategy".to_owned(), json!(strategy));
    }

    let payload = json!({
        "version": 1,
        "exportedAt": exported_at,
        "editor": Value::Object(editor),
        "sqlSnippets": input.sql_snippets,
    });
    serde_json::to_string_pretty(&payload).unwrap_or_default()
}

/// `importEditorSettings`' failure sentences (React's `throw new
/// Error(…)` messages; the card's caller surfaces the generic
/// invalid status for every one of them).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ImportEditorSettingsError {
    /// 'Invalid JSON format'.
    InvalidJson,
    /// 'Root must be an object'.
    RootNotObject,
}

/// The `Partial<AppSettings>` the import maps onto: only the safe
/// fields the JSON actually carried (absent stays `None` — "leaves
/// the current preference unchanged").
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct ImportedEditorSettings {
    pub(crate) editor_font_size: Option<f64>,
    pub(crate) editor_font_family: Option<String>,
    pub(crate) completion_quote_policy: Option<String>,
    pub(crate) completion_include_table_prefix: Option<bool>,
    pub(crate) keymap_preset: Option<String>,
    /// The object rides opaque (React casts without checking the
    /// values; the host's coercion is its own).
    pub(crate) custom_keymap: Option<Map<String, Value>>,
    pub(crate) sql_format_options: Option<Value>,
    pub(crate) sql_execution_strategy: Option<String>,
    /// `None` when the JSON carried no `sqlSnippets` array.
    pub(crate) sql_snippets: Option<Vec<SqlSnippetItem>>,
}

/// `importEditorSettings`: validate and map only the safe
/// editor/snippet fields. Every field check is the React one: a
/// wrong-typed or out-of-vocabulary value is skipped (not an error);
/// only a broken parse or a non-object root refuses the whole read.
pub(crate) fn import_editor_settings(text: &str) -> Result<ImportedEditorSettings, ImportEditorSettingsError> {
    let parsed: Value = match serde_json::from_str(text) {
        Ok(parsed) => parsed,
        Err(_) => return Err(ImportEditorSettingsError::InvalidJson),
    };
    let Some(obj) = parsed.as_object() else {
        return Err(ImportEditorSettingsError::RootNotObject);
    };

    let mut result = ImportedEditorSettings::default();

    if let Some(ed) = obj.get("editor").filter(|ed| ed.is_object()).and_then(Value::as_object) {
        if let Some(include) = ed.get("completionIncludeTablePrefix").and_then(Value::as_bool) {
            result.completion_include_table_prefix = Some(include);
        }
        if let Some(size) = ed.get("fontSize").and_then(Value::as_f64) {
            result.editor_font_size = Some(size);
        }
        if let Some(family) = ed.get("fontFamily").and_then(Value::as_str) {
            result.editor_font_family = Some(family.to_owned());
        }
        if let Some(policy) = ed
            .get("completionQuotePolicy")
            .and_then(Value::as_str)
            .filter(|p| *p == "unquoted" || *p == "always" || *p == "both")
        {
            result.completion_quote_policy = Some(policy.to_owned());
        }
        if let Some(preset) = ed
            .get("keymapPreset")
            .and_then(Value::as_str)
            .filter(|p| *p == "default" || *p == "dbeaver" || *p == "navicat")
        {
            result.keymap_preset = Some(preset.to_owned());
        }
        if let Some(keymap) = ed.get("customKeymap").filter(|m| m.is_object()).and_then(Value::as_object) {
            result.custom_keymap = Some(keymap.clone());
        }
        if let Some(options) = ed.get("sqlFormatOptions").filter(|o| o.is_object()) {
            result.sql_format_options = Some(options.clone());
        }
        if let Some(strategy) = ed
            .get("sqlExecutionStrategy")
            .and_then(Value::as_str)
            .filter(|s| {
                *s == "current_statement"
                    || *s == "entire_script"
                    || *s == "largest_statement"
                    || *s == "ask"
            })
        {
            result.sql_execution_strategy = Some(strategy.to_owned());
        }
    }

    if let Some(snippets) = obj.get("sqlSnippets").and_then(Value::as_array) {
        // The React filter checks id/prefix/template — a missing
        // `descriptionKey` still keeps the row (it lands as the
        // empty string here; React leaves `undefined`).
        result.sql_snippets = Some(
            snippets
                .iter()
                .filter_map(|item| {
                    let obj = item.as_object()?;
                    let id = obj.get("id").and_then(Value::as_str)?;
                    let prefix = obj.get("prefix").and_then(Value::as_str)?;
                    let template = obj.get("template").and_then(Value::as_str)?;
                    Some(SqlSnippetItem {
                        id: id.to_owned(),
                        prefix: prefix.to_owned(),
                        description_key: obj
                            .get("descriptionKey")
                            .and_then(Value::as_str)
                            .unwrap_or_default()
                            .to_owned(),
                        template: template.to_owned(),
                    })
                })
                .collect(),
        );
    }

    Ok(result)
}

/// The card's state: the settings mirror, the dialog triple + the
/// T417 draft, the status line with its timer, and the delete
/// confirm.
#[derive(Clone)]
pub(crate) struct SqlSnippetsCardState {
    /// The `settings` prop's read side (the custom snippets + the
    /// export whitelist).
    pub(crate) settings: EditorSettingsExportInput,
    /// The T417 dialog's draft (its `open` is the triple's first
    /// field).
    pub(crate) dialog: SnippetEditDialogState,
    /// The snippet being edited or duplicated (`None` = an add).
    pub(crate) dialog_snippet: Option<SqlSnippetItem>,
    /// `isDuplicate` — the duplicate title and the fresh-id save.
    pub(crate) dialog_is_duplicate: bool,
    /// The status line (`None` = nothing to say).
    pub(crate) status_message: Option<String>,
    /// The live status timer's id (`None` = none armed).
    pub(crate) status_timer: Option<u32>,
    status_timer_next: u32,
    /// The `useConfirmDialog` state (the card is the hook's host).
    pub(crate) confirm: ConfirmDialogState,
    /// The id the pending delete confirm targets.
    pending_delete: Option<String>,
}

impl std::fmt::Debug for SqlSnippetsCardState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SqlSnippetsCardState")
            .field("settings", &self.settings)
            .field("dialog", &self.dialog)
            .field("dialog_snippet", &self.dialog_snippet)
            .field("dialog_is_duplicate", &self.dialog_is_duplicate)
            .field("status_message", &self.status_message)
            .field("status_timer", &self.status_timer)
            .field("confirm_open", &self.confirm.is_open())
            .field("pending_delete", &self.pending_delete)
            .finish()
    }
}

impl SqlSnippetsCardState {
    /// The mount: the settings prop's first read
    /// (`sqlSnippets ?? []` rides the input).
    pub(crate) fn new(settings: &EditorSettingsExportInput) -> Self {
        Self {
            settings: settings.clone(),
            dialog: SnippetEditDialogState::new(),
            dialog_snippet: None,
            dialog_is_duplicate: false,
            status_message: None,
            status_timer: None,
            status_timer_next: 0,
            confirm: ConfirmDialogState::default(),
            pending_delete: None,
        }
    }

    /// The settings prop's next read (the store's change; the
    /// dialog state is independent of it).
    pub(crate) fn resync(&mut self, settings: &EditorSettingsExportInput) {
        self.settings = settings.clone();
    }

    /// `customSnippets` — `settings.sqlSnippets ?? []`.
    pub(crate) fn custom_snippets(&self) -> &[SqlSnippetItem] {
        &self.settings.sql_snippets
    }

    /// The Add button: `setDialogState({ open: true, snippet: null,
    /// isDuplicate: false })` + the seed.
    pub(crate) fn begin_add(&mut self) {
        self.dialog_snippet = None;
        self.dialog_is_duplicate = false;
        self.dialog.seed(true, None, false);
    }

    /// One custom row's edit: `snippet: item, isDuplicate: false`.
    pub(crate) fn begin_edit(&mut self, item: &SqlSnippetItem) {
        self.dialog_snippet = Some(item.clone());
        self.dialog_is_duplicate = false;
        self.dialog.seed(true, Some(item), false);
    }

    /// One builtin row's duplicate: `snippet: item,
    /// isDuplicate: true`.
    pub(crate) fn begin_duplicate(&mut self, item: &SqlSnippetItem) {
        self.dialog_snippet = Some(item.clone());
        self.dialog_is_duplicate = true;
        self.dialog.seed(true, Some(item), true);
    }

    /// The dialog's `onClose`: the triple resets closed.
    pub(crate) fn close_dialog(&mut self) {
        self.dialog.seed(false, None, false);
        self.dialog_snippet = None;
        self.dialog_is_duplicate = false;
    }

    /// `existingPrefixes`: every builtin and custom prefix, minus
    /// the edited snippet's own (a duplicate keeps the source's
    /// prefix in the set — hence the `_copy` seed).
    pub(crate) fn existing_prefixes(&self) -> Vec<String> {
        let mut all: Vec<String> = builtin_sql_snippets()
            .iter()
            .map(|s| s.prefix.clone())
            .chain(self.custom_snippets().iter().map(|s| s.prefix.clone()))
            .collect();
        if !self.dialog_is_duplicate {
            if let Some(own) = &self.dialog_snippet {
                let own = own.prefix.clone();
                all.retain(|p| *p != own);
            }
        }
        all
    }

    /// `handleSave`'s validation gate alone (the host mints the
    /// fresh id only past it — React's order: `crypto.randomUUID`
    /// runs after the checks).
    pub(crate) fn validate_snippet(&mut self) -> Result<(), SnippetSaveError> {
        let edited_id: Option<&str> = if self.dialog_is_duplicate {
            None
        } else {
            self.dialog_snippet.as_ref().map(|s| s.id.as_str())
        };
        let existing = self.existing_prefixes();
        self.dialog.validate(edited_id, &existing)
    }

    /// `handleSaveSnippet` + the dialog's save tail: validate
    /// through the draft (an edit keeps its id; a duplicate takes
    /// the fresh one), replace-by-id-or-append, close, and answer
    /// the updated list for the host's `onUpdateSnippets`. A
    /// validation failure keeps the dialog open with its error
    /// (the host gates its mint through [`Self::validate_snippet`]
    /// first — a refused save never consumes one).
    pub(crate) fn save_snippet(
        &mut self,
        fresh_id: String,
    ) -> Result<Vec<SqlSnippetItem>, SnippetSaveError> {
        let edited_id: Option<&str> = if self.dialog_is_duplicate {
            None
        } else {
            self.dialog_snippet.as_ref().map(|s| s.id.as_str())
        };
        let existing = self.existing_prefixes();
        let item = self.dialog.save(edited_id, || fresh_id, &existing)?;
        match self
            .settings
            .sql_snippets
            .iter_mut()
            .find(|s| s.id == item.id)
        {
            Some(slot) => *slot = item,
            None => self.settings.sql_snippets.push(item),
        }
        // React: `onSave(item); onClose();`
        self.close_dialog();
        Ok(self.settings.sql_snippets.clone())
    }

    /// `handleDeleteSnippet`'s confirm half: arm the warning confirm
    /// with the `{prefix}` message and remember the target.
    pub(crate) fn request_delete(&mut self, item: &SqlSnippetItem) -> u64 {
        let message = t("query.snippets.deleteConfirmMessage").replace("{prefix}", &item.prefix);
        self.pending_delete = Some(item.id.clone());
        self.confirm.open(ConfirmOptionsVm {
            title: t("query.snippets.deleteConfirmTitle").to_owned(),
            message,
            kind: ConfirmKind::Warning,
            ..ConfirmOptionsVm::default()
        })
    }

    /// The id the pending delete confirm targets (the audit's
    /// observation; the host never reads it).
    pub(crate) fn pending_delete_id(&self) -> Option<&str> {
        self.pending_delete.as_deref()
    }

    /// The confirm's resolution: OK filters the target out and
    /// answers the updated list for `onUpdateSnippets`; every other
    /// answer (or a stale request) keeps it.
    pub(crate) fn resolve_confirm(&mut self, result: bool) -> Option<Vec<SqlSnippetItem>> {
        let ok = matches!(self.confirm.answer_pending(result), Some(true));
        let target = self.pending_delete.take();
        if !ok {
            return None;
        }
        let Some(id) = target else { return None };
        self.settings.sql_snippets.retain(|s| s.id != id);
        Some(self.settings.sql_snippets.clone())
    }

    /// `showStatus`: stamp the message and hand out the timer id
    /// (the host arms the 3s timeout; a newer status supersedes the
    /// previous timer the way `clearTimeout` does).
    pub(crate) fn show_status(&mut self, message: &str) -> u32 {
        self.status_timer_next = self.status_timer_next.saturating_add(1);
        let id = self.status_timer_next;
        self.status_message = Some(message.to_owned());
        self.status_timer = Some(id);
        id
    }

    /// The 3s timeout's firing: only the live timer clears (a
    /// superseded one lands here and is discarded).
    pub(crate) fn status_fired(&mut self, timer: u32) -> bool {
        if self.status_timer == Some(timer) {
            self.status_message = None;
            self.status_timer = None;
            true
        } else {
            false
        }
    }

    /// `exportEditorSettings(settings)` — the pure build; the
    /// host supplies the `exportedAt` stamp (React's
    /// `new Date().toISOString()`).
    pub(crate) fn export_json(&self, exported_at: &str) -> String {
        export_editor_settings(&self.settings, exported_at)
    }

    /// `handleImport`'s parse half: the pure validation, with the
    /// invalid status already raised on failure (the React `catch`
    /// arm). `Some(imported)` hands the host the settings to apply
    /// (its `updateSettings`); the success status comes after that
    /// write, through [`Self::note_import_ok`] (a rejected write
    /// lands in [`Self::note_import_invalid`], the same `catch`).
    pub(crate) fn import_text(&mut self, text: &str) -> Option<ImportedEditorSettings> {
        match import_editor_settings(text) {
            Ok(imported) => Some(imported),
            Err(_) => {
                self.show_status(t("query.importSettingsInvalid"));
                None
            }
        }
    }

    /// `showStatus(t('query.exportSettingsSuccess'))`.
    pub(crate) fn note_export_ok(&mut self) -> u32 {
        self.show_status(t("query.exportSettingsSuccess"))
    }

    /// `showStatus(String(e))` — the clipboard write's failure.
    pub(crate) fn note_export_error(&mut self, message: &str) -> u32 {
        self.show_status(message)
    }

    /// `showStatus(t('query.importSettingsSuccess'))` — after the
    /// store write accepted the import.
    pub(crate) fn note_import_ok(&mut self) -> u32 {
        self.show_status(t("query.importSettingsSuccess"))
    }

    /// `showStatus(t('query.importSettingsInvalid'))` — the parse,
    /// clipboard or store-write failure.
    pub(crate) fn note_import_invalid(&mut self) -> u32 {
        self.show_status(t("query.importSettingsInvalid"))
    }
}

/// `SqlSnippetsCard({ settings, onUpdateSnippets })`: the card over
/// the state, the card's state lens and the host's action slots.
pub(crate) struct SqlSnippetsCard<'a, S: 'static> {
    state: &'a SqlSnippetsCardState,
    dark: bool,
    /// The lens into the owning state (the draft edits, the dialog
    /// triple and the confirm all mutate through it).
    card_edit: Rc<dyn for<'b> Fn(&'b mut S) -> &'b mut SqlSnippetsCardState>,
    /// `onUpdateSnippets` — persist the next custom list.
    on_update_snippets: Option<Rc<dyn Fn(&mut S, Vec<SqlSnippetItem>)>>,
    /// `handleExport`'s host half: build the JSON through the state,
    /// write the clipboard, report back.
    on_export: Option<Rc<dyn Fn(&mut S)>>,
    /// `handleImport`'s host half: read the clipboard, parse through
    /// the state, apply, report back.
    on_import: Option<Rc<dyn Fn(&mut S)>>,
    /// The `crypto.randomUUID` seam — the fresh id an add/duplicate
    /// saves under.
    fresh_id: Option<Rc<dyn Fn(&mut S) -> String>>,
}

impl<'a, S: 'static> SqlSnippetsCard<'a, S> {
    pub(crate) fn new(
        state: &'a SqlSnippetsCardState,
        dark: bool,
        card_edit: impl for<'b> Fn(&'b mut S) -> &'b mut SqlSnippetsCardState + 'static,
    ) -> Self {
        Self {
            state,
            dark,
            card_edit: Rc::new(card_edit),
            on_update_snippets: None,
            on_export: None,
            on_import: None,
            fresh_id: None,
        }
    }

    /// The `onUpdateSnippets` prop: persist the next custom list
    /// (the save and the confirmed delete both fire it).
    pub(crate) fn on_update_snippets(
        mut self,
        f: impl Fn(&mut S, Vec<SqlSnippetItem>) + 'static,
    ) -> Self {
        self.on_update_snippets = Some(Rc::new(f));
        self
    }

    /// The export button's clipboard half.
    pub(crate) fn on_export(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_export = Some(Rc::new(f));
        self
    }

    /// The import button's clipboard half.
    pub(crate) fn on_import(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_import = Some(Rc::new(f));
        self
    }

    /// The `crypto.randomUUID` seam (the fresh id an add/duplicate
    /// takes).
    pub(crate) fn fresh_id(mut self, f: impl Fn(&mut S) -> String + 'static) -> Self {
        self.fresh_id = Some(Rc::new(f));
        self
    }

    /// One snippet row (the custom and builtin faces share the
    /// shape; `badge` differs and the action pair is the caller's).
    fn snippet_row(
        &self,
        k: &Kit,
        item: &SqlSnippetItem,
        row_key: String,
        badge: (String, BadgeTone, &'static str),
        actions: El,
    ) -> El {
        let mut left = row()
            .key(format!("{row_key}-lead"))
            .gap(10.0)
            .center_y()
            .grow(1.0)
            .min_w(0.0);
        // The `kbd` prefix chip: mono, semibold, the accent face on
        // custom rows, the plain face on builtin rows.
        let prefix_color = if badge.1 == BadgeTone::Accent {
            k.c.text_accent
        } else {
            k.c.text_base
        };
        left = left.child(
            row()
                .key(format!("{row_key}-prefix"))
                .min_w(56.0) // min-w-14
                .px(6.0)
                .py(2.0)
                .radius(4.0)
                .fill(k.c.surface_raised) // bg-surface-raised
                .border(1.0, k.c.border_muted)
                .center()
                .child(
                    text(item.prefix.clone())
                        .font(TextStyle::new(11.0, 600.0).mono())
                        .color(prefix_color),
                ),
        );
        let mut meta = col().key(format!("{row_key}-meta")).gap(2.0).grow(1.0).min_w(0.0);
        let mut title = row().key(format!("{row_key}-title")).gap(6.0).center_y();
        title = title.child(
            text(resolve_snippet_desc(&item.description_key).to_owned())
                .font(TEXT_XS_MEDIUM)
                .color(k.c.text_base),
        );
        title = title.child(
            Badge::new(badge.0, t(badge.2).to_owned())
                .tone(badge.1)
                .pad(6.0, 0.0)
                .font(TextStyle::new(10.0, 400.0))
                .build(self.dark),
        );
        meta = meta.child(title);
        // The template preview: newlines fold to spaces.
        meta = meta.child(
            text(item.template.replace('\n', " "))
                .key(format!("{row_key}-template"))
                .font(TextStyle::new(11.0, 400.0).mono())
                .color(k.c.text_muted),
        );
        left = left.child(meta);

        row()
            .key(row_key)
            .w_full()
            .gap(12.0)
            .center_y()
            .justify_between()
            .padding(10.0) // p-2.5
            .child(left)
            .child(actions)
    }

    /// The card (`space-y-4 rounded-lg border bg-surface-alt`).
    pub(crate) fn build(self, k: &Kit, layer: &mut Vec<El>) -> El {
        let state = self.state;
        let mut card = col()
            .key("sql-snippets-card")
            .w_full()
            .gap(16.0) // space-y-4
            .padding(14.0) // p-3.5
            .radius(8.0) // rounded-lg
            .border(1.0, k.c.border_base) // border border-edge
            .fill(k.c.surface_weak); // bg-surface-alt

        // The header: the Code2 title + the add/export/import row.
        let mut head = row()
            .key("sql-snippets-head")
            .w_full()
            .gap(8.0)
            .center_y()
            .justify_between();
        head = head.child(
            row()
                .key("sql-snippets-title")
                .gap(8.0)
                .center_y()
                .child(icon(&ICON_CODE, 16.0).color(k.c.text_accent))
                .child(
                    text(t("query.snippets").to_owned())
                        .font(TEXT_XS_SEMIBOLD)
                        .color(k.c.text_base),
                ),
        );

        let mut buttons = row().key("sql-snippets-actions").gap(6.0).center_y();
        // Add (primary → Submit).
        let card_edit = Rc::clone(&self.card_edit);
        let mut add = Button::new(t("query.snippets.add"))
            .icon(&ICON_PLUS)
            .variant(ButtonVariant::Submit) // React primary
            .control_size(ControlSize::Small)
            .on_press(move |s: &mut S| card_edit(s).begin_add());
        buttons = buttons.child(add.build(k).key("sql-snippets-add"));
        // Export (secondary → Outline).
        if let Some(on_export) = &self.on_export {
            let on_export = Rc::clone(on_export);
            let mut export = Button::new(t("query.exportSettings"))
                .icon(&ICON_DOWNLOAD)
                .variant(ButtonVariant::Outline) // React secondary
                .control_size(ControlSize::Small)
                .help(t("query.exportSettings"))
                .on_press(move |s: &mut S| on_export(s));
            buttons = buttons.child(export.build(k).key("sql-snippets-export"));
        }
        // Import (secondary → Outline).
        if let Some(on_import) = &self.on_import {
            let on_import = Rc::clone(on_import);
            let mut import = Button::new(t("query.importSettings"))
                .icon(&ICON_UPLOAD)
                .variant(ButtonVariant::Outline) // React secondary
                .control_size(ControlSize::Small)
                .help(t("query.importSettings"))
                .on_press(move |s: &mut S| on_import(s));
            buttons = buttons.child(import.build(k).key("sql-snippets-import"));
        }
        head = head.child(buttons);
        card = card.child(head);

        // The status line.
        if let Some(message) = &state.status_message {
            card = card.child(
                text(message.clone())
                    .key("sql-snippets-status")
                    .font(TEXT_XS)
                    .color(k.c.text_accent),
            );
        }

        // The custom snippets block.
        let custom = state.custom_snippets();
        let mut custom_block = col().key("sql-snippets-custom").w_full().gap(8.0);
        custom_block = custom_block.child(
            text(format!("{} ({})", t("query.snippets.custom"), custom.len()))
                .key("sql-snippets-custom-count")
                .font(TEXT_11_MEDIUM)
                .color(k.c.text_muted),
        );
        if custom.is_empty() {
            // The empty state (React's dashed border renders solid).
            custom_block = custom_block.child(
                col()
                    .key("sql-snippets-custom-empty")
                    .w_full()
                    .padding(16.0)
                    .radius(6.0)
                    .border(1.0, k.c.border_muted)
                    .center()
                    .child(
                        text(t("query.snippets.emptyCustom").to_owned())
                            .font(TEXT_XS)
                            .color(k.c.text_muted),
                    ),
            );
        } else {
            let mut list = col()
                .key("sql-snippets-custom-list")
                .w_full()
                .radius(6.0)
                .border(1.0, k.c.border_base)
                .fill(k.c.surface_weak); // bg-surface
            for (index, item) in custom.iter().enumerate() {
                let row_key = format!("sql-snippets-item-{}", item.id);
                // The edit + delete pair.
                let mut actions = row().gap(4.0).center_y().shrink();
                let card_edit = Rc::clone(&self.card_edit);
                let item_for_edit = item.clone();
                let mut edit = Button::new("")
                    .icon(&ICON_PENCIL)
                    .variant(ButtonVariant::Ghost)
                    .control_size(ControlSize::Small)
                    .help(t("query.snippets.edit"))
                    .on_press(move |s: &mut S| {
                        card_edit(s).begin_edit(&item_for_edit);
                    });
                actions = actions
                    .child(edit.build(k).key(format!("sql-snippets-edit-{}", item.id)));
                let card_edit = Rc::clone(&self.card_edit);
                let item_for_delete = item.clone();
                let mut delete = Button::new("")
                    .icon(&ICON_TRASH)
                    .variant(ButtonVariant::Ghost)
                    .control_size(ControlSize::Small)
                    .help(t("common.delete"))
                    .on_press(move |s: &mut S| {
                        card_edit(s).request_delete(&item_for_delete);
                    });
                // React's `data-testid` verbatim.
                actions = actions
                    .child(delete.build(k).key(format!("delete-snippet-{}", item.id)));

                let mut row_el = self.snippet_row(
                    k,
                    item,
                    row_key,
                    (
                        format!("sql-snippets-badge-{}", item.id),
                        BadgeTone::Accent,
                        "query.snippets.custom",
                    ),
                    actions,
                );
                if index > 0 {
                    // `divide-y divide-edge`.
                    row_el = row_el.border_sides([1.0, 0.0, 0.0, 0.0], k.c.border_base);
                }
                list = list.child(row_el);
            }
            custom_block = custom_block.child(list);
        }
        card = card.child(custom_block);

        // The builtin snippets block.
        let builtins = builtin_sql_snippets();
        let mut builtin_block = col().key("sql-snippets-builtin").w_full().gap(8.0).pt(8.0);
        builtin_block = builtin_block.child(
            text(format!("{} ({})", t("query.snippets.builtin"), builtins.len()))
                .key("sql-snippets-builtin-count")
                .font(TEXT_11_MEDIUM)
                .color(k.c.text_muted),
        );
        let mut builtin_list = col()
            .key("sql-snippets-builtin-list")
            .w_full()
            .radius(6.0)
            .border(1.0, k.c.border_base)
            .fill(k.c.surface_weak); // bg-surface
        for (index, item) in builtins.iter().enumerate() {
            let row_key = format!("sql-snippets-builtin-item-{}", item.id);
            // The duplicate button (ghost, icon + label).
            let card_edit = Rc::clone(&self.card_edit);
            let item_for_duplicate = item.clone();
            let mut duplicate = Button::new(t("query.snippets.duplicate"))
                .icon(&ICON_COPY)
                .variant(ButtonVariant::Ghost)
                .control_size(ControlSize::Small)
                .help(t("query.snippets.duplicate"))
                .on_press(move |s: &mut S| {
                    card_edit(s).begin_duplicate(&item_for_duplicate);
                });
            let actions = row()
                .gap(4.0)
                .center_y()
                .shrink()
                .child(duplicate.build(k).key(format!("sql-snippets-duplicate-{}", item.id)));

            let mut row_el = self.snippet_row(
                k,
                item,
                row_key,
                (
                    format!("sql-snippets-builtin-badge-{}", item.id),
                    BadgeTone::Neutral,
                    "query.snippets.builtin",
                ),
                actions,
            );
            if index > 0 {
                row_el = row_el.border_sides([1.0, 0.0, 0.0, 0.0], k.c.border_base);
            }
            builtin_list = builtin_list.child(row_el);
        }
        builtin_block = builtin_block.child(builtin_list);
        card = card.child(builtin_block);

        // The T417 edit dialog: the draft lenses through the card
        // lens; the save runs `handleSaveSnippet` (persist + close).
        let editing = !state.dialog_is_duplicate && state.dialog_snippet.is_some();
        let existing = state.existing_prefixes();
        let card_edit = Rc::clone(&self.card_edit);
        let prefix_lens = Rc::clone(&card_edit);
        let description_lens = Rc::clone(&card_edit);
        let template_lens = Rc::clone(&card_edit);
        let mut dialog = SnippetEditDialog::<S>::new(
            &state.dialog,
            self.dark,
            state.dialog_is_duplicate,
            editing,
            &existing,
            move |s: &mut S| &mut prefix_lens(s).dialog.prefix,
            move |s: &mut S| &mut description_lens(s).dialog.description,
            move |s: &mut S| &mut template_lens(s).dialog.template,
        )
        .on_close(move |s: &mut S| card_edit(s).close_dialog());
        let card_edit = Rc::clone(&self.card_edit);
        let on_update_snippets = self.on_update_snippets.clone();
        let fresh_id = self.fresh_id.clone();
        dialog = dialog.on_save_pressed(move |s: &mut S| {
            // The gate first — the fresh id mints only past it
            // (React's `crypto.randomUUID` runs after the checks).
            let valid = {
                let mut card = card_edit(s);
                card.validate_snippet().is_ok()
            };
            if !valid {
                return;
            }
            let fresh = fresh_id.as_ref().map(|f| f(s)).unwrap_or_default();
            let mut card = card_edit(s);
            if let Ok(updated) = card.save_snippet(fresh) {
                if let Some(on_update_snippets) = &on_update_snippets {
                    on_update_snippets(s, updated);
                }
            }
        });
        dialog.build(k, layer);

        // The delete confirm (the hook's host).
        let card_edit = Rc::clone(&self.card_edit);
        let on_update_snippets = self.on_update_snippets.clone();
        let confirm_host = ConfirmDialogHost::<S>::new(&state.confirm).on_resolved(
            move |s: &mut S, result: bool| {
                let mut card = card_edit(s);
                if let Some(updated) = card.resolve_confirm(result) {
                    if let Some(on_update_snippets) = &on_update_snippets {
                        on_update_snippets(s, updated);
                    }
                }
            },
        );
        confirm_host.build(self.dark, layer);

        card
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snippet(id: &str, prefix: &str) -> SqlSnippetItem {
        SqlSnippetItem {
            id: id.to_owned(),
            prefix: prefix.to_owned(),
            description_key: format!("{prefix} description"),
            template: format!("SELECT {prefix};"),
        }
    }

    fn settings_input() -> EditorSettingsExportInput {
        EditorSettingsExportInput {
            editor_font_size: Some(14.0),
            editor_font_family: Some("Fira Code".to_owned()),
            completion_quote_policy: Some("always".to_owned()),
            completion_include_table_prefix: None,
            keymap_preset: Some("dbeaver".to_owned()),
            custom_keymap: vec![("editor.execute".to_owned(), "ctrl-enter".to_owned())],
            sql_format_options: Some(json!({
                "keywordCase": "upper",
                "linesBetweenQueries": 2,
            })),
            sql_execution_strategy: Some("entire_script".to_owned()),
            sql_snippets: vec![snippet("user-s1", "selc")],
        }
    }

    /// The React `exports only whitelisted safe editor and snippet
    /// properties` + `imports valid exported json correctly` pair.
    #[test]
    fn the_export_whitelists_and_the_import_round_trips() {
        let json = export_editor_settings(&settings_input(), "2026-01-01T00:00:00.000Z");
        let parsed: Value = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed["version"], 1);
        assert_eq!(parsed["exportedAt"], "2026-01-01T00:00:00.000Z");
        assert_eq!(parsed["editor"]["fontSize"].as_f64(), Some(14.0));
        assert_eq!(parsed["editor"]["fontFamily"], "Fira Code");
        assert_eq!(parsed["editor"]["keymapPreset"], "dbeaver");
        assert_eq!(parsed["editor"]["customKeymap"]["editor.execute"], "ctrl-enter");
        assert_eq!(parsed["editor"]["sqlExecutionStrategy"], "entire_script");
        assert_eq!(parsed["sqlSnippets"].as_array().unwrap().len(), 1);
        assert_eq!(parsed["sqlSnippets"][0]["prefix"], "selc");
        // An unset field stays absent (not null).
        assert!(parsed["editor"].get("completionIncludeTablePrefix").is_none());
        // Sensitive settings can never appear: the builder copies
        // only the whitelist keys.
        assert!(parsed.get("theme").is_none());
        assert!(parsed.get("mcpAllowedConnectionIds").is_none());
        assert!(parsed.get("driverSettings").is_none());
        assert!(parsed.get("wappSettings").is_none());

        let imported = import_editor_settings(&json).unwrap();
        assert_eq!(imported.editor_font_size, Some(14.0));
        assert_eq!(imported.editor_font_family.as_deref(), Some("Fira Code"));
        assert_eq!(
            imported.sql_execution_strategy.as_deref(),
            Some("entire_script")
        );
        assert_eq!(imported.sql_snippets.as_ref().unwrap().len(), 1);
        assert_eq!(imported.sql_snippets.as_ref().unwrap()[0].prefix, "selc");
        // An absent preference stays absent ("leaves the current
        // preference unchanged when importing older settings").
        assert_eq!(imported.completion_include_table_prefix, None);
    }

    /// The `round-trips the table prefix preference` cases.
    #[test]
    fn the_table_prefix_preference_round_trips_both_ways() {
        for enabled in [true, false] {
            let input = EditorSettingsExportInput {
                completion_include_table_prefix: Some(enabled),
                ..EditorSettingsExportInput::default()
            };
            let json = export_editor_settings(&input, "now");
            let parsed: Value = serde_json::from_str(&json).unwrap();
            assert_eq!(parsed["editor"]["completionIncludeTablePrefix"], enabled);
            let imported = import_editor_settings(&json).unwrap();
            assert_eq!(imported.completion_include_table_prefix, Some(enabled));
        }
    }

    /// The `ignores invalid table prefix preferences` cases: a
    /// wrong-typed value is skipped, not an error.
    #[test]
    fn invalid_table_prefix_values_are_ignored() {
        for value in [json!("false"), json!(0), json!(null), json!({})] {
            let text = json!({ "editor": { "completionIncludeTablePrefix": value } }).to_string();
            let imported = import_editor_settings(&text).unwrap();
            assert_eq!(
                imported.completion_include_table_prefix, None,
                "the {value} payload is skipped"
            );
        }
    }

    /// The `rejects invalid json` cases.
    #[test]
    fn broken_payloads_refuse_the_read() {
        assert_eq!(
            import_editor_settings("invalid json"),
            Err(ImportEditorSettingsError::InvalidJson)
        );
        assert_eq!(
            import_editor_settings("123"),
            Err(ImportEditorSettingsError::RootNotObject)
        );
    }

    /// The out-of-vocabulary enum strings are skipped; the snippet
    /// filter keeps the rows React's filter keeps (a missing
    /// `descriptionKey` rides as the empty string).
    #[test]
    fn the_vocabulary_and_snippet_filters_follow_react() {
        let text = json!({
            "editor": {
                "keymapPreset": "vim",
                "completionQuotePolicy": "always",
                "sqlExecutionStrategy": "yolo",
                "customKeymap": { "a": "Mod-Enter", "b": 5 },
            },
            "sqlSnippets": [
                { "id": "a", "prefix": "pa", "template": "T" },
                { "id": "b", "prefix": 7, "template": "T" },
                { "id": "c", "prefix": "pc" },
                "junk",
                { "id": "d", "prefix": "pd", "template": "TD", "descriptionKey": "desc" },
            ],
        })
        .to_string();
        let imported = import_editor_settings(&text).unwrap();
        assert_eq!(imported.keymap_preset, None, "vim is not a preset");
        assert_eq!(imported.completion_quote_policy.as_deref(), Some("always"));
        assert_eq!(imported.sql_execution_strategy, None, "yolo is not a strategy");
        let keymap = imported.custom_keymap.unwrap();
        assert_eq!(keymap["a"], json!("Mod-Enter"), "string values ride");
        assert_eq!(keymap["b"], json!(5), "non-strings ride opaque");
        let snippets = imported.sql_snippets.unwrap();
        assert_eq!(snippets.len(), 2, "only the id+prefix+template rows keep");
        assert_eq!(snippets[0].description_key, "", "a missing descriptionKey lands empty");
        assert_eq!(snippets[1].description_key, "desc");
    }

    /// `existingPrefixes`: builtin + custom, minus the edited
    /// snippet's own prefix; a duplicate keeps the source's.
    #[test]
    fn the_existing_prefixes_exclude_only_the_edited_own() {
        let mut state = SqlSnippetsCardState::new(&EditorSettingsExportInput {
            sql_snippets: vec![snippet("user-s1", "myown")],
            ..EditorSettingsExportInput::default()
        });
        // Closed: every prefix counts.
        let all = state.existing_prefixes();
        assert!(all.contains(&"sel*".to_owned()), "builtin prefixes count");
        assert!(all.contains(&"myown".to_owned()), "custom prefixes count");
        // Editing `myown`: its own prefix leaves the set.
        state.begin_edit(&snippet("user-s1", "myown"));
        let editing = state.existing_prefixes();
        assert!(!editing.contains(&"myown".to_owned()), "the own prefix leaves");
        assert!(editing.contains(&"sel*".to_owned()));
        // Duplicating `myown`: the source's prefix stays in the set.
        state.begin_duplicate(&snippet("user-s1", "myown"));
        let duplicating = state.existing_prefixes();
        assert!(duplicating.contains(&"myown".to_owned()), "the duplicate's source stays");
    }

    /// `handleSaveSnippet`: an edit replaces by id, an add/duplicate
    /// appends under the fresh id; both close the dialog; a
    /// validation failure keeps it open.
    #[test]
    fn the_save_replaces_or_appends_and_closes() {
        let mut state = SqlSnippetsCardState::new(&EditorSettingsExportInput {
            sql_snippets: vec![snippet("user-s1", "old*")],
            ..EditorSettingsExportInput::default()
        });
        // An edit replaces by id.
        state.begin_edit(&snippet("user-s1", "old*"));
        state.dialog.prefix = TextEdit::new("new*".to_owned());
        state.dialog.template = TextEdit::new("SELECT 2;".to_owned());
        let updated = state.save_snippet("fresh".to_owned()).unwrap();
        assert_eq!(updated.len(), 1);
        assert_eq!(updated[0].id, "user-s1", "the edited id is kept");
        assert_eq!(updated[0].prefix, "new*");
        assert!(!state.dialog.open, "the dialog closes");
        assert!(state.dialog_snippet.is_none());
        // An add appends under the fresh id.
        state.begin_add();
        state.dialog.prefix = TextEdit::new("added*".to_owned());
        state.dialog.template = TextEdit::new("SELECT 3;".to_owned());
        let updated = state.save_snippet("fresh-2".to_owned()).unwrap();
        assert_eq!(updated.len(), 2);
        assert_eq!(updated[1].id, "fresh-2");
        // A duplicate takes the fresh id too.
        state.begin_duplicate(&snippet("user-s1", "new*"));
        state.dialog.prefix = TextEdit::new("newer*".to_owned());
        state.dialog.template = TextEdit::new("SELECT 4;".to_owned());
        let updated = state.save_snippet("fresh-3".to_owned()).unwrap();
        assert_eq!(updated.len(), 3);
        assert_eq!(updated[2].id, "fresh-3");
        // A duplicate prefix is refused: the gate errors first (the
        // host never mints), the dialog stays open.
        state.begin_add();
        state.dialog.prefix = TextEdit::new("NEW*".to_owned());
        state.dialog.template = TextEdit::new("SELECT 5;".to_owned());
        assert_eq!(state.validate_snippet(), Err(SnippetSaveError::PrefixDuplicate));
        assert!(state.dialog.open, "a refused save keeps the dialog");
    }

    /// `handleDeleteSnippet`: OK removes the row; a decline keeps
    /// it; a stale answer never lands.
    #[test]
    fn the_delete_confirm_removes_only_on_ok() {
        let mut state = SqlSnippetsCardState::new(&EditorSettingsExportInput {
            sql_snippets: vec![snippet("user-s1", "a*"), snippet("user-s2", "b*")],
            ..EditorSettingsExportInput::default()
        });
        let id = state.request_delete(&snippet("user-s1", "a*"));
        assert!(state.confirm.is_open());
        // A decline keeps the list.
        assert_eq!(state.resolve_confirm(false), None);
        assert_eq!(state.custom_snippets().len(), 2);
        assert!(state.pending_delete_id().is_none(), "the target clears");
        // OK removes the row.
        let id = state.request_delete(&snippet("user-s2", "b*"));
        let _ = id;
        let updated = state.resolve_confirm(true).unwrap();
        assert_eq!(updated.len(), 1);
        assert_eq!(updated[0].id, "user-s1");
        assert_eq!(state.custom_snippets().len(), 1);
        // No request pending: a late answer never lands.
        assert!(!state.confirm.is_open());
        assert_eq!(state.resolve_confirm(true), None);
        assert_eq!(state.custom_snippets().len(), 1, "the list is untouched");
    }

    /// `showStatus`: a newer status supersedes the old timer; only
    /// the live firing clears.
    #[test]
    fn the_status_line_follows_the_timer_ref() {
        let mut state = SqlSnippetsCardState::new(&EditorSettingsExportInput::default());
        let first = state.note_export_ok();
        assert_eq!(state.status_message, Some(t("query.exportSettingsSuccess").to_owned()));
        // A newer status replaces the message and the timer.
        let second = state.note_import_ok();
        assert_eq!(state.status_message, Some(t("query.importSettingsSuccess").to_owned()));
        // The superseded timer's firing is discarded.
        assert!(!state.status_fired(first));
        assert!(state.status_message.is_some());
        // The live timer clears.
        assert!(state.status_fired(second));
        assert_eq!(state.status_message, None);
        assert_eq!(state.status_timer, None);
    }
}
