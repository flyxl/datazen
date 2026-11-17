//! Temporal value input: a single-line editor for date, time and datetime cells.
//!
//! This is the Rune port of `packages/ui/src/TemporalValueInput.tsx` as an
//! in-place cell editor. The React control is a free-typing input with a
//! calendar popover; the Rune base keeps the typing contract and the inline
//! validation, and leaves calendar picking to a later overlay task:
//!
//! - `TemporalKind::Date` accepts `YYYY-MM-DD` and commits it verbatim.
//! - `TemporalKind::Time` accepts `H:MM` / `H:MM:SS` (one or two hour digits),
//!   checks 24h ranges and commits canonical zero-padded `HH:MM:SS`.
//! - `TemporalKind::DateTime` accepts `YYYY-MM-DD[T ]H:MM[:SS]` and commits the
//!   typed text verbatim (the separator the user typed is preserved).
//!
//! Empty input means SQL `NULL` when the input is nullable, and is refused with
//! `cellEditor.notNull` when it is not. Validation messages are the same
//! `cellEditor.invalid*` keys the React editor uses.
//!
//! ## Key convention
//!
//! | part   | key           |
//! |--------|---------------|
//! | root   | `<key>`       |
//! | field  | `<key>-input` |
//! | error  | `<key>-error` |
//! | hint   | `<key>-hint`  |
//!
//! ## Commit and cancel
//!
//! `on_actions` wires the host callbacks: `Enter` commits when the text is
//! valid, `Escape` cancels and restores, and `Tab` commits and moves focus to
//! `tab_to` (or blurs when no target was set). An invalid `Tab` stays on the
//! field and does not commit, matching the React cell host. The error state
//! paints the field border and message in `kit.c.danger`; everything else is
//! the shared `TextField` chrome.
//!
//! Unlike the React control, which paints its error only after a commit
//! attempt, the Rune base validates the current text on every build: a
//! NOT NULL empty field or malformed text shows the error row immediately.
//! This is deliberate inline feedback for an in-place editor, not a stale
//! error left behind by a wiring bug.

#![allow(dead_code)]

use std::cell::RefCell;
use std::rc::Rc;

use rune::{col, text, El, Key, KeyEvent, TextEdit, Tx};

use super::field::TextField;
use super::kit::kit;
use crate::i18n::t;

/// Which temporal value the field edits.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum TemporalKind {
    /// `YYYY-MM-DD`.
    #[default]
    Date,
    /// `HH:MM:SS` after canonicalisation.
    Time,
    /// `YYYY-MM-DDTHH:MM:SS`.
    DateTime,
}

impl TemporalKind {
    /// The error message shown when the typed text does not parse.
    fn invalid_message(self) -> &'static str {
        match self {
            TemporalKind::Date => t("cellEditor.invalidDate"),
            TemporalKind::Time => t("cellEditor.invalidTime"),
            TemporalKind::DateTime => t("cellEditor.invalidDateTime"),
        }
    }

    /// The format hint under the field (overridable with `.hint`).
    fn default_hint(self) -> &'static str {
        match self {
            TemporalKind::Date => "YYYY-MM-DD",
            TemporalKind::Time => "HH:MM:SS",
            TemporalKind::DateTime => "YYYY-MM-DDTHH:MM:SS",
        }
    }
}

type FieldBind<'a> = Box<dyn FnOnce(TextField<'a>) -> TextField<'a> + 'a>;

/// A temporal cell editor: text field, inline validation and commit keys.
pub(crate) struct TemporalValueInput<'a> {
    key: String,
    edit: &'a TextEdit,
    kind: TemporalKind,
    nullable: bool,
    placeholder: Option<String>,
    hint: Option<String>,
    width: Option<f32>,
    height: f32,
    tab_target: Rc<RefCell<Option<String>>>,
    binder: Option<FieldBind<'a>>,
}

impl<'a> TemporalValueInput<'a> {
    /// A field over `edit`, editing a value of `kind`, 28 points tall,
    /// nullable, filling the parent's width.
    pub(crate) fn new(key: impl Into<String>, edit: &'a TextEdit, kind: TemporalKind) -> Self {
        Self {
            key: key.into(),
            edit,
            kind,
            nullable: true,
            placeholder: None,
            hint: None,
            width: None,
            height: 28.0,
            tab_target: Rc::new(RefCell::new(None)),
            binder: None,
        }
    }

