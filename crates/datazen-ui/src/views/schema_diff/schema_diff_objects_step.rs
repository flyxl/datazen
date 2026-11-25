//! Port of `packages/frontend/src/windows/schema-diff/SchemaDiffObjectsStep.tsx`
//! (T149).
//!
//! The wizard's object-selection step: a centered 2xl accent spinner while
//! discovery runs, an empty-state card when the paired inventory is empty,
//! otherwise a "selected n / total" header with Select-all / Deselect-all
//! link buttons above a bordered, divided list of one checkbox row per
//! table pick — the mono name plus the origin badge (`Target only` in
//! danger, else the muted `Source only` / `Source & target`).
//!
//! ## What maps where
//!
//! - Props become builder fields: `loading` / `tables` are inputs;
//!   `onToggle(name)` / `onSelectAll` / `onSelectNone` become callbacks
//!   (React's closures carry the row name in, the port's `on_toggle` does).
//! - `SchemaDiffTablePick` is the seam module's [`SchemaDiffTablePickVm`]
//!   (T351's port of `schemaDiffTableNames.ts`); `origin` is its
//!   [`TablePickOriginVm`] with the React spellings.
//! - `Spinner size="2xl" tone="accent"` → [`Spinner`] `Xxl` (the new h-6
//!   tier) + `Accent` tone.
//! - `t('schemaDiff.objectsSelected', { count, total })`: the i18n table
//!   keeps the `{count}` / `{total}` placeholders; the caller formats them,
//!   exactly as React's `t()` does.
//! - The link buttons (`text-xs text-accent hover:underline`) follow the
//!   batch-export select-all convention: role=button pressable accent text;
//!   the hover underline itself is not reproduced (the accent ink carries
//!   the affordance) — a documented delta.
//! - `divide-y divide-edge` → a top rule on every list item except the
//!   first (the same hairline between rows).
//! - The origin badge: `text-[10px] font-medium uppercase`, danger ink for
//!   `target-only`, muted otherwise (the target-only colour is the point —
//!   those rows deploy as DROP TABLE).
//!
//! ## Keys
//!
//! React `data-testid` values stay verbatim where they are unique: the root
//! `schema-diff-objects-panel` and each origin badge
//! `schema-diff-table-origin-{name}`. React repeats `schema-diff-table-row`
//! across rows (a testid, not a key) and keys rows by `name`; the port's
//! row key is therefore the derived `schema-diff-table-row-{name}` (unique,
//! and addressable per name). `data-table-name` / `data-table-origin` have
//! no Rune attributes — the name text is keyed
//! `schema-diff-table-name-{name}` and the origin badge carries both.
//!
//! Known deltas (documented, not silent): pixel classes become points
//! (×0.75 — `py-12` 36pt, `py-8` 24pt, `px-3`/`py-2` 9/6pt, `gap-2` 6pt,
//! `space-y-3` 9pt); the hover underline is dropped (above).

use std::rc::Rc;

use rune::{col, row, text, El, TextStyle};

use crate::i18n::t;
use crate::ui::kit::kit;
use crate::ui::spinner::{Spinner, SpinnerSize, SpinnerTone};
use crate::ui::toggle::Checkbox;
use crate::views::schema_diff::schema_diff_table_names::{
    SchemaDiffTablePickVm, TablePickOriginVm,
};

type ToggleFn<S> = Rc<dyn Fn(&mut S) + 'static>;
type NameFn<S> = Rc<dyn Fn(&mut S, String) + 'static>;

/// The objects step builder, generic over the owning state `S`.
pub struct SchemaDiffObjectsStep<'a, S: 'static> {
    loading: bool,
    tables: &'a [SchemaDiffTablePickVm],
    on_toggle: Option<NameFn<S>>,
    on_select_all: Option<ToggleFn<S>>,
    on_select_none: Option<ToggleFn<S>>,
}

