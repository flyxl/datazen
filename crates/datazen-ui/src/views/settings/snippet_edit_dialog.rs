//! Port of
//! `packages/frontend/src/windows/settings/SnippetEditDialog.tsx`
//! (T417) — the SQL snippet add/edit/duplicate dialog.
//!
//! ## What maps where
//!
//! - The `useEffect([open, snippet, isDuplicate, t])` seed is
//!   [`SnippetEditDialogState::seed`]: an edit seeds the draft from
//!   the snippet (a duplicate gets the `{prefix}_copy` prefix); an
//!   add clears it; the error resets. The description resolves
//!   through the i18n table (`resolvedDesc === descriptionKey ?
//!   descriptionKey : resolvedDesc` — a missing key keeps the key
//!   itself, the seed a raw imported snippet carries).
//! - `handleSave`'s validation chain is
//!   [`SnippetEditDialogState::save`]: the trimmed prefix must be
//!   non-empty and match `PREFIX_REGEX`
//!   (`^[A-Za-z_][A-Za-z0-9_*]*$`), must not duplicate an existing
//!   prefix case-insensitively, and the trimmed template must be
//!   non-empty — the three errors in that order, each the React
//!   message key. The saved item keeps the edited snippet's id (a
//!   duplicate gets a fresh one — the host supplies it, React's
//!   `crypto.randomUUID`), and the description falls back to the
//!   trimmed prefix when empty.
//! - `if (!open) return null` — the dialog renders nothing closed.
//! - `onSave` + `onClose` are the host slots; the save's `onClose()`
//!   after `onSave(...)` is the host's composition (the audit pins
//!   both firing).
//! - `Dialog` is the ui dialog (title by mode: duplicate / edit /
//!   add); the footer's ghost Cancel + primary Save follow the T022
//!   variant mapping (`primary` → Submit).
//!
//! ## Keys
//!
//! React's `htmlFor` ids stay as the field keys (`snippet-prefix`,
//! `snippet-template`); the description field, the error block and
//! the footer buttons carry structural `snippet-*` keys.
//!
//! Known deltas: the template `<textarea>` renders as the kit's
//! single-line [`TextField`] in the mono face (the value keeps the
//! multi-line form — the same ruling the memory section's edit form
//! records); the required `*` marker rides the label text.

// No production consumer yet (the SQL snippets card T426 mounts it);
// this crate's tests are the callers meanwhile.
#![allow(dead_code)]

use std::rc::Rc;

use rune::{col, row, text, El, TextEdit, TextStyle};
use rune_kit::{Button, ButtonVariant, ControlSize, Kit};

use crate::components::sql_editor::snippets::types::SqlSnippetItem;
use crate::i18n::t;
use crate::ui::dialog::Dialog;
use crate::ui::field::TextField;
use crate::ui::kit::kit;
use crate::views::settings::setting_hint::SettingHint;

/// `text-xs` — the labels and the error.
const TEXT_XS: TextStyle = TextStyle::new(12.0, 400.0);
/// `text-xs font-medium` — the field labels.
const TEXT_XS_MEDIUM: TextStyle = TextStyle::new(12.0, 500.0);

/// `PREFIX_REGEX`: `^[A-Za-z_][A-Za-z0-9_*]*$`.
pub fn prefix_valid(prefix: &str) -> bool {
    let mut chars = prefix.chars();
    match chars.next() {
        Some(first) if first.is_ascii_alphabetic() || first == '_' => {}
        _ => return false,
    }
    chars.all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '*')
}

/// The three validation errors (the React message keys).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SnippetSaveError {
    PrefixInvalid,
    PrefixDuplicate,
    TemplateRequired,
}

impl SnippetSaveError {
    /// The message key the React `setError(t(...))` spells.
    pub fn message_key(self) -> &'static str {
        match self {
            Self::PrefixInvalid => "query.snippets.prefixInvalid",
            Self::PrefixDuplicate => "query.snippets.prefixDuplicate",
            Self::TemplateRequired => "query.snippets.templateRequired",
        }
    }
}

