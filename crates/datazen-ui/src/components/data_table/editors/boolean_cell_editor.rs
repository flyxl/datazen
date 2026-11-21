//! The boolean cell editor, ported from React
//! `packages/frontend/src/components/DataTable/editors/BooleanCellEditor.tsx`
//! (T278).
//!
//! A nullable column gets **three** states, because two of them are the
//! same wire value with a different meaning: `NULL` is not `false`. A
//! single checkbox cannot express that, so the nullable case is a
//! three-item select and the NOT NULL case is the themed checkbox
//! (05 §3.5, §11 #2).
//!
//! `Enter` belongs to the select — it opens the option list — so it is
//! not forwarded to the host. The value is written when an option is
//! picked.
//!
//! ## Keys
//!
//! | part | key |
//! |---|---|
//! | the control (React test id, both branches) | `cell-editor-boolean` |
//!
//! ## Props read
//!
//! The React control takes the host's whole `CellEditorControlProps`
//! but reads `draft`, `nullable`, `ariaLabel`, `onPick`, `onKeyDown`
//! and `onBlur`. The port takes the first four — the key and blur
//! channels are the host's wiring (T297; the select's own `Enter`
//! ownership is documented below), and Rune's controls carry their own
//! key handling.
//!
//! ## Documented deviations from React
//!
//! * **The select owns `Enter`.** React's `handleKeyDown` stops the
//!   host's `Enter` (it opens the option list instead); Rune's select
//!   owns its own keys outright, so no interception is needed — the
//!   outcome is the same: Enter never commits through the host.
//! * **The checkbox's press is the toggle.** React reads
//!   `event.target.checked`; Rune's kit checkbox toggles on press, so
//!   the editor computes the next value (`!checked`) itself — the same
//!   value React's event would report.
//! * **`sr-only` labels** ride the kit controls' own a11y label
//!   channels (`Checkbox::label`, the select's trigger label).

use std::rc::Rc;

use rune::{row, El};
use rune_kit::Kit;
use serde_json::Value;

use crate::ui::select::Select;
use crate::ui::toggle::Checkbox;

/// The option that stands for SQL `NULL` in the three-state control
/// (React's `NULL_OPTION`).
const NULL_OPTION: &str = "NULL";

/// The three options (React's `OPTIONS`): NULL, true, false.
const OPTIONS: [&str; 3] = [NULL_OPTION, "true", "false"];

/// One discrete choice: `None` is SQL `NULL`, `Some(Value::Bool)` is
/// the boolean (`onPick`'s `Value | null`).
type Pick<S> = Rc<dyn Fn(&mut S, Option<Value>)>;

/// `BooleanCellEditor`: the two-branch control.
pub(crate) struct BooleanCellEditor<S: 'static> {
    draft: String,
    nullable: bool,
    aria_label: String,
    on_pick: Option<Pick<S>>,
    /// The host's select-open cell (the three-state branch's
    /// listbox); the enabled-select rule makes the toggle real.
    select_open: bool,
    on_select_toggle: Option<Rc<dyn Fn(&mut S)>>,
    on_select_close: Option<Rc<dyn Fn(&mut S)>>,
}

impl<S: 'static> BooleanCellEditor<S> {
    /// The control over the fields it reads (`draft`, `nullable`,
    /// `ariaLabel`).
    pub(crate) fn new(draft: impl Into<String>, nullable: bool, aria_label: impl Into<String>) -> Self {
        Self {
            draft: draft.into(),
            nullable,
            aria_label: aria_label.into(),
            on_pick: None,
            select_open: false,
            on_select_toggle: None,
            on_select_close: None,
        }
    }

    /// React `onPick(value | null)`: a discrete choice was made — it
    /// is written right away.
    pub(crate) fn on_pick(mut self, f: impl Fn(&mut S, Option<Value>) + 'static) -> Self {
        self.on_pick = Some(Rc::new(f));
        self
    }

    /// The three-state select's listbox open flag (the host's cell).
    pub(crate) fn select_open(mut self, open: bool) -> Self {
        self.select_open = open;
        self
    }

    /// The select trigger's press: open or close the listbox (the
    /// enabled-select rule).
    pub(crate) fn on_select_toggle(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_select_toggle = Some(Rc::new(f));
        self
    }

    /// Presses outside and Escape close the select's listbox.
    pub(crate) fn on_select_close(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_select_close = Some(Rc::new(f));
        self
    }

    /// The control — the checkbox for NOT NULL, the three-item select
    /// for nullable. The select's listbox rides `layer`.
    pub(crate) fn build(self, k: &Kit, layer: &mut Vec<El>) -> El {
        if !self.nullable {
            return self.checkbox(k);
        }
        self.three_state_select(k, layer)
    }

    /// The NOT NULL branch (React 34-48): the themed checkbox,
    /// `checked={draft === 'true'}`, the wrapper `flex h-7 w-full
    /// items-center justify-center`.
    fn checkbox(&self, k: &Kit) -> El {
        let checked = self.draft == "true";
        let mut checkbox = Checkbox::new("cell-editor-boolean", checked)
            .label(self.aria_label.clone());
        if let Some(on_pick) = &self.on_pick {
            let on_pick = Rc::clone(on_pick);
            // React reads event.target.checked; the press toggles, so
            // the next value is the opposite of the current one.
            let next = !checked;
            checkbox = checkbox.on_toggle(move |s: &mut S| on_pick(s, Some(Value::Bool(next))));
        }
        row()
            .key("cell-editor-boolean-row")
            .w_full()
            .h(28.0)
            .center()
            .child(checkbox.build(k))
    }

    /// The nullable branch (React 50-74): the three-item select —
    /// `selected = draft === 'true' || draft === 'false' ? draft :
    /// NULL_OPTION`, the choice mapping NULL → `onPick(null)` and the
    /// booleans → `onPick(value === 'true')`, the wrapper `flex h-7
    /// w-full items-center`.
    fn three_state_select(&self, k: &Kit, layer: &mut Vec<El>) -> El {
        let selected = if self.draft == "true" || self.draft == "false" {
            self.draft.as_str()
        } else {
            NULL_OPTION
        };
        let mut select = Select::new("cell-editor-boolean", &OPTIONS)
            .open(self.select_open)
            .current(Some(selected));
        if let Some(on_pick) = &self.on_pick {
            let on_pick = Rc::clone(on_pick);
            select = select.on_choose(move |s: &mut S, value: &str, _tx: &mut rune::Tx| {
                if value == NULL_OPTION {
                    on_pick(s, None);
                } else {
                    on_pick(s, Some(Value::Bool(value == "true")));
                }
            });
        }
        if let Some(on_toggle) = &self.on_select_toggle {
            let on_toggle = Rc::clone(on_toggle);
            select = select.on_toggle(move |s: &mut S| on_toggle(s));
        }
        if let Some(on_close) = &self.on_select_close {
            let on_close = Rc::clone(on_close);
            select = select.on_close(move |s: &mut S| on_close(s));
        }
        row()
            .key("cell-editor-boolean-row")
            .w_full()
            .h(28.0)
            .center_y()
            .child(select.build(k, layer))
    }
}
