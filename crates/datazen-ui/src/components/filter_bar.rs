//! The active-filter strip above the data grid.
//!
//! Ported from `packages/frontend/src/components/FilterBar.tsx` (58 lines):
//! a bottom-bordered bar holding the `filter.filter` label, one accent pill
//! per condition (label + remove button), and a clear-all button. Empty
//! renders nothing — React returns `null` — so [`FilterBar::build`]
//! returns `Option<El>`.
//!
//! The pills are built here, not reused from [`FilterConditionChip`]:
//! the chip renders `condition_summary` (the translated
//! `filter.<operator>` label, with an edit action) while this bar renders
//! React's `labelFor` (the raw operator id, no edit action) — see
//! [`filter_bar_label`].
//!
//! ## Key convention
//!
//! React gives this bar no `data-testid`, so the keys below are new stable
//! names under the caller's `<key>` prefix:
//!
//! | part | key |
//! |---|---|
//! | root | `<key>` |
//! | `filter.filter` label | `<key>-label` |
//! | chips row | `<key>-filters` |
//! | chip `i` | `<key>-filter-{i}` |
//! | chip `i` label | `<key>-filter-{i}-text` |
//! | chip `i` remove button | `<key>-filter-{i}-remove` |
//! | clear-all button | `<key>-clear` |
//!
//! ## Deliberate boundaries
//!
//! * **Loading.** React sets `aria-busy` / `aria-disabled` on the root and
//!   disables both buttons; the native buttons then swallow clicks. Rune
//!   has no `busy` attribute, so loading is expressed the way the chip
//!   expresses it: the root and every button carry `.disabled(true)`, the
//!   remove buttons leave the tab order (`focusable(false)`) and dim to
//!   `layer_opacity(0.5)`, and no press/key handler is attached while
//!   loading — a click or keypress dispatches nothing. The root's disabled
//!   state is visible in `Runtime::accessibility_json()`.
//! * **Wrapping.** React's bar and chips row both `flex-wrap`; Rune rows
//!   need an explicit `.flex_wrap()` (the plain `.wrap()` only wraps text —
//!   the history filter bar's `bar.wrap()` does not wrap its row).
//! * **Label case.** React uppercases the label (`uppercase`); the zh-CN
//!   string `筛选` has no case, so no transform is applied.
//! * **Clear button size.** React is `text-xs` (12pt); the kit has no 12pt
//!   button, so clear-all is a kit Ghost Small (13pt) like the history
//!   bar's reset button.
//! * **`className` is dropped.** React merges an optional extra class onto
//!   the root; Rune has no CSS classes, and the only caller
//!   (`DataTable.tsx:919`) passes none.
//! * **No `FilterConditionChip` reuse, no store slice.** The bar takes the
//!   conditions it renders; the DataTable view that owns the filter list
//!   (and removes / clears it) is a later task, as is the filter panel
//!   that builds the conditions. Until then the component tests are the
//!   only callers.

// The production consumer is the DataTable view wiring task; until then the
// component tests are the only callers, so the dead-code lint is silenced
// here rather than in a crate-wide allow.
#![allow(dead_code)]

use std::rc::Rc;

use rune::{icon, row, text, El, Key, KeyEvent, Tx};
use rune_kit::{Button, ButtonVariant, ControlSize};
use serde_json::Value;

use super::filter::{FilterCondition, FilterValue};
use crate::i18n::t;
use crate::icons::ICON_X;
use crate::support::formatters::format_cell;
use crate::ui::kit::kit;

/// React's `labelFor` (`FilterBar.tsx:12–14`), reproduced literally: the
/// raw operator id — `status eq paid`, not the translated `filter.eq`
/// the chip shows — and the value through `format_cell`. A set/range value
/// therefore renders as its JSON array text (`age between ["1","9"]`,
/// `status in ["a","b"]`), exactly as React's `formatCell` shows it; only
/// an absent value (`FilterValue::None`, React `undefined`) drops the
/// suffix. `FilterValue::Single` already carries the `format_cell` output
/// for its value (a null value arrives as `"NULL"`, as React renders it).
pub(crate) fn filter_bar_label(filter: &FilterCondition) -> String {
    let base = format!("{} {}", filter.column, filter.operator);
    match &filter.value {
        FilterValue::None => base,
        FilterValue::Single(value) => format!("{base} {value}"),
        FilterValue::Pair(lower, upper) => format!(
            "{base} {}",
            format_cell(&Value::Array(vec![
                Value::String(lower.clone()),
                Value::String(upper.clone()),
            ]))
        ),
        FilterValue::Tokens(tokens) => format!(
            "{base} {}",
            format_cell(&Value::Array(
                tokens.iter().map(|t| Value::String(t.clone())).collect()
            ))
        ),
    }
}

/// The bar. Build it with the conditions the view already owns, then hand
/// it the remove / clear callbacks — the same shape as `Pagination<S>`.
pub(crate) struct FilterBar<'a, S: 'static> {
    key: String,
    filters: &'a [FilterCondition],
    loading: bool,
    on_remove: Option<Rc<dyn Fn(&mut S, usize)>>,
    on_clear: Option<Rc<dyn Fn(&mut S)>>,
}

