//! Port of `packages/frontend/src/windows/schema-diff/SchemaDiffTableListPanel.tsx`
//! (T269).
//!
//! The schema-diff wizard's table list rail: a fixed header ("Tables"),
//! then one full-width pressable row per selected table — the table name in
//! mono, a `Changed` badge when the table has schema differences, an
//! "Identical" hint when it does not — with the selected row highlighted
//! and an empty-state hint when no table is selected for the compare.
//!
//! ## What maps where
//!
//! - The props become builder fields: `tables` / `selectedTable` /
//!   `tableHasDiff` are inputs, `onSelect(table)` becomes the
//!   [`SchemaDiffTableListPanel::on_select`] callback (the press carries the
//!   row's name into the host state, exactly React's closure).
//! - `className` / `style` composition (`flex-none` + `width`) becomes
//!   [`SchemaDiffTableListPanel::width`]: the parent (the compare step,
//!   T075) pins the rail's width; `flex-none` is the default here because
//!   the rail never grows.
//! - `bg-surface` → `k.c.surface_base`; `bg-surface-alt` (active row and
//!   hover) → `k.c.layer_01` / `k.c.hover` (the workspace-navigator
//!   convention); `border-edge` → `k.c.border_base` via `border_sides`
//!   (the root's right rule, the header's and each row's bottom rule).
//! - `hasDiff === true` / `hasDiff === false` are EXACT comparisons in
//!   React: an absent entry (`tableHasDiff?.[name]` → `undefined`) renders
//!   neither badge nor hint. The port keeps the three states apart through
//!   `Option<&HashMap>` → `map.get(name)` → `Option<&bool>`.
//! - The `Changed` badge: `bg-warning/15` → `k.alpha(k.c.warning.fg, 0.15)`
//!   (the connection-settings wash convention), `text-warning` →
//!   `k.c.warning.fg`, uppercase, `rounded` → a 3pt radius.
//! - Row presses: `.role("button")` + `.on_press` (the batch-export row
//!   convention); the hover wash applies only to the unselected rows, like
//!   React's `active && 'bg-surface-alt'` toggle.
//!
//! ## Keys
//!
//! React `data-testid` values stay verbatim: the root
//! `schema-diff-table-list` and each row `schema-diff-table-row-{name}`
//! (the name is the row's React `key` too). Derived keys (no React
//! counterpart, for observation): the header's label text
//! `schema-diff-table-list-header-label` (-header is the container), the
//! row list `schema-diff-table-list-rows`, and the empty hint's text
//! `schema-diff-table-list-empty-hint` (-empty is the container).
//!
//! Known deltas (documented, not silent): pixel classes become points
//! (×0.75 — `text-[11px]` 8.25pt, `text-xs` 9pt, `text-[10px]` 7.5pt,
//! `px-3`/`py-2` 9/6pt); the row `type="button"` semantics become the
//! `button` role (Return activation follows the shell's focus rules).

use std::collections::HashMap;
use std::rc::Rc;

use rune::{col, row, text, El, ScrollSpec, TextStyle};

use crate::i18n::t;
use crate::ui::kit::kit;

type SelectFn<S> = Rc<dyn Fn(&mut S, String) + 'static>;

/// The table list rail builder, generic over the owning state `S` (the
/// ported-dialog pattern).
pub(crate) struct SchemaDiffTableListPanel<'a, S: 'static> {
    tables: &'a [String],
    selected_table: Option<&'a str>,
    table_has_diff: Option<&'a HashMap<String, bool>>,
    width: Option<f32>,
    on_select: Option<SelectFn<S>>,
}

