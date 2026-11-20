//! Port of `packages/frontend/src/components/FilterBar.tsx` (T014).
//!
//! The bar lists the active column filters as accent chips, each with its
//! formatted summary and a remove button, plus a trailing "clear" button. It
//! returns nothing at all for an empty list (React returns `null`), so
//! [`FilterBar::build`] is an `Option<El>` — a caller mounts it only when there
//! is something to show.
//!
//! ## React → Rune
//!
//! | React                                              | Rune                                        |
//! |----------------------------------------------------|---------------------------------------------|
//! | `border-b border-edge bg-surface px-3 py-2`         | bottom hairline, `k.c.bg_base`, 12/8 padding |
//! | `text-[11px] font-semibold uppercase tracking-wider` | `k.ty.small_medium` + `TextCase::Uppercase` |
//! | `Badge tone="accent"`                               | 10% accent wash, 20% accent ring, 9pt radius|
//! | `X h-3.5 w-3.5`                                     | registry `ICON_X` at 14pt                   |
//! | `disabled={loading}` / `aria-busy`                  | `El::disabled` + handlers left unattached   |
//!
//! `tracking-wider` has no `El`-level letter-spacing API in this Rune version;
//! the uppercase small-medium font is the closest kit-native match. Copy
//! comes from `crate::i18n::t` (`filter.filter`, `filter.remove`,
//! `filter.clear`); no UI literal is hard-coded.
//!
//! ## Keys
//!
//! | part      | key                                  |
//! |-----------|--------------------------------------|
//! | root      | `filter-bar`                         |
//! | label     | `filter-bar-label`                   |
//! | filters   | `filter-bar-filters`                 |
//! | badge i   | `filter-bar-filter-{i}`              |
//! | summary i | `filter-bar-filter-{i}-label`        |
//! | remove i  | `filter-bar-filter-{i}-remove`       |
//! | clear     | `filter-bar-clear`                   |

#![allow(dead_code)] // the query view (later track) is the production caller

use std::rc::Rc;

use rune::{icon, row, text, El, Key, KeyEvent, TextCase, Tx};
use serde_json::Value;

use crate::i18n::t;
use crate::icons::ICON_X;
use crate::support::formatters::format_cell;
use crate::ui::kit::kit;

/// Root bar, React's `flex flex-wrap items-center gap-2 border-b … px-3 py-2`.
pub(crate) const ROOT_KEY: &str = "filter-bar";
/// The `filter.filter` label.
pub(crate) const LABEL_KEY: &str = "filter-bar-label";
/// The `flex min-w-0 flex-1 flex-wrap gap-2` chip container.
pub(crate) const FILTERS_KEY: &str = "filter-bar-filters";
/// The trailing `filter.clear` button.
pub(crate) const CLEAR_KEY: &str = "filter-bar-clear";

/// React `gap-2` / `px-3` / `py-2`.
const GAP: f32 = 8.0;
const PAD_X: f32 = 12.0;
const PAD_Y: f32 = 8.0;
/// React `Badge`：`text-[11px] rounded-[9px] px-2 py-0.5`.
const BADGE_RADIUS: f32 = 9.0;
const BADGE_PAD_X: f32 = 8.0;
const BADGE_PAD_Y: f32 = 2.0;
/// React `X h-3.5 w-3.5` (14pt glyph in an 18pt hit box).
const REMOVE_ICON: f32 = 14.0;
const REMOVE_BOX: f32 = 18.0;

pub(crate) fn badge_key(idx: usize) -> String {
    format!("filter-bar-filter-{idx}")
}

pub(crate) fn summary_key(idx: usize) -> String {
    format!("filter-bar-filter-{idx}-label")
}

pub(crate) fn remove_key(idx: usize) -> String {
    format!("filter-bar-filter-{idx}-remove")
}

/// The condition shape the bar renders: React's `FilterCondition` with
/// `column`, `operator` (the raw operator string) and an optional value.
/// `None` means `undefined` (omit the suffix); `Some(Value::Null)` is a typed
/// null and shows `NULL`, exactly as React's `formatCell(null)` does.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct FilterCondition {
    pub column: String,
    pub operator: String,
    pub value: Option<Value>,
}

impl FilterCondition {
    pub(crate) fn new(
        column: impl Into<String>,
        operator: impl Into<String>,
        value: Option<Value>,
    ) -> Self {
        Self { column: column.into(), operator: operator.into(), value }
    }
}

/// React `labelFor`: `` `${f.column} ${f.operator}${f.value === undefined ? '' : ` ${formatCell(f.value)}`}` ``.
///
/// Note the bar shows the operator id verbatim (unlike `FilterConditionChip`,
/// which uses the translated `filter.<operator>` label).
pub(crate) fn label_for(filter: &FilterCondition) -> String {
    match &filter.value {
        None => format!("{} {}", filter.column, filter.operator),
        Some(value) => format!("{} {} {}", filter.column, filter.operator, format_cell(value)),
    }
}