impl<'a, S: 'static> FilterBar<'a, S> {
    pub(crate) fn new(key: impl Into<String>, filters: &'a [FilterCondition]) -> Self {
        Self {
            key: key.into(),
            filters,
            loading: false,
            on_remove: None,
            on_clear: None,
        }
    }

    /// Prevents filter mutations while the next page is loading
    /// (`FilterBar.tsx:11`).
    pub(crate) fn loading(mut self, loading: bool) -> Self {
        self.loading = loading;
        self
    }

    /// React's `onRemove(index)`.
    pub(crate) fn on_remove(mut self, f: impl Fn(&mut S, usize) + 'static) -> Self {
        self.on_remove = Some(Rc::new(f));
        self
    }

    /// React's `onClear()`.
    pub(crate) fn on_clear(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_clear = Some(Rc::new(f));
        self
    }

    /// One accent pill: the label plus its remove button. The pill follows
    /// the `Badge` geometry (`rounded-[9px] px-2 py-0.5`, accent/10 wash,
    /// accent/20 hairline) with the label in `text-accent`; the remove
    /// button is React's `rounded-sm p-0.5` around a 14pt X, i.e. an 18pt
    /// box. The remove ink is `text-accent/70`, as React paints it (the
    /// chip's own X is /80).
    fn chip(&self, dark: bool, index: usize, filter: &FilterCondition) -> El {
        let k = kit(dark);
        let key = &self.key;
        let label = filter_bar_label(filter);
        let remove_label = t("filter.remove");

        let mut remove = row()
            .role("button")
            .size(18.0, 18.0)
            .center()
            .radius(k.radius.sm)
            .label(remove_label)
            .help(remove_label)
            .child(
                icon(&ICON_X, 14.0)
                    .key("icon")
                    .color(k.alpha(k.c.text_accent, 0.7))
                    .a11y_hidden(),
            )
            .focusable(!self.loading)
            .key(format!("{key}-filter-{index}-remove"));
        if self.loading {
            remove = remove.disabled(true).layer_opacity(0.5);
        }
        if let Some(on_remove) = &self.on_remove {
            if !self.loading {
                // React's native `<button>` also fires on Enter; Rune
                // presses a focused button with Space only, so the pill
                // carries an explicit Enter handler (the chip's shape).
                let press = Rc::clone(on_remove);
                let enter = Rc::clone(on_remove);
                remove = remove.on_press(move |s: &mut S| press(s, index)).on_key(
                    move |s: &mut S, ev: &KeyEvent, _tx: &mut Tx| {
                        if ev.key == Key::Enter {
                            enter(s, index);
                            true
                        } else {
                            false
                        }
                    },
                );
            }
        }

        row()
            .key(format!("{key}-filter-{index}"))
            .self_start()
            .min_w(0.0)
            .shrink()
            .gap(8.0)
            .center_y()
            .radius(9.0)
            .px(8.0)
            .py(2.0)
            .fill(k.alpha(k.c.bg_accent, 0.10))
            .border(1.0, k.alpha(k.c.bg_accent, 0.20))
            .disabled(self.loading)
            .child(
                text(label.clone())
                    .key(format!("{key}-filter-{index}-text"))
                    .font(k.ty.small_medium)
                    .color(k.c.text_accent)
                    .truncate()
                    .shrink()
                    .min_w(0.0)
                    .help(label),
            )
            .child(remove)
    }

    /// `None` when there is nothing to show (`filters.length === 0`
    /// returns `null` in React, `FilterBar.tsx:18`).
    pub(crate) fn build(self, dark: bool) -> Option<El> {
        if self.filters.is_empty() {
            return None;
        }
        let k = kit(dark);
        let key = self.key.clone();
        let loading = self.loading;

        let mut chips = row()
            .key(format!("{key}-filters"))
            .grow(1.0)
            .min_w(0.0)
            .flex_wrap()
            .center_y()
            .gap(8.0);
        for (index, filter) in self.filters.iter().enumerate() {
            chips = chips.child(self.chip(dark, index, filter));
        }

        let mut clear = Button::new(t("filter.clear"))
            .variant(ButtonVariant::Ghost)
            .control_size(ControlSize::Small)
            .disabled(loading);
        if let Some(on_clear) = &self.on_clear {
            if !loading {
                let f = Rc::clone(on_clear);
                clear = clear.on_press(move |s: &mut S| f(s));
            }
        }
        let mut clear_el = clear.build(k).key(format!("{key}-clear"));
        if !loading {
            if let Some(on_clear) = &self.on_clear {
                let f = Rc::clone(on_clear);
                clear_el = clear_el.on_key(move |s: &mut S, ev: &KeyEvent, _tx: &mut Tx| {
                    if ev.key == Key::Enter {
                        f(s);
                        true
                    } else {
                        false
                    }
                });
            }
        }

        Some(
            row()
                .key(key.clone())
                .w_full()
                .flex_wrap()
                .center_y()
                .gap(8.0)
                .px(12.0)
                .py(8.0)
                .fill(k.c.surface_base)
                .border_sides([0.0, 0.0, 1.0, 0.0], k.c.border_base)
                .border_layout()
                .disabled(loading)
                .child(
                    text(t("filter.filter").to_string())
                        .key(format!("{key}-label"))
                        .font(k.ty.small_medium)
                        .color(k.c.text_muted),
                )
                .child(chips)
                .child(clear_el),
        )
    }
}