/// The dialog's state: the open flag, the three-field draft and the
/// validation error.
#[derive(Clone, Default, PartialEq)]
pub struct SnippetEditDialogState {
    pub open: bool,
    pub prefix: TextEdit,
    pub description: TextEdit,
    pub template: TextEdit,
    pub error: Option<SnippetSaveError>,
}

impl std::fmt::Debug for SnippetEditDialogState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SnippetEditDialogState")
            .field("open", &self.open)
            .field("prefix", &self.prefix.text)
            .field("description", &self.description.text)
            .field("template", &self.template.text)
            .field("error", &self.error)
            .finish()
    }
}

impl SnippetEditDialogState {
    pub fn new() -> Self {
        Self::default()
    }

    /// The `useEffect` seed: open (re)seeds the draft; the error
    /// always resets.
    pub fn seed(
        &mut self,
        open: bool,
        snippet: Option<&SqlSnippetItem>,
        is_duplicate: bool,
    ) {
        self.open = open;
        if !open {
            return;
        }
        match snippet {
            Some(snippet) => {
                let prefix = if is_duplicate {
                    format!("{}_copy", snippet.prefix)
                } else {
                    snippet.prefix.clone()
                };
                self.prefix = TextEdit::new(prefix);
                // `resolvedDesc === descriptionKey ? descriptionKey :
                // resolvedDesc` — a key that resolves seeds the
                // resolved text; one that does not seeds the key
                // itself (a raw imported snippet).
                let resolved = t(&snippet.description_key);
                self.description = TextEdit::new(resolved.to_owned());
                self.template = TextEdit::new(snippet.template.clone());
            }
            None => {
                self.prefix = TextEdit::default();
                self.description = TextEdit::default();
                self.template = TextEdit::default();
            }
        }
        self.error = None;
    }

    /// The validation chain alone (the same three errors in the
    /// same order; the error lands in the draft either way) — the
    /// host's gate before it mints a fresh id.
    pub fn validate(
        &mut self,
        edited_id: Option<&str>,
        existing_prefixes: &[String],
    ) -> Result<(), SnippetSaveError> {
        let _ = edited_id;
        let trimmed_prefix = self.prefix.text.trim().to_owned();
        let trimmed_template = self.template.text.trim().to_owned();

        if trimmed_prefix.is_empty() || !prefix_valid(&trimmed_prefix) {
            self.error = Some(SnippetSaveError::PrefixInvalid);
            return Err(SnippetSaveError::PrefixInvalid);
        }
        let lower = trimmed_prefix.to_lowercase();
        if existing_prefixes
            .iter()
            .any(|p| p.to_lowercase() == lower)
        {
            self.error = Some(SnippetSaveError::PrefixDuplicate);
            return Err(SnippetSaveError::PrefixDuplicate);
        }
        if trimmed_template.is_empty() {
            self.error = Some(SnippetSaveError::TemplateRequired);
            return Err(SnippetSaveError::TemplateRequired);
        }
        Ok(())
    }

    /// `handleSave`'s validation chain and the item build. The
    /// `edited_id` is the snippet being edited's id (kept); the
    /// `fresh_id` mints the new identity an add/duplicate takes
    /// (React's `crypto.randomUUID`) and only runs once the
    /// validation has passed — a refused save never consumes one.
    pub fn save(
        &mut self,
        edited_id: Option<&str>,
        fresh_id: impl FnOnce() -> String,
        existing_prefixes: &[String],
    ) -> Result<SqlSnippetItem, SnippetSaveError> {
        self.validate(edited_id, existing_prefixes)?;
        let trimmed_prefix = self.prefix.text.trim().to_owned();
        let trimmed_template = self.template.text.trim().to_owned();

        let item = SqlSnippetItem {
            id: edited_id.map(str::to_owned).unwrap_or_else(fresh_id),
            prefix: trimmed_prefix.clone(),
            description_key: {
                let trimmed = self.description.text.trim().to_owned();
                if trimmed.is_empty() {
                    trimmed_prefix
                } else {
                    trimmed
                }
            },
            template: trimmed_template,
        };
        // React never clears the error on success — the dialog closes
        // (unmounts) and the next open's seed resets it; the port
        // keeps the same survival (unobserved while closed).
        Ok(item)
    }