impl<'a, S: 'static> SchemaDiffTableListPanel<'a, S> {
    /// React's `tables` / `selectedTable` props.
    pub(crate) fn new(tables: &'a [String], selected_table: Option<&'a str>) -> Self {
        Self {
            tables,
            selected_table,
            table_has_diff: None,
            width: None,
            on_select: None,
        }
    }

    /// React's optional `tableHasDiff` map (`undefined` renders neither
    /// badge nor hint).
    pub(crate) fn table_has_diff(mut self, map: Option<&'a HashMap<String, bool>>) -> Self {
        self.table_has_diff = map;
        self
    }

    /// React's `className="flex-none" style={{ width }}` composition from
    /// the compare step: the rail's fixed width in points.
    pub(crate) fn width(mut self, width: f32) -> Self {
        self.width = Some(width);
        self
    }

    /// React's `onSelect(table)`.
    pub(crate) fn on_select(mut self, f: impl Fn(&mut S, String) + 'static) -> Self {
        self.on_select = Some(Rc::new(f));
        self
    }

    /// Paint the rail (React's bordered column).
    pub(crate) fn build(self, dark: bool) -> El {
        let k = kit(dark);
        // React `flex-none` is the default here (no shrink along the rail).
        let mut panel = col()
            .key("schema-diff-table-list")
            .min_w(0.0)
            .min_h(0.0)
            .fill(k.c.surface_base) // bg-surface
            .border_sides([0.0, 1.0, 0.0, 0.0], k.c.border_base); // border-r border-edge
        if let Some(width) = self.width {
            panel = panel.w(width);
        }

        // --- The fixed header ("Tables"), bottom-ruled. ---
        panel = panel.child(
            col()
                .key("schema-diff-table-list-header")
                .pl(9.0)
                .pr(9.0)
                .pt(6.0)
                .pb(6.0) // React px-3 py-2
                .border_sides([0.0, 0.0, 1.0, 0.0], k.c.border_base)
                .child(
                    text(t("schemaDiff.tables").to_uppercase())
                        .key("schema-diff-table-list-header-label")
                        .font(header_type())
                        .color(k.c.text_muted),
                ),
        );

        // --- The scrolling row list (or the empty hint). ---
        let mut list = col()
            .key("schema-diff-table-list-rows")
            .min_h(0.0)
            .grow(1.0)
            .scroll(ScrollSpec { stick_bottom: false });
        if self.tables.is_empty() {
            list = list.child(
                col()
                    .key("schema-diff-table-list-empty")
                    .w_full()
                    .padding(12.0) // React p-4
                    .center_y() // horizontal centring on a column's cross axis
                    .child(
                        text(t("schemaDiff.tableRequired").to_owned())
                            .key("schema-diff-table-list-empty-hint")
                            .font(k.ty.text_xsmall)
                            .color(k.c.text_muted),
                    ),
            );
        } else {
            for name in self.tables {
                list = list.child(self.table_row(name, dark));
            }
        }
        panel.child(list)
    }

    /// One table row: the pressable name + the diff badge / identical hint.
    fn table_row(&self, name: &str, dark: bool) -> El {
        let k = kit(dark);
        let active = self.selected_table == Some(name);
        // `tableHasDiff?.[name]` — absent map or absent entry is `undefined`
        // (no badge), Some(true) is the badge, Some(false) the hint.
        let has_diff = self.table_has_diff.and_then(|map| map.get(name)).copied();

        let mut row_el = row()
            .key(format!("schema-diff-table-row-{name}"))
            .role("button")
            .w_full()
            .center_y()
            .gap(6.0) // React gap-2
            .pl(9.0)
            .pr(9.0)
            .pt(6.0)
            .pb(6.0) // React px-3 py-2
            .border_sides([0.0, 0.0, 1.0, 0.0], k.c.border_base)
            .fill(if active {
                // React `active && 'bg-surface-alt'`.
                k.c.layer_01
            } else {
                rune::Color::TRANSPARENT
            });
        if !active {
            // React `hover:bg-surface-alt` — only the unselected rows wash.
            row_el = row_el.hover(k.c.hover);
        }
        if let Some(on_select) = &self.on_select {
            let on_select = Rc::clone(on_select);
            let name = name.to_owned();
            row_el = row_el.on_press(move |s: &mut S| on_select(s, name.clone()));
        }

        row_el = row_el.child(
            text(name.to_owned())
                .font(k.ty.text_xsmall.mono()) // text-xs font-mono
                .color(k.c.text_base)
                .grow(1.0)
                .min_w(0.0)
                .truncate(),
        );
        if has_diff == Some(true) {
            row_el = row_el.child(
                text(t("schemaDiff.colChanged").to_uppercase())
                    .font(badge_type()) // text-[10px] font-medium
                    .color(k.c.warning.fg)
                    .pl(4.5)
                    .pr(4.5)
                    .pt(1.5)
                    .pb(1.5) // React px-1.5 py-0.5
                    .radius(3.0) // React rounded
                    .fill(k.alpha(k.c.warning.fg, 0.15)), // bg-warning/15
            );
        } else if has_diff == Some(false) {
            row_el = row_el.child(
                text(t("schemaDiff.identical").to_owned())
                    .font(badge_type())
                    .color(k.c.text_muted),
            );
        }
        row_el
    }
}

/// React `text-[11px] font-semibold uppercase tracking-wider`
/// (11px → 8.25pt, semibold 600).
fn header_type() -> TextStyle {
    TextStyle::new(8.25, 600.0)
}

/// React `text-[10px] font-medium` (10px → 7.5pt, medium 500).
fn badge_type() -> TextStyle {
    TextStyle::new(7.5, 500.0)
}
