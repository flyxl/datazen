//! Index connection fields, ported 1:1 from
//! `packages/frontend/src/components/connection/IndexConnectionFields.tsx`.
//!
//! React renders one labelled numeric input over `form.database`. Its onChange
//! keeps an empty string while typing, otherwise clamps `parseInt(v, 10) || 0`
//! into `0..=15`; onBlur defaults an empty value to `"0"`. Rune has no blur
//! event on a field, so [`clamp_index`] is the commit path and [`default_index`]
//! is applied on Enter (`on_submit`); the host must apply the same helpers when
//! it commits a value from elsewhere.
//!
//! ## Local contract (removal stage)
//!
//! `ConnectionFormState` has no Rust DTO in `datazen-ui`; [`IndexValue`] mirrors
//! only `database`. Remove it when the form state port lands and pass that value
//! directly.
//!
//! ## Keys
//!
//! `new-conn-database-index` (the React-form key), plus `-block` / `-label`
//! from the shared field layout.

use std::rc::Rc;

use rune::{col, El, TextEdit, Tx};

use crate::i18n::t;
use crate::ui::field::TextField;
use crate::ui::kit::kit;
use crate::ui::label::Label;

const ROOT: &str = "new-conn-database-index";

/// Mirror of `ConnectionFormState.database` for this field group.
#[allow(dead_code)] // contract mirror; remove when the form state port lands.
#[derive(Clone, Debug, Default)]
pub struct IndexValue {
    pub database: TextEdit,
}

/// JS `parseInt(v, 10) || 0`: leading decimal digits (with an optional sign),
/// no exponent, no trailing garbage. Overflow clamps to the far end so the
/// following `0..=15` clamp still agrees with JS.
fn js_parse_int(raw: &str) -> i64 {
    let trimmed = raw.trim_start();
    let (sign, rest) = if let Some(rest) = trimmed.strip_prefix('-') {
        (-1i64, rest)
    } else if let Some(rest) = trimmed.strip_prefix('+') {
        (1i64, rest)
    } else {
        (1i64, trimmed)
    };
    let digits: String = rest.chars().take_while(char::is_ascii_digit).collect();
    if digits.is_empty() {
        return 0;
    }
    sign * digits.parse::<i64>().unwrap_or(if sign < 0 { i64::MIN } else { i64::MAX })
}

/// React's onChange: `""` stays empty (so the user can clear the field), all
/// other input is clamped into `0..=15`.
#[allow(dead_code)] // commit helper; consumed by the field and the tests.
pub fn clamp_index(raw: &str) -> String {
    if raw.is_empty() {
        return String::new();
    }
    js_parse_int(raw).clamp(0, 15).to_string()
}

/// React's onBlur: an empty (or whitespace-only) value becomes `"0"`.
#[allow(dead_code)] // commit helper; consumed by the field and the tests.
pub fn default_index(raw: &str) -> String {
    if raw.trim().is_empty() {
        "0".to_owned()
    } else {
        raw.to_owned()
    }
}

type EditFn<S> = Rc<dyn for<'b> Fn(&'b mut S) -> &'b mut TextEdit + 'static>;

/// Builder for the single field. `S` owns the edited `TextEdit`.
#[allow(dead_code)] // consumer: the connection form (not wired); remove when wired.
pub struct IndexConnectionFields<'a, S: 'static> {
    value: &'a IndexValue,
    edit: EditFn<S>,
}

#[allow(dead_code)] // consumer: the connection form (not wired); remove when wired.
impl<'a, S: 'static> IndexConnectionFields<'a, S> {
    pub fn new(
        value: &'a IndexValue,
        edit: impl for<'b> Fn(&'b mut S) -> &'b mut TextEdit + 'static,
    ) -> Self {
        Self { value, edit: Rc::new(edit) }
    }

    pub fn build(self, dark: bool) -> El {
        let k = kit(dark);
        let mut field = TextField::new(ROOT, &self.value.database)
            .height(32.0)
            .pad_x(10.0)
            .radius(k.radius.sm)
            .fill(k.c.surface_base)
            .border(1.0, k.c.border_base)
            .placeholder("0");
        let on_edit = self.edit.clone();
        field = field.on_edit(rune::edit_access(move |s: &mut S| on_edit(s)));
        // Enter is the expressible commit boundary (Rune fields have no blur).
        let on_commit = self.edit.clone();
        field = field.on_submit(move |s: &mut S, _tx: &mut Tx| {
            let edit = on_commit(s);
            edit.text = default_index(&clamp_index(&edit.text));
        });
        col()
            .key(format!("{ROOT}-block"))
            .w_full()
            .gap(4.0)
            .child(Label::new(format!("{ROOT}-label"), t("newConn.databaseIndex")).build(dark))
            .child(field.build(dark))
    }
}
