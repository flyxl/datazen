//! Port of `packages/frontend/src/components/filter/FilterValueInput.tsx`.
//!
//! A single filter value: a local draft bound to a host-owned [`TextEdit`],
//! committed on Enter (and on blur in React). React's 350 ms debounce exists
//! because the parent builds a query from the draft; Rune reads the live draft
//! each frame, so a timer would only add a stale snapshot. The constant is kept
//! for parity and the deviation is documented.
//!
//! ## Keys
//!
//! | part  | key                    | React `data-testid` |
//! |-------|------------------------|---------------------|
//! | field | `<prefix>-value`       | `filter-value`      |
//! | row   | `<prefix>-value-row`   | —                   |
//! | marker| —                      | `filter-value`      |
//!
//! `autoFocus` has no Rune equivalent (views cannot move focus on mount); the
//! caller decides focus, and the option is intentionally absent here.
//!
//! Removal stage: this is a shared filter part; drop the per-item
//! `#[allow(dead_code)]` when the filter panel view mounts it (T024).

use std::rc::Rc;

use rune::{el, row, El, Key, KeyEvent, TextEdit, Tx};

use crate::ui::field::TextField;
use crate::ui::kit::kit;

/// React `FILTER_VALUE_DEBOUNCE_MS`. The Rune port reads the live draft, so no
/// timer is armed; the constant documents the source contract.
#[allow(dead_code)] // source parity; delete with the debounce if it is ever wired.
pub(crate) const FILTER_VALUE_DEBOUNCE_MS: u64 = 350;

type CommitFn<S> = Rc<dyn Fn(&mut S, String)>;
type CompleteFn<S> = Rc<dyn Fn(&mut S)>;

/// Builder for the ported single-value input.
#[allow(dead_code)] // consumer: FilterConditionEditor (this wave).
pub(crate) struct FilterValueInput<'a, S: 'static> {
    key: String,
    value: &'a TextEdit,
    bind: fn(&mut S) -> &mut TextEdit,
    placeholder: String,
    disabled: bool,
    on_commit: Option<CommitFn<S>>,
    on_enter_complete: Option<CompleteFn<S>>,
}

#[allow(dead_code)] // consumer: FilterConditionEditor (this wave).
impl<'a, S: 'static> FilterValueInput<'a, S> {
    pub(crate) fn new(
        key: impl Into<String>,
        value: &'a TextEdit,
        bind: fn(&mut S) -> &mut TextEdit,
    ) -> Self {
        Self {
            key: key.into(),
            value,
            bind,
            placeholder: String::new(),
            disabled: false,
            on_commit: None,
            on_enter_complete: None,
        }
    }

    /// React `placeholder` (the caller passes `t("filter.value")`).
    pub(crate) fn placeholder(mut self, placeholder: impl Into<String>) -> Self {
        self.placeholder = placeholder.into();
        self
    }

    /// React `disabled`: a disabled field keeps its value but takes no caret.
    pub(crate) fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }

    /// React `onCommit(value)`: called with the current draft on Enter.
    pub(crate) fn on_commit(mut self, f: impl Fn(&mut S, String) + 'static) -> Self {
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

        let mut field = TextField::new(format!("{}-value", self.key), self.value)
            .height(28.0)
            .width(96.0)
            .pad_x(8.0)
            .text_style(k.ty.text_xsmall)
            .placeholder(self.placeholder)
            .disabled(self.disabled);
        // IME discipline: Rune's runtime keeps Return/Escape away from a field
        // while a composition is open, which is React's `composingRef` guard.
        if !self.disabled {
            field = field.on_edit(bind).on_key(
                move |s: &mut S, event: &KeyEvent, _tx: &mut Tx| match event.key {
                    Key::Enter => {
                        let value = bind(s).text.clone();
                        if let Some(commit) = &commit {
                            commit(s, value.clone());
                        }
                        if !value.trim().is_empty() {
                            if let Some(complete) = &complete {
                                complete(s);
                            }
                        }
                        true
                    }
                    _ => false,
                },
            );
        }
        let field = field.build(dark);

        row()
            .key(format!("{}-value-row", self.key))
            .w_full()
            .min_w(0.0)
            .child(field)
            .child(el().key("filter-value").a11y_hidden())
    }
}

// Leftover L1 (removal stage): when the filter panel view owns the draft slice,
// pass its `TextEdit` accessor here and drop the blanket allow above. The blur
// commit React performs has no Rune element-level equivalent; the panel's
// focus tracking is the honest substitute and stays the caller's responsibility.