    /// An edit clears the error (the prefix/template onChange arms).
    pub fn clear_error(&mut self) {
        self.error = None;
    }
}

/// `SnippetEditDialog({...})`: the dialog over the state, the
/// existing prefixes and the host's action slots.
pub struct SnippetEditDialog<'a, S: 'static> {
    state: &'a SnippetEditDialogState,
    dark: bool,
    /// `isDuplicate` (the title and the `{prefix}_copy` seed).
    is_duplicate: bool,
    /// Whether a snippet is being edited (the title's third arm).
    editing: bool,
    existing_prefixes: &'a [String],
    // the text lenses
    prefix_edit: Rc<dyn for<'b> Fn(&'b mut S) -> &'b mut TextEdit>,
    description_edit: Rc<dyn for<'b> Fn(&'b mut S) -> &'b mut TextEdit>,
    template_edit: Rc<dyn for<'b> Fn(&'b mut S) -> &'b mut TextEdit>,
    // host actions
    on_close: Option<Rc<dyn Fn(&mut S)>>,
    /// `handleSave` — the host runs the validation
    /// ([`SnippetEditDialogState::save`]) and, on success, fires its
    /// own save + close (the component's `onSave(...); onClose();`
    /// tail).
    on_save_pressed: Option<Rc<dyn Fn(&mut S)>>,
}