impl<'a, S: 'static> SchemaDiffObjectsStep<'a, S> {
    /// React's `loading` / `tables` props.
    pub fn new(loading: bool, tables: &'a [SchemaDiffTablePickVm]) -> Self {
        Self {
            loading,
            tables,
            on_toggle: None,
            on_select_all: None,
            on_select_none: None,
        }
    }

    /// React's `onToggle(name)`.
    pub fn on_toggle(mut self, f: impl Fn(&mut S, String) + 'static) -> Self {
        self.on_toggle = Some(Rc::new(f));
        self
    }

    /// React's `onSelectAll`.
    pub fn on_select_all(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_select_all = Some(Rc::new(f));
        self
    }

    /// React's `onSelectNone`.
    pub fn on_select_none(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_select_none = Some(Rc::new(f));
        self
    }

    /// Paint the step (React's three branches).
    pub fn build(self, dark: bool) -> El {
        let k = kit(dark);

        // --- The loading branch: a centered 2xl accent spinner. ---
        if self.loading {
            return row()
                .key("schema-diff-objects-panel")
                .w_full()
                .py(36.0) // React py-12
                .justify_center()
                .child(
                    Spinner::new("schema-diff-objects-spinner")
                        .size(SpinnerSize::Xxl)
                        .tone(SpinnerTone::Accent)
                        .build(dark),
                );
        }

        // --- The empty branch: the bordered no-tables card. ---
        if self.tables.is_empty() {
            return col()
                .key("schema-diff-objects-panel")
                .w_full()
                .px(12.0) // React px-4
                .py(24.0) // React py-8
                .center_y() // text-center on the cross axis
                .radius(4.5) // React rounded-lg
                .border(0.75, k.c.border_base)
                .fill(k.c.layer_01) // bg-surface-alt
                .child(
                    text(t("schemaDiff.noTablesFound").to_owned())
                        .key("schema-diff-objects-empty")
                        .font(type_sm())
                        .color(k.c.text_muted),
                );
        }

        // --- The list branch. ---
        let enabled_count = self.tables.iter().filter(|row| row.enabled).count();
        let mut panel = col()
            .key("schema-diff-objects-panel")
            .w_full()
            .gap(9.0) // React space-y-3
            .child(self.header_row(dark, enabled_count));

        let mut list = col()
            .key("schema-diff-objects-list")
            .w_full()
            .radius(4.5) // React rounded-lg
            .border(0.75, k.c.border_base)
            .fill(k.c.layer_01); // bg-surface-alt
        for (index, row) in self.tables.iter().enumerate() {
            list = list.child(self.table_row(index, row, dark));
        }
        panel = panel.child(list);
        panel
    }

    /// The "selected n / total" line with the two link buttons.
    fn header_row(&self, dark: bool, enabled_count: usize) -> El {
        let k = kit(dark);
        let mut header = row()
            .key("schema-diff-objects-header")
            .w_full()
            .center_y()
            .justify_between()
            .gap(6.0) // React gap-2
            .child(
                text(
                    t("schemaDiff.objectsSelected")
                        .replace("{count}", &enabled_count.to_string())
                        .replace("{total}", &self.tables.len().to_string()),
                )
                .key("schema-diff-objects-count")
                .font(type_sm())
                .color(k.c.text_muted),
            );

        let mut buttons = row()
            .key("schema-diff-objects-header-buttons")
            .gap(6.0); // React gap-2
        let mut select_all = row()
            .key("schema-diff-objects-select-all")
            .role("button")
            .child(
                text(t("common.selectAll").to_owned())
                    .key("schema-diff-objects-select-all-label")
                    .font(type_xs())
                    .color(k.c.text_accent),
            );
        if let Some(on_select_all) = &self.on_select_all {
            let on_select_all = Rc::clone(on_select_all);
            select_all = select_all.on_press(move |s: &mut S| on_select_all(s));
        }
        let mut select_none = row()
            .key("schema-diff-objects-select-none")
            .role("button")
            .child(
                text(t("common.deselectAll").to_owned())
                    .key("schema-diff-objects-select-none-label")
                    .font(type_xs())
                    .color(k.c.text_accent),
            );
        if let Some(on_select_none) = &self.on_select_none {
            let on_select_none = Rc::clone(on_select_none);
            select_none = select_none.on_press(move |s: &mut S| on_select_none(s));
        }
        buttons = buttons.child(select_all).child(select_none);
        header.child(buttons)
    }

    /// One table pick row: checkbox + mono name + origin badge.
    fn table_row(&self, index: usize, pick: &SchemaDiffTablePickVm, dark: bool) -> El {
        let k = kit(dark);
        let mut row_el = row()
            .key(format!("schema-diff-table-row-{}", pick.name))
            .w_full()
            .center_y()
            .gap(6.0) // React gap-2
            .pl(9.0)
            .pr(9.0)
            .pt(6.0)
            .pb(6.0) // React px-3 py-2
            .font(type_sm()); // React text-sm
        // divide-y: the hairline sits between rows, not around the list.
        if index > 0 {
            row_el = row_el.border_sides([1.0, 0.0, 0.0, 0.0], k.c.border_base);
        }

        let mut checkbox = Checkbox::new(format!("schema-diff-table-check-{}", pick.name), pick.enabled);
        if let Some(on_toggle) = &self.on_toggle {
            let on_toggle = Rc::clone(on_toggle);
            let name = pick.name.clone();
            checkbox = checkbox.on_toggle(move |s: &mut S| on_toggle(s, name.clone()));
        }
        row_el = row_el
            .child(checkbox.build(k))
            .child(
                text(pick.name.clone())
                    .key(format!("schema-diff-table-name-{}", pick.name))
                    .font(type_xs().mono()) // React text-xs font-mono
                    .color(k.c.text_base)
                    .grow(1.0)
                    .min_w(0.0)
                    .truncate(),
            )
            .child(origin_badge(&pick.name, pick.origin, dark));
        row_el
    }
}

/// The origin badge: danger ink for target-only rows, muted otherwise.
fn origin_badge(name: &str, origin: TablePickOriginVm, dark: bool) -> El {
    let k = kit(dark);
    let (key, color) = match origin {
        TablePickOriginVm::TargetOnly => ("schemaDiff.targetOnly", k.c.danger.fg),
        TablePickOriginVm::SourceOnly => ("schemaDiff.sourceOnly", k.c.text_muted),
        TablePickOriginVm::Both => ("schemaDiff.sourceAndTarget", k.c.text_muted),
    };
    text(t(key).to_uppercase())
        .key(format!("schema-diff-table-origin-{name}"))
        .font(badge_type()) // React text-[10px] font-medium uppercase
        .color(color)
}

/// React `text-[10px] font-medium` (10px → 7.5pt, medium 500).
fn badge_type() -> TextStyle {
    TextStyle::new(7.5, 500.0)
}

/// React `text-sm` (14px → 10.5pt).
fn type_sm() -> TextStyle {
    TextStyle::new(10.5, 400.0)
}

/// React `text-xs` (12px → 9pt).
fn type_xs() -> TextStyle {
    TextStyle::new(9.0, 400.0)
}