    /// Placeholder text shown while the field is empty.
    pub(crate) fn placeholder(mut self, placeholder: impl Into<String>) -> Self {
        self.placeholder = Some(placeholder.into());
        self
    }

    /// Replace the format hint under the field; `None` hides it.
    pub(crate) fn hint(mut self, hint: Option<String>) -> Self {
        self.hint = hint;
        self
    }

    /// Whether empty input means SQL `NULL` (default) or is invalid.
    pub(crate) fn nullable(mut self, nullable: bool) -> Self {
        self.nullable = nullable;
        self
    }

    /// The box's height in points (default 28).
    pub(crate) fn height(mut self, height: f32) -> Self {
        self.height = height;
        self
    }

    /// A fixed width in points; without it the field fills its parent.
    pub(crate) fn width(mut self, width: f32) -> Self {
        self.width = Some(width);
        self
    }

    /// Where `Tab` sends focus after a successful commit. Without a target the
    /// field commits and blurs. The target is shared with the key handler, so
    /// this call may come before or after [`Self::on_actions`].
    pub(crate) fn tab_to(self, key: impl Into<String>) -> Self {
        *self.tab_target.borrow_mut() = Some(key.into());
        self
    }

    /// Wire the edit accessor and the host callbacks:
    ///
    /// - `commit` receives the canonical value (`""` for a nullable empty).
    /// - `cancel` restores the original value.
    ///
    /// `Enter` commits, `Tab` commits and moves focus, `Escape` cancels. The
    /// edit accessor is also bound to the field so it can take focus.
    pub(crate) fn on_actions<S: 'static>(
        mut self,
        edit: impl for<'b> Fn(&'b mut S) -> &'b mut TextEdit + Clone + 'static,
        commit: impl Fn(&mut S, &str) + Clone + 'static,
        cancel: impl Fn(&mut S) + Clone + 'static,
    ) -> Self {
        let kind = self.kind;
        let nullable = self.nullable;
        let tab_target = Rc::clone(&self.tab_target);

        let edit_submit = edit.clone();
        let commit_submit = commit.clone();
        let submit = move |state: &mut S, tx: &mut Tx| {
            let typed = edit_submit(state).text.clone();
            if let Ok(value) = committed_value(kind, &typed, nullable) {
                commit_submit(state, &value);
                tx.blur();
            }
        };

        let edit_key = edit.clone();
        let commit_key = commit.clone();
        let cancel_key = cancel.clone();
        let key = move |state: &mut S, event: &KeyEvent, tx: &mut Tx| -> bool {
            match event.key {
                Key::Escape => {
                    cancel_key(state);
                    tx.blur();
                    true
                }
                Key::Tab => {
                    let typed = edit_key(state).text.clone();
                    if let Ok(value) = committed_value(kind, &typed, nullable) {
                        commit_key(state, &value);
                        match tab_target.borrow().as_deref() {
                            Some(next) => tx.focus(next, true),
                            None => tx.blur(),
                        }
                    }
                    true
                }
                _ => false,
            }
        };

        self.binder = Some(Box::new(move |field: TextField<'a>| {
            field.on_edit(edit).on_submit(submit).on_key(key)
        }));
        self
    }

    /// Build the field and its validation line.
    pub(crate) fn build(self, dark: bool) -> El {
        let k = kit(dark);
        let key = self.key.clone();
        let message = validation_error(self.kind, &self.edit.text, self.nullable);

        let mut field = TextField::new(format!("{key}-input"), self.edit)
            .height(self.height)
            .placeholder(self.placeholder.clone().unwrap_or_default());
        if let Some(width) = self.width {
            field = field.width(width);
        }
        let field = match self.binder {
            Some(bind) => bind(field),
            None => {
                debug_assert!(
                    false,
                    "a temporal input without on_actions cannot edit or commit"
                );
                field
            }
        };
        let field = if message.is_some() {
            field.border(1.0, k.c.danger.border)
        } else {
            field
        };

        let mut root = col()
            .key(key.clone())
            .w_full()
            .gap(4.0)
            .child(field.build(dark));
        if let Some(error) = message {
            root = root.child(
                text(error)
                    .key(format!("{key}-error"))
                    .font(k.ty.text_xsmall)
                    .color(k.c.danger.fg),
            );
        } else if let Some(hint) = &self.hint {
            root = root.child(
                text(hint.clone())
                    .key(format!("{key}-hint"))
                    .font(k.ty.text_xsmall)
                    .color(k.c.text_muted),
            );
        }
        if let Some(width) = self.width {
            root = root.w(width);
        }
        root
    }
}