/// The filter bar. Callbacks are held behind `Rc` so each chip can clone its
/// own index-capturing handler without the component being generic over a
/// closure type.
pub(crate) struct FilterBar<S> {
    filters: Vec<FilterCondition>,
    loading: bool,
    on_remove: Option<Rc<dyn Fn(&mut S, usize)>>,
    on_clear: Option<Rc<dyn Fn(&mut S)>>,
}

impl<S: 'static> FilterBar<S> {
    pub(crate) fn new(filters: Vec<FilterCondition>) -> Self {
        Self { filters, loading: false, on_remove: None, on_clear: None }
    }

    /// Prevents mutations while the next page loads (React `loading`).
    pub(crate) fn loading(mut self, loading: bool) -> Self {
        self.loading = loading;
        self
    }

    /// Called with the chip's index when its X is pressed (React `onRemove`).
    pub(crate) fn on_remove(mut self, f: impl Fn(&mut S, usize) + 'static) -> Self {
        self.on_remove = Some(Rc::new(f));
        self
    }

    /// Called when the trailing button is pressed (React `onClear`).
    pub(crate) fn on_clear(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_clear = Some(Rc::new(f));
        self
    }

    /// `None` for an empty list, mirroring React's early `return null`.
    pub(crate) fn build(self, dark: bool) -> Option<El> {
        if self.filters.is_empty() {
            return None;
        }
        let k = kit(dark);
        let accent = k.c.text_accent;

        let label = text(t("filter.filter"))
            .key(LABEL_KEY)
            .font(k.ty.small_medium)
            .color(k.c.text_muted)
            .text_case(TextCase::Uppercase)
            .shrink();

        let mut filters = row()
            .key(FILTERS_KEY)
            .min_w(0.0)
            .grow(1.0)
            .flex_wrap()
            .gap(GAP)
            .items_center();
        for (idx, condition) in self.filters.iter().enumerate() {
            filters = filters.child(self.badge(idx, condition, k, accent, dark));
        }

        let clear_label = t("filter.clear");
        let mut clear = text(clear_label)
            .key(CLEAR_KEY)
            .font(k.ty.text_xsmall)
            .color(k.c.text_muted)
            .hover(k.c.text_base)
            .shrink();
        if !self.loading {
            if let Some(cb) = &self.on_clear {
                let cb = Rc::clone(cb);
                clear = clear.on_press(move |s: &mut S| cb(s));
            }
        } else {
            clear = clear.disabled(true).layer_opacity(0.5);
        }

        Some(
            row()
                .key(ROOT_KEY)
                .w_full()
                .flex_wrap()
                .items_center()
                .gap(GAP)
                .px(PAD_X)
                .py(PAD_Y)
                .border_sides([0.0, 0.0, 1.0, 0.0], k.c.border_base)
                .fill(k.c.bg_base)
                .disabled(self.loading)
                .child(label)
                .child(filters)
                .child(clear),
        )
    }

    /// One accent chip: the truncated summary and its remove button.
    fn badge(
        &self,
        idx: usize,
        condition: &FilterCondition,
        k: &'static rune_kit::Kit,
        accent: rune::Color,
        _dark: bool,
    ) -> El {
        let summary = label_for(condition);
        let mut badge = row()
            .key(badge_key(idx))
            .items_center()
            .gap(GAP)
            .radius(BADGE_RADIUS)
            .px(BADGE_PAD_X)
            .py(BADGE_PAD_Y)
            .max_w(320.0)
            .fill(k.alpha(accent, 0.10))
            .border(1.0, k.alpha(accent, 0.20))
            .disabled(self.loading)
            .child(
                text(summary.clone())
                    .key(summary_key(idx))
                    .font(k.ty.small_medium)
                    .color(accent)
                    .help(summary)
                    .truncate(),
            );

        let remove_label = t("filter.remove");
        let mut remove = row()
            .key(remove_key(idx))
            .role("button")
            .size(REMOVE_BOX, REMOVE_BOX)
            .center()
            .label(remove_label)
            .help(remove_label)
            .child(
                icon(&ICON_X, REMOVE_ICON)
                    .key("icon")
                    .color(k.alpha(accent, 0.7))
                    .a11y_hidden(),
            );
        if !self.loading {
            if let Some(cb) = &self.on_remove {
                let press = Rc::clone(cb);
                let key = Rc::clone(cb);
                remove = remove
                    .on_press(move |s: &mut S| press(s, idx))
                    .on_key(move |s: &mut S, ev: &KeyEvent, _tx: &mut Tx| {
                        if ev.key == Key::Enter {
                            key(s, idx);
                            true
                        } else {
                            false
                        }
                    });
            }
        } else {
            remove = remove.disabled(true).layer_opacity(0.5);
        }
        badge = badge.child(remove);
        badge
    }
}
