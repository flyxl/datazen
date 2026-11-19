//! Port of `packages/frontend/src/components/filter/FilterTokenInput.tsx`.
//!
//! The value editor of a set operator (`in` / `notIn`): a list of chips plus a
//! text field that becomes a chip on Enter or on a comma, and removes the last
//! chip on Backspace when the field is empty. The token list is owned by the
//! caller (React's `value: string[]`) and reported back through
//! `onCommit(Vec<String>)` — never joined into a comma string, so a token that
//! itself contains a comma survives the round trip.
//!
//! ## Keys
//!
//! | part        | key                              | React `data-testid`     |
//! |-------------|----------------------------------|-------------------------|
//! | root        | `<prefix>-tokens`                | `filter-token-input`    |
//! | chip n      | `<prefix>-token-<n>`             | `filter-token-<n>`      |
//! | chip remove | `<prefix>-token-<n>-remove`      | `filter-token-remove-<n>`|
//! | field       | `<prefix>-token-field`           | `filter-token-field`    |
//!
//! The indexed React markers are unique keys (React repeats `filter-token` on
//! every chip, but Rune keys must be unique among siblings). `autoFocus` has no
//! Rune equivalent and is intentionally absent.
//!
//! IME discipline is structural here: Rune's runtime withholds Return from a
//! focused field while a composition is open, which is React's manual
//! `composingRef` guard.
//!
//! Removal stage: drop the per-item allows when the filter panel view mounts
//! this control (T024).

use std::rc::Rc;

use rune::{el, icon, row, text, El, Key, KeyEvent, TextEdit, Tx};

use crate::icons::ICON_X;
use crate::ui::field::TextField;
use crate::ui::kit::kit;

type CommitFn<S> = Rc<dyn Fn(&mut S, Vec<String>)>;
type CompleteFn<S> = Rc<dyn Fn(&mut S)>;

/// Builder for the ported token input.
#[allow(dead_code)] // consumer: FilterConditionEditor (this wave).
pub(crate) struct FilterTokenInput<'a, S: 'static> {
    key: String,
    tokens: Vec<String>,
    draft: &'a TextEdit,
    bind: fn(&mut S) -> &mut TextEdit,
    placeholder: String,
    disabled: bool,
    remove_label: String,
    on_commit: Option<CommitFn<S>>,
    on_enter_complete: Option<CompleteFn<S>>,
}

