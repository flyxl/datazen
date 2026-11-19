//! The header strip half of the T069 `ChartWidgetTile` port
//! (`packages/frontend/src/windows/dashboard/ChartWidgetTile.tsx`, the first
//! `<div data-no-drag>` of the tile): the truncating title, the fired-alert
//! glyph, the five-way chart-type switch (React's `CHART_TYPE_ICONS` table,
//! declaration order), the chart/table view switch, and the refresh /
//! history / expand / edit / delete ghost icon buttons. The shared state,
//! view models and body states live in the parent module; the geometry
//! constants and React testid keys it reads are the parent's privates.
//!
//! `data-no-drag` itself is the host grid's drag concern (documented in the
//! parent's deltas); the buttons keep the hover-group tints of the T059
//! toolbar port.

use std::rc::Rc;

use rune::{icon, row, text, Color, El, TextStyle};
use rune_kit::Kit;

use super::{
    ChartWidgetTile, TileViewMode, WidgetRunStatus, BUTTON_SIDE, DELETE_KEY, EDIT_KEY, EXPAND_KEY,
    HEADER_GAP, HEADER_PAD_X, HEADER_PAD_Y, HISTORY_KEY, ICON_SIZE, REFRESH_KEY, TYPE_GAP,
    TYPE_ICON_SIZE, TYPE_PAD, TYPE_TOGGLE_KEY, VIEW_CHART_KEY, VIEW_PX, VIEW_PY, VIEW_TABLE_KEY,
    VIEW_TOGGLE_KEY,
};
use crate::components::chart::chart_canvas::{ChartType, ALL_CHART_TYPES};
use crate::i18n::t;
use crate::icons::{
    ICON_ALERT_TRIANGLE, ICON_CHART_COLUMN, ICON_CHART_LINE, ICON_CHART_PIE, ICON_CHART_SCATTER,
    ICON_HISTORY, ICON_MAXIMIZE_2, ICON_PENCIL, ICON_REFRESH, ICON_TRENDING_UP, ICON_TRASH,
};
use crate::ui::palette;
use crate::ui::spinner::{Spinner, SpinnerSize};