/// Validate `typed` and return the canonical committed value.
///
/// `Err` carries the user-facing message; an empty nullable value commits as
/// `""` (the host maps it to SQL `NULL`).
fn committed_value(
    kind: TemporalKind,
    typed: &str,
    nullable: bool,
) -> Result<String, &'static str> {
    if let Some(error) = validation_error(kind, typed, nullable) {
        return Err(error);
    }
    if typed.is_empty() {
        return Ok(String::new());
    }
    Ok(match kind {
        TemporalKind::Time => canonical_time(typed).unwrap_or_else(|| typed.to_owned()),
        TemporalKind::Date | TemporalKind::DateTime => typed.to_owned(),
    })
}

/// The error for `typed`, if it is not a valid value of `kind`.
fn validation_error(kind: TemporalKind, typed: &str, nullable: bool) -> Option<&'static str> {
    if typed.is_empty() {
        return if nullable {
            None
        } else {
            Some(t("cellEditor.notNull"))
        };
    }
    let valid = match kind {
        TemporalKind::Date => valid_date(typed),
        TemporalKind::Time => valid_time(typed),
        TemporalKind::DateTime => valid_datetime(typed),
    };
    if valid {
        None
    } else {
        Some(kind.invalid_message())
    }
}

fn digits(s: &str, len: usize) -> bool {
    s.len() == len && s.bytes().all(|b| b.is_ascii_digit())
}

fn number(s: &str) -> Option<u32> {
    if s.is_empty() || !s.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    s.parse().ok()
}

/// `YYYY-MM-DD`, including a real calendar day (leap years included).
fn valid_date(text: &str) -> bool {
    parse_date(text).is_some()
}

fn parse_date(text: &str) -> Option<(u32, u32, u32)> {
    let bytes = text.as_bytes();
    if bytes.len() != 10 || bytes[4] != b'-' || bytes[7] != b'-' {
        return None;
    }
    let (year, month, day) = (
        number(&text[0..4])?,
        number(&text[5..7])?,
        number(&text[8..10])?,
    );
    if !(1..=12).contains(&month) || day == 0 || day > days_in_month(year, month) {
        return None;
    }
    Some((year, month, day))
}

fn days_in_month(year: u32, month: u32) -> u32 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if is_leap(year) => 29,
        2 => 28,
        _ => 0,
    }
}

fn is_leap(year: u32) -> bool {
    (year % 4 == 0 && year % 100 != 0) || year % 400 == 0
}

/// `H:MM` or `H:MM:SS` with one or two hour digits and 24h ranges.
fn valid_time(text: &str) -> bool {
    parse_time(text).is_some()
}

fn parse_time(text: &str) -> Option<(u32, u32, u32)> {
    let parts: Vec<&str> = text.split(':').collect();
    if parts.len() != 2 && parts.len() != 3 {
        return None;
    }
    let hour = number(parts[0])?;
    if parts[0].len() > 2 || !digits(parts[1], 2) {
        return None;
    }
    let minute = number(parts[1])?;
    let second = if parts.len() == 3 {
        if !digits(parts[2], 2) {
            return None;
        }
        number(parts[2])?
    } else {
        0
    };
    if hour > 23 || minute > 59 || second > 59 {
        return None;
    }
    Some((hour, minute, second))
}

fn canonical_time(text: &str) -> Option<String> {
    let (hour, minute, second) = parse_time(text)?;
    Some(format!("{hour:02}:{minute:02}:{second:02}"))
}

/// `YYYY-MM-DD[T ]H:MM[:SS]`; the date and time halves are validated as above.
fn valid_datetime(text: &str) -> bool {
    let Some(split) = text.find(['T', ' ']) else {
        return false;
    };
    let (date, rest) = text.split_at(split);
    let time = &rest[1..];
    parse_date(date).is_some() && parse_time(time).is_some()
}