#[allow(dead_code)] // consumer: FilterConditionEditor (this wave).
impl<'a, S: 'static> FilterTokenInput<'a, S> {
    pub(crate) fn new(
        key: impl Into<String>,
        tokens: Vec<String>,
        draft: &'a TextEdit,
        bind: fn(&mut S) -> &mut TextEdit,
    ) -> Self {
        Self {
            key: key.into(),
            tokens,
            draft,
            bind,
            placeholder: String::new(),
            disabled: false,
            remove_label: String::new(),
            on_commit: None,
            on_enter_complete: None,
        }
    }

    pub(crate) fn placeholder(mut self, value: impl Into<String>) -> Self {
        self.placeholder = value.into();
        self
    }

    pub(crate) fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }

    /// React `removeLabel`: the chip remove button's accessible name.
    pub(crate) fn remove_label(mut self, label: impl Into<String>) -> Self {
        self.remove_label = label.into();
        self
    }

    /// React `onCommit(tokens)`: called with the complete next token list.
    pub(crate) fn on_commit(mut self, f: impl Fn(&mut S, Vec<String>) + 'static) -> Self {
        self.on_commit = Some(Rc::new(f));
        self
    }

    /// React `onEnterComplete`: called only after a non-empty Enter commit.
    pub(crate) fn on_enter_complete(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_enter_complete = Some(Rc::new(f));
        self
    }

    pub(crate) fn build(self, dark: bool) -> El {
        let k = kit(dark);
        let bind = self.bind;
        let commit = self.on_commit.clone();
        let complete = self.on_enter_complete.clone();

        let mut list = row()
            .key(format!("{}-tokens", self.key))
            .min_w(0.0)
            .max_w(256.0)
            .gap(2.0)
            .center_y()
            .child(el().key("filter-token-input").a11y_hidden());

        for (position, token) in self.tokens.iter().enumerate() {
            list = list.child(chip(
                k,
                &format!("{}-token-{position}", self.key),
                position,
                token,
                &self.tokens,
                &self.remove_label,
                self.disabled,
                commit.clone(),
            ));
        }

        let mut field = TextField::new(format!("{}-token-field", self.key), self.draft)
            .height(28.0)
            .width(80.0)
            .pad_x(8.0)
            .text_style(k.ty.text_xsmall)
            .placeholder(self.placeholder)
            .disabled(self.disabled);
        if !self.disabled {
            let tokens = self.tokens.clone();
            field = field.on_edit(bind).on_key(
                move |s: &mut S, event: &KeyEvent, _tx: &mut Tx| match event.key {
                    Key::Enter => {
                        let token = bind(s).text.trim().to_owned();
                        if token.is_empty() {
                            return true;
                        }
                        bind(s).replace_text(String::new());
                        let mut next = tokens.clone();
                        next.push(token);
                        if let Some(commit) = &commit {
                            commit(s, next);
                        }
                        if let Some(complete) = &complete {
                            complete(s);
                        }
                        true
                    }
                    Key::Text(ref value) if value.as_str() == "," => {
                        let token = bind(s).text.trim().to_owned();
                        if token.is_empty() {
                            return true;
                        }
                        bind(s).replace_text(String::new());
                        let mut next = tokens.clone();
                        next.push(token);
                        if let Some(commit) = &commit {
                            commit(s, next);
                        }
                        true
                    }
                    Key::Backspace if bind(s).text.is_empty() => {
                        if tokens.is_empty() {
                            return false;
                        }
                        let mut next = tokens.clone();
                        next.pop();
                        if let Some(commit) = &commit {
                            commit(s, next);
                        }
                        true
                    }
                    _ => false,
                },
            );
        }
        list.child(
            row()
                .key(format!("{}-token-field-row", self.key))
                .min_w(0.0)
                .child(field.build(dark))
                .child(el().key("filter-token-field").a11y_hidden()),
        )
    }
}

fn chip<S: 'static>(
    k: &rune_kit::Kit,
    key: &str,
    position: usize,
    token: &str,
    all_tokens: &[String],
    remove_label: &str,
    disabled: bool,
    commit: Option<CommitFn<S>>,
) -> El {
    let mut remove = row()
        .role("button")
        .key(format!("{key}-remove"))
        .size(12.0, 12.0)
        .center()
        .radius(k.radius.sm)
        .label(remove_label.to_owned())
        .help(remove_label.to_owned())
        .focusable(!disabled)
        .child(icon(&ICON_X, 10.0).color(k.c.text_accent).a11y_hidden());
    if disabled {
        remove = remove.disabled(true).layer_opacity(0.5);
    } else if let Some(commit) = commit {
        // React `value.filter((_, i) => i !== position)` — the position is the
        // identity, even for duplicate token strings.
        let next: Vec<String> = all_tokens
            .iter()
            .enumerate()
            .filter(|(i, _)| *i != position)
            .map(|(_, value)| value.clone())
            .collect();
        remove = remove.on_press(move |s: &mut S| commit(s, next.clone()));
    }
    row()
        .key(key)
        .min_w(0.0)
        .max_w(128.0)
        .gap(2.0)
        .center_y()
        .radius(k.radius.sm)
        .pl(4.0)
        .pr(2.0)
        .py(1.0)
        .fill(k.alpha(k.c.bg_accent, 0.15))
        .child(
            text(token.to_owned())
                .key("label")
                .font(k.ty.small)
                .color(k.c.text_accent)
                .truncate(),
        )
        .child(remove)
        .child(el().key(format!("filter-token-{position}")).a11y_hidden())
        .child(el().key(format!("filter-token-remove-{position}")).a11y_hidden())
}