impl<'a, S: 'static> ChartWidgetTile<'a, S> {
    /// The tile's header strip (`flex shrink-0 items-center gap-1 border-b
    /// border-edge px-2 py-1.5`).
    pub(super) fn header(&self, k: &Kit, dark: bool) -> El {
        let mut header = row()
            .key("dashboard-tile-header")
            .w_full()
            .items_center()
            .gap(HEADER_GAP)
            .px(HEADER_PAD_X)
            .py(HEADER_PAD_Y)
            .border_sides([0.0, 0.0, 1.0, 0.0], k.c.border_base)
            // React's `selectable` title: rune text is selectable by default.
            .child(
                text(self.widget.title.clone())
                    .key("dashboard-tile-title")
                    .grow(1.0) // React flex-1
                    .min_w(0.0)
                    .font(TextStyle::new(12.0, 500.0)) // text-xs font-medium
                    .color(k.c.text_base)
                    .truncate(),
            );
        if self.run.is_some_and(|run| run.alert_fired) {
            header = header.child(
                icon(&ICON_ALERT_TRIANGLE, ICON_SIZE)
                    .key("dashboard-tile-alert")
                    .color(palette::alert_amber(dark)) // text-amber-500
                    .help(t("dashboard.alertFired")),
            );
        }
        if self.on_chart_type_change.is_some()
            && matches!(self.widget.view_mode, TileViewMode::Chart)
        {
            header = header.child(self.type_toggle(k));
        }
        if self.on_view_mode_change.is_some() {
            header = header.child(self.view_toggle(k));
        }
        header = header.child(self.refresh_button(k, dark));
        let mut history =
            self.plain_button(k, HISTORY_KEY, t("dashboard.history"), &ICON_HISTORY, None);
        if let Some(on_history) = self.on_history.as_ref() {
            let on_history = Rc::clone(on_history);
            history = history.on_press(move |s: &mut S| on_history(s));
        }
        header = header.child(history);
        if matches!(self.run.map(|run| run.status), Some(WidgetRunStatus::Ok)) {
            header = header.child(self.expand_button(k));
        }
        let mut edit =
            self.plain_button(k, EDIT_KEY, t("dashboard.editWidget"), &ICON_PENCIL, None);
        if let Some(on_edit) = self.on_edit.as_ref() {
            let on_edit = Rc::clone(on_edit);
            edit = edit.on_press(move |s: &mut S| on_edit(s));
        }
        header = header.child(edit);
        if self.on_delete.is_some() {
            let mut delete = self.plain_button(
                k,
                DELETE_KEY,
                t("dashboard.deleteWidget"),
                &ICON_TRASH,
                Some(k.c.danger.fg), // React text-red-400
            );
            if let Some(on_delete) = self.on_delete.as_ref() {
                let on_delete = Rc::clone(on_delete);
                delete = delete.on_press(move |s: &mut S| on_delete(s));
            }
            header = header.child(delete);
        }
        header
    }

    /// The five-way chart-type switch (React `CHART_TYPE_ICONS`, declaration
    /// order; the icons and label keys are the T059 toolbar's table).
    fn type_toggle(&self, k: &Kit) -> El {
        let mut group = row()
            .key(TYPE_TOGGLE_KEY)
            .items_center()
            .gap(TYPE_GAP)
            .padding(TYPE_PAD)
            .radius(k.radius.sm) // React rounded
            .border(1.0, k.c.border_base);
        for chart_type in ALL_CHART_TYPES {
            group = group.child(self.type_button(k, chart_type));
        }
        group
    }

    fn type_button(&self, k: &Kit, chart_type: ChartType) -> El {
        let label = t(chart_type_label_key(chart_type));
        let selected = self
            .widget
            .chart_config
            .as_ref()
            .is_some_and(|config| config.chart_type == chart_type);
        let mut glyph = icon(chart_type_icon(chart_type), TYPE_ICON_SIZE)
            .key(format!("{TYPE_TOGGLE_KEY}-{}-icon", chart_type.as_str()))
            .color(if selected {
                k.c.text_accent // React text-accent
            } else {
                k.c.text_muted // React text-fg-muted
            });
        let mut button = row()
            .key(format!("{TYPE_TOGGLE_KEY}-{}", chart_type.as_str()))
            .role("button")
            .label(label)
            .help(label)
            .items_center()
            .padding(TYPE_PAD) // React p-0.5
            .radius(k.radius.sm) // React rounded
            .focusable(true)
            .focus_ring(2.0, 2.0, k.c.border_focus);
        if selected {
            button = button.fill(k.alpha(k.c.bg_accent, 0.2)); // bg-accent/20
        } else {
            // React `hover:text-fg-secondary`: the toolbar's hover-group tint.
            button = button.hover_group();
            glyph = glyph.group_color(k.c.text_weak);
        }
        button = button.child(glyph);
        if let Some(on_change) = self.on_chart_type_change.as_ref() {
            let on_change = Rc::clone(on_change);
            button = button.on_press(move |s: &mut S| on_change(s, chart_type));
        }
        button
    }

    /// The chart/table view switch.
    fn view_toggle(&self, k: &Kit) -> El {
        let mut group = row()
            .key(VIEW_TOGGLE_KEY)
            .items_center()
            .radius(k.radius.sm)
            .border(1.0, k.c.border_base);
        for (mode, key, label) in [
            (TileViewMode::Chart, VIEW_CHART_KEY, t("dashboard.viewChart")),
            (TileViewMode::Table, VIEW_TABLE_KEY, t("dashboard.viewTable")),
        ] {
            group = group.child(self.view_button(k, mode, key, label));
        }
        group
    }

    fn view_button(&self, k: &Kit, mode: TileViewMode, key: &str, label: &str) -> El {
        let selected = self.widget.view_mode == mode;
        let ink = if selected {
            k.c.text_accent // React text-accent
        } else {
            k.c.text_muted // React text-fg-muted
        };
        let mut label_el = text(label)
            .key(format!("{key}-label"))
            .font(TextStyle::new(10.0, 400.0)) // React text-[10px]
            .color(ink);
        let mut button = row()
            .key(key)
            .role("button")
            .label(label)
            .help(label)
            .items_center()
            .px(VIEW_PX)
            .py(VIEW_PY)
            .focusable(true)
            .focus_ring(2.0, 2.0, k.c.border_focus);
        if selected {
            button = button.fill(k.alpha(k.c.bg_accent, 0.2)); // bg-accent/20
        } else {
            // React `hover:text-fg`: the hover-group tint.
            button = button.hover_group();
            label_el = label_el.group_color(k.c.text_weak);
        }
        if let Some(on_change) = self.on_view_mode_change.as_ref() {
            let on_change = Rc::clone(on_change);
            button = button.on_press(move |s: &mut S| on_change(s, mode));
        }
        button.child(label_el)
    }

    /// The refresh button: React `disabled={busy}` swaps `RefreshCw` for the
    /// shared spinner and makes the control inert.
    fn refresh_button(&self, k: &Kit, dark: bool) -> El {
        let label = t("dashboard.refreshWidget");
        let button = row()
            .key(REFRESH_KEY)
            .role("button")
            .label(label)
            .help(label)
            .size(BUTTON_SIDE, BUTTON_SIDE)
            .center()
            .radius(k.radius.md);
        if self.busy {
            // React's `<Spinner size="md" />` (14px) renders as the 16pt kit
            // tier — the 12pt tier under-fills the 24pt button — and the
            // control is inert (no focus, no press).
            return button.child(
                Spinner::new(format!("{REFRESH_KEY}-spinner"))
                    .size(SpinnerSize::Md)
                    .build(dark),
            );
        }
        let button = button
            .focusable(true)
            .focus_ring(2.0, 2.0, k.c.border_focus)
            .hover_group()
            .hover(k.c.hover)
            .child(
                icon(&ICON_REFRESH, ICON_SIZE)
                    .key(format!("{REFRESH_KEY}-icon"))
                    .color(k.c.text_muted)
                    .group_color(k.c.text_weak), // hover:text-fg-secondary
            );
        if let Some(on_refresh) = self.on_refresh.as_ref() {
            let on_refresh = Rc::clone(on_refresh);
            return button.on_press(move |s: &mut S| on_refresh(s));
        }
        button
    }

    /// A plain 24×24 ghost icon button (history / expand / edit / delete).
    /// `ink` overrides the glyph colour (the delete button's danger ink).
    fn plain_button(
        &self,
        k: &Kit,
        key: &str,
        label: &str,
        glyph: &'static rune::Icon,
        ink: Option<Color>,
    ) -> El {
        let button = row()
            .key(key)
            .role("button")
            .label(label)
            .help(label)
            .size(BUTTON_SIDE, BUTTON_SIDE)
            .center()
            .radius(k.radius.md)
            .focusable(true)
            .focus_ring(2.0, 2.0, k.c.border_focus)
            .hover_group()
            .hover(k.c.hover);
        let mut glyph_el = icon(glyph, ICON_SIZE)
            .key(format!("{key}-icon"))
            .color(ink.unwrap_or(k.c.text_muted));
        if ink.is_none() {
            glyph_el = glyph_el.group_color(k.c.text_weak); // hover:text-fg-secondary
        }
        button.child(glyph_el)
    }

    /// The expand button — React's internal `setExpanded(true)`.
    fn expand_button(&self, k: &Kit) -> El {
        let mut button =
            self.plain_button(k, EXPAND_KEY, t("dashboard.expand"), &ICON_MAXIMIZE_2, None);
        if let Some(on_expand) = self.on_expand.as_ref() {
            let on_expand = Rc::clone(on_expand);
            button = button.on_press(move |s: &mut S| on_expand(s));
        }
        button
    }
}

/// The lucide glyph React's `CHART_TYPE_ICONS` pairs with a chart type (the
/// T059 toolbar's table: `lucide-react` 0.487's aliases).
fn chart_type_icon(chart_type: ChartType) -> &'static rune::Icon {
    match chart_type {
        ChartType::Bar => &ICON_CHART_COLUMN,      // React BarChart3
        ChartType::Line => &ICON_CHART_LINE,       // React LineChart
        ChartType::Pie => &ICON_CHART_PIE,         // React PieChart
        ChartType::Scatter => &ICON_CHART_SCATTER, // React ScatterChart
        ChartType::Area => &ICON_TRENDING_UP,      // React TrendingUp
    }
}

/// The i18n key React's `CHART_TYPE_ICONS` pairs with a chart type.
fn chart_type_label_key(chart_type: ChartType) -> &'static str {
    match chart_type {
        ChartType::Bar => "chart.type.bar",
        ChartType::Line => "chart.type.line",
        ChartType::Pie => "chart.type.pie",
        ChartType::Scatter => "chart.type.scatter",
        ChartType::Area => "chart.type.area",
    }
}