impl<'a, S: 'static> SnippetEditDialog<'a, S> {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        state: &'a SnippetEditDialogState,
        dark: bool,
        is_duplicate: bool,
        editing: bool,
        existing_prefixes: &'a [String],
        prefix_edit: impl for<'b> Fn(&'b mut S) -> &'b mut TextEdit + 'static,
        description_edit: impl for<'b> Fn(&'b mut S) -> &'b mut TextEdit + 'static,
        template_edit: impl for<'b> Fn(&'b mut S) -> &'b mut TextEdit + 'static,
    ) -> Self {
        Self {
            state,
            dark,
            is_duplicate,
            editing,
            existing_prefixes,
            prefix_edit: Rc::new(prefix_edit),
            description_edit: Rc::new(description_edit),
            template_edit: Rc::new(template_edit),
            on_close: None,
            on_save_pressed: None,
        }
    }

    pub fn on_close(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_close = Some(Rc::new(f));
        self
    }

    /// The Save button's `handleSave`: the host validates through
    /// the state and, on success, saves and closes (React's
    /// `onSave(item); onClose();` tail — the audit pins both
    /// firing).
    pub fn on_save_pressed(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_save_pressed = Some(Rc::new(f));
        self
    }

    /// The dialog (nothing when closed).
    pub fn build(self, k: &Kit, layer: &mut Vec<El>) {
        if !self.state.open {
            return;
        }
        let title = if self.is_duplicate {
            t("query.snippets.duplicate")
        } else if self.editing {
            t("query.snippets.edit")
        } else {
            t("query.snippets.add")
        };

        let mut dialog = Dialog::new("snippet-edit-dialog", title)
            .open(true)
            .width(576.0); // max-w-xl
        // The dismissal policy (the header close, Escape): a dialog
        // without it mounts nothing — the host's `onClose` is it.
        if let Some(on_close) = &self.on_close {
            let on_close = Rc::clone(on_close);
            dialog = dialog.on_close(move |s: &mut S| on_close(s));
        }

        // The body: the error block + the three fields.
        let mut body = col().key("snippet-edit-body").w_full().gap(16.0);

        if let Some(error) = self.state.error {
            body = body.child(
                row()
                    .key("snippet-edit-error")
                    .w_full()
                    .padding(8.0)
                    .radius(6.0)
                    .fill(k.c.danger.bg.alpha(0.15)) // bg-danger/15
                    .child(
                        text(t(error.message_key()).to_owned())
                            .font(TEXT_XS)
                            .color(k.c.danger.fg),
                    ),
            );
        }

        // The prefix field (required, with the hint).
        let mut prefix_label = row().key("snippet-prefix-label").gap(4.0).center_y().child(
            text(format!("{} *", t("query.snippets.prefix")))
                .font(TEXT_XS_MEDIUM)
                .color(k.c.text_muted),
        );
        prefix_label = prefix_label.child(
            SettingHint::new(
                "snippet-prefix-hint".to_owned(),
                t("query.snippets.prefix").to_owned(),
                t("query.snippets.prefixHint").to_owned(),
            )
            .build(k, layer),
        );
        let prefix_lens = Rc::clone(&self.prefix_edit);
        let prefix_field = TextField::new("snippet-prefix", &self.state.prefix)
            .height(36.0)
            .placeholder(t("query.snippets.prefixPlaceholder"))
            .on_edit::<S>(move |s: &mut S| prefix_lens(s));
        let mut prefix_block = col().key("snippet-prefix-block").w_full().gap(6.0);
        prefix_block = prefix_block.child(prefix_label).child(prefix_field.build(self.dark));
        body = body.child(prefix_block);

        // The description field.
        let description_lens = Rc::clone(&self.description_edit);
        let description_field = TextField::new("snippet-description", &self.state.description)
            .height(36.0)
            .placeholder(t("query.snippets.descriptionPlaceholder"))
            .on_edit::<S>(move |s: &mut S| description_lens(s));
        let mut description_block = col().key("snippet-description-block").w_full().gap(6.0);
        description_block = description_block.child(
            text(t("query.snippets.description").to_owned())
                .font(TEXT_XS_MEDIUM)
                .color(k.c.text_muted),
        );
        description_block = description_block.child(description_field.build(self.dark));
        body = body.child(description_block);

        // The template field (required, mono, with the syntax guide
        // hint).
        let mut template_label =
            row().key("snippet-template-label").gap(4.0).center_y().child(
                text(format!("{} *", t("query.snippets.template")))
                    .font(TEXT_XS_MEDIUM)
                    .color(k.c.text_muted),
            );
        template_label = template_label.child(
            SettingHint::new(
                "snippet-template-hint".to_owned(),
                t("query.snippets.template").to_owned(),
                format!(
                    "{}\n{}",
                    t("query.snippets.syntaxGuideTitle"),
                    t("query.snippets.syntaxGuide")
                ),
            )
            .build(k, layer),
        );
        let template_lens = Rc::clone(&self.template_edit);
        let template_field = TextField::new("snippet-template", &self.state.template)
            .height(150.0)
            .placeholder(t("query.snippets.templatePlaceholder"))
            .text_style(TextStyle::new(12.0, 400.0).mono())
            .on_edit::<S>(move |s: &mut S| template_lens(s));
        let mut template_block = col().key("snippet-template-block").w_full().gap(6.0);
        template_block = template_block.child(template_label).child(template_field.build(self.dark));
        body = body.child(template_block);

        dialog = dialog.child(body);

        // The footer: ghost Cancel + primary Save (the right-aligned
        // pair).
        let mut cancel = Button::new(t("common.cancel"))
            .variant(ButtonVariant::Ghost)
            .control_size(ControlSize::Small);
        if let Some(on_close) = &self.on_close {
            let on_close = Rc::clone(on_close);
            cancel = cancel.on_press(move |s: &mut S| on_close(s));
        }
        let mut save = Button::new(t("common.save"))
            .variant(ButtonVariant::Submit) // React primary
            .control_size(ControlSize::Small);
        if let Some(on_save_pressed) = &self.on_save_pressed {
            let on_save_pressed = Rc::clone(on_save_pressed);
            save = save.on_press(move |s: &mut S| on_save_pressed(s));
        }
        let footer = row()
            .key("snippet-edit-footer")
            .w_full()
            .gap(8.0)
            .justify_end()
            .child(cancel.build(k).key("snippet-edit-cancel"))
            .child(save.build(k).key("snippet-edit-save"));
        dialog = dialog.footer(footer);

        dialog.build(k, layer);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snippet() -> SqlSnippetItem {
        SqlSnippetItem {
            id: "snip-1".to_owned(),
            prefix: "sel*".to_owned(),
            description_key: "query.editor.snippet.selectAll".to_owned(),
            template: "SELECT * FROM ${1:t};".to_owned(),
        }
    }

    #[test]
    fn the_prefix_regex_is_the_react_one() {
        assert!(prefix_valid("sel*"));
        assert!(prefix_valid("a"));
        assert!(prefix_valid("_under_score9"));
        assert!(!prefix_valid(""), "empty");
        assert!(!prefix_valid("1abc"), "starts with a digit");
        assert!(!prefix_valid("a-b"), "hyphen not allowed");
        assert!(!prefix_valid("a b"), "space not allowed");
        assert!(!prefix_valid("*abc"), "leading star not allowed");
    }

    #[test]
    fn the_seed_follows_the_mode() {
        let mut state = SnippetEditDialogState::new();
        // An add clears.
        state.seed(true, None, false);
        assert!(state.open);
        assert_eq!(state.prefix.text, "");
        assert!(state.error.is_none());
        // An edit seeds verbatim.
        state.seed(true, Some(&snippet()), false);
        assert_eq!(state.prefix.text, "sel*");
        assert_eq!(state.template.text, "SELECT * FROM ${1:t};");
        // A duplicate gets the _copy prefix.
        state.seed(true, Some(&snippet()), true);
        assert_eq!(state.prefix.text, "sel*_copy");
        // Closing keeps the draft (the dialog unmounts; the next open
        // reseeds).
        state.seed(false, Some(&snippet()), false);
        assert!(!state.open);
    }

    #[test]
    fn the_save_validates_in_order() {
        let mut state = SnippetEditDialogState::new();
        state.seed(true, None, false);
        // Empty prefix.
        assert_eq!(
            state.save(None, || "fresh".to_owned(), &[]),
            Err(SnippetSaveError::PrefixInvalid)
        );
        assert_eq!(state.error, Some(SnippetSaveError::PrefixInvalid));
        // An invalid shape.
        state.prefix = TextEdit::new("1bad".to_owned());
        assert_eq!(
            state.save(None, || "fresh".to_owned(), &[]),
            Err(SnippetSaveError::PrefixInvalid)
        );
        // A duplicate (case-insensitive).
        state.prefix = TextEdit::new("SEL*".to_owned());
        assert_eq!(
            state.save(None, || "fresh".to_owned(), &["sel*".to_owned()]),
            Err(SnippetSaveError::PrefixDuplicate)
        );
        // An empty template.
        state.prefix = TextEdit::new("nuevo*".to_owned());
        state.template = TextEdit::new("   ".to_owned());
        assert_eq!(
            state.save(None, || "fresh".to_owned(), &[]),
            Err(SnippetSaveError::TemplateRequired)
        );
    }

    #[test]
    fn the_save_builds_the_item() {
        let mut state = SnippetEditDialogState::new();
        state.seed(true, Some(&snippet()), false);
        state.prefix = TextEdit::new("  nuevo*  ".to_owned());
        state.description = TextEdit::new("  ".to_owned());
        state.template = TextEdit::new("  SELECT 1;  ".to_owned());
        // The description falls back to the trimmed prefix.
        let item = state.save(Some("snip-1"), || "fresh".to_owned(), &[]).unwrap();
        assert_eq!(item.id, "snip-1", "the edited id is kept");
        assert_eq!(item.prefix, "nuevo*");
        assert_eq!(item.description_key, "nuevo*");
        assert_eq!(item.template, "SELECT 1;");
        // An add takes the fresh id.
        let item = state.save(None, || "fresh-2".to_owned(), &[]).unwrap();
        assert_eq!(item.id, "fresh-2");
        // A kept description.
        state.description = TextEdit::new(" my desc ".to_owned());
        let item = state.save(None, || "fresh-3".to_owned(), &[]).unwrap();
        assert_eq!(item.description_key, "my desc");
    }
}
