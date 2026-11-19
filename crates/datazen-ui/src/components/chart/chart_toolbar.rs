//! Port of `packages/frontend/src/components/chart/ChartToolbar.tsx` (Wave 2, T059).
//!
//! The React component is the strip above the chart: a five-way chart-type
//! switch (the React `CHART_TYPES` table, declaration order), three display
//! toggles (legend / grid / values), a split-view toggle that only exists when
//! a handler is wired **and** the config carries more than one Y axis, a
//! spacer, an optional expand button and the export dropdown (PNG / SVG).
//!
//! The shell is the shared `ToolbarShell` look with React's className override
//! (`gap-1 px-2 py-1` merged over the base `gap-2 px-4 py-1`): a 4pt-gap row
//! with 8pt/4pt padding, a bottom edge and the same surface the generic
//! `ui/toolbar.rs` shell paints (`layer_01`, the established mapping for
//! React `bg-surface-alt`).
//!
//! ## Keys
//!
//! | part            | key                          | React anchor |
//! |-----------------|------------------------------|--------------|
//! | root            | `chart-toolbar`              | — |
//! | type group      | `chart-type-group`           | `rounded-md bg-surface p-0.5` div |
//! | type buttons    | `chart-type-{bar,line,pie,scatter,area}` | `data-testid="chart-type-{type}"` |
//! | separators      | `chart-toolbar-sep-types`, `chart-toolbar-sep-split` | `mx-1 h-4 w-px bg-edge` spans |
//! | toggle rows     | `chart-toolbar-{legend,grid,values}` | the three `<label>`s |
//! | split toggle    | `chart-toolbar-split`        | LayoutGrid button |
//! | spacer          | `chart-toolbar-spacer`       | `flex-1` div |
//! | expand button   | `chart-toolbar-expand`       | Maximize2 button |
//! | export trigger  | `chart-export-trigger` (kit wrapper) + inner `chart-export-button` | Download button |
//! | export panel    | `chart-export-menu` (kit)    | `absolute right-0 top-full` div |
//! | export entries  | `chart-export-item{0,1}` (kit) | the PNG / SVG buttons |
//!
//! ## The export seam (honest, not faked)
//!
//! React's `handleExport` serializes `chartRef.current` — a live DOM subtree —
//! through `html-to-image` (`lib/chart/export.ts`) and hands the bytes to the
//! host save dialog. Rune has neither the DOM subtree (the five chart engines
//! are their own migration tasks) nor `html-to-image`; the toolbar therefore
//! exposes `on_export(format)` and stays out of the encoding business. The
//! chart-view track (T104) owns the state this component needs from the app:
//! `export_open` plus the toggle/close/export callbacks — the same
//! immediate-mode split every menu-bearing port takes.
//!
//! ## Deliberate deviations
//!
//! * The three display toggles are the shared `ui::toggle::Checkbox` (16pt kit
//!   control + kit label) instead of React's 12pt `<input>`: one switch
//!   implementation for the whole app, the same consolidation
//!   `DataTableSelectionBar` documented. React's `text-xs text-fg-secondary`
//!   label styling is the kit's own type scale and ink.
//! * The export dropdown is `ui::menu::Menu` (kit) in its standard (bordered,
//!   raised) look rather than the hand-built `absolute` panel: React's own
//!   `border border-edge bg-surface-raised shadow-lg` styling is what the
//!   kit's standard menu paints, and the kit's dismissal layer supplies the
//!   `fixed inset-0` click-away that React hand-mounts.
//! * React's `hover:text-fg-secondary` (a colour-only hover) maps to rune's
//!   `hover_group` + `group_color` on the glyph/text; the buttons that also
//!   carry `hover:bg-surface` take the kit's semantic hover overlay. Note the
//!   kit palette collapses React's two muted inks (`fg-muted` and
//!   `fg-secondary` both derive from `t.text_secondary`), so the port's
//!   hover tint is the token-correct `text_weak` even though it equals the
//!   resting `text_muted` in this theme system.
//! * `shrink-0` has no rune builder (`.shrink()` is allow-shrink, the
//!   inverse); the strip relies on its parent honouring its content height,
//!   and the geometry tests pin the laid-out height.
//! * The icons are the registry bodies for `lucide-react` 0.487's aliases
//!   (`BarChart3`→`chart-column`, `LineChart`→`chart-line`, `PieChart`→
//!   `chart-pie`, `ScatterChart`→`chart-scatter`, `TrendingUp`, `Maximize2`);
//!   `LayoutGrid` reuses `ICON_WORKSPACE` (identical body) and `Download`
//!   reuses `ICON_DOWNLOAD`.
//! * `transition-colors` is a CSS transition; rune paints the states directly
//!   and no animation is ported.

use std::rc::Rc;

use rune::{el, icon, row, text, El};

use super::chart_canvas::{ChartConfig, ChartType, ALL_CHART_TYPES};
use crate::i18n::t;
use crate::icons::{
    ICON_CHART_COLUMN, ICON_CHART_LINE, ICON_CHART_PIE, ICON_CHART_SCATTER, ICON_DOWNLOAD,
    ICON_MAXIMIZE_2, ICON_TRENDING_UP, ICON_WORKSPACE,
};
use rune_kit::Kit;

use crate::ui::kit::kit;
use crate::ui::menu::{Item, Menu, Placement};
use crate::ui::toggle::Checkbox;

/// Root strip, React's `ToolbarShell` + `gap-1 px-2 py-1` override.
pub(crate) const ROOT_KEY: &str = "chart-toolbar";
/// The chart-type switch container (`rounded-md bg-surface p-0.5`).
pub(crate) const TYPE_GROUP_KEY: &str = "chart-type-group";
/// Separator after the type switch (always present).
pub(crate) const SEP_TYPES_KEY: &str = "chart-toolbar-sep-types";
/// Separator before the split toggle (conditional).
pub(crate) const SEP_SPLIT_KEY: &str = "chart-toolbar-sep-split";
/// The split-view button.
pub(crate) const SPLIT_KEY: &str = "chart-toolbar-split";
/// The `flex-1` spacer.
pub(crate) const SPACER_KEY: &str = "chart-toolbar-spacer";
/// The expand button (conditional).
pub(crate) const EXPAND_KEY: &str = "chart-toolbar-expand";
/// The export dropdown's kit menu root (trigger/menu/scrim derive from it).
pub(crate) const EXPORT_KEY: &str = "chart-export";
/// The export trigger's own button (inside the kit's `-trigger` row).
pub(crate) const EXPORT_BUTTON_KEY: &str = "chart-export-button";

// Tailwind geometry, verbatim.
const SHELL_GAP: f32 = 4.0; // gap-1
const SHELL_PAD_X: f32 = 8.0; // px-2
const SHELL_PAD_Y: f32 = 4.0; // py-1
const GROUP_GAP: f32 = 2.0; // gap-0.5
const GROUP_PAD: f32 = 2.0; // p-0.5
const BUTTON_PAD_X: f32 = 8.0; // px-2
const BUTTON_PAD_Y: f32 = 4.0; // py-1
const ICON_SIZE: f32 = 14.0; // h-3.5 w-3.5
const SEP_H: f32 = 16.0; // h-4
const SEP_W: f32 = 1.0; // w-px
const SEP_MX: f32 = 4.0; // mx-1
const TOGGLE_ML: f32 = 8.0; // ml-2
const EXPORT_MENU_W: f32 = 128.0; // w-32
const EXPORT_GUTTER: f32 = 4.0; // mt-1

/// The lucide glyph React's `CHART_TYPES` table pairs with a chart type.
fn chart_type_icon(chart_type: ChartType) -> &'static rune::Icon {
    match chart_type {
        ChartType::Bar => &ICON_CHART_COLUMN, // React `BarChart3`
        ChartType::Line => &ICON_CHART_LINE, // React `LineChart`
        ChartType::Pie => &ICON_CHART_PIE, // React `PieChart`
        ChartType::Scatter => &ICON_CHART_SCATTER, // React `ScatterChart`
        ChartType::Area => &ICON_TRENDING_UP, // React `TrendingUp`
    }
}

/// The i18n key React's `CHART_TYPES` table pairs with a chart type.
fn chart_type_label_key(chart_type: ChartType) -> &'static str {
    match chart_type {
        ChartType::Bar => "chart.type.bar",
        ChartType::Line => "chart.type.line",
        ChartType::Pie => "chart.type.pie",
        ChartType::Scatter => "chart.type.scatter",
        ChartType::Area => "chart.type.area",
    }
}

/// React's `handleExport(format)` payload: the export format the dropdown
/// selected.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ExportFormat {
    Png,
    Svg,
}

impl ExportFormat {
    /// The dropdown entry's own label (React renders the literal "PNG"/"SVG").
    pub(crate) fn menu_label(self) -> &'static str {
        match self {
            Self::Png => "PNG",
            Self::Svg => "SVG",
        }
    }
}

type ChangeFn<S> = Rc<dyn Fn(&mut S, ChartConfig)>;
type ActionFn<S> = Rc<dyn Fn(&mut S)>;
type ExportFn<S> = Rc<dyn Fn(&mut S, ExportFormat)>;

/// The strip above the chart. React's props are required; here the callbacks
/// are `Option` so a probe can render any branch — a production caller wires
/// all of them, and the split/expand parts follow React's own conditional
/// rendering rather than the optional callbacks.
pub(crate) struct ChartToolbar<'a, S: 'static> {
    config: &'a ChartConfig,
    split_view: bool,
    export_open: bool,
    on_change: Option<ChangeFn<S>>,
    on_expand: Option<ActionFn<S>>,
    on_toggle_split: Option<ActionFn<S>>,
    on_export_toggle: Option<ActionFn<S>>,
    on_export_close: Option<ActionFn<S>>,
    on_export: Option<ExportFn<S>>,
}

impl<'a, S: 'static> ChartToolbar<'a, S> {
    pub(crate) fn new(config: &'a ChartConfig) -> Self {
        Self {
            config,
            split_view: false,
            export_open: false,
            on_change: None,
            on_expand: None,
            on_toggle_split: None,
            on_export_toggle: None,
            on_export_close: None,
            on_export: None,
        }
    }

    /// React `splitView`.
    pub(crate) fn split_view(mut self, split: bool) -> Self {
        self.split_view = split;
        self
    }

    /// React `exportOpen` (the dropdown's `useState`, owned by the caller).
    pub(crate) fn export_open(mut self, open: bool) -> Self {
        self.export_open = open;
        self
    }

    /// React `onChange`: receives the full replacement config.
    pub(crate) fn on_change(mut self, f: impl Fn(&mut S, ChartConfig) + 'static) -> Self {
        self.on_change = Some(Rc::new(f));
        self
    }

    /// React `onExpand`.
    pub(crate) fn on_expand(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_expand = Some(Rc::new(f));
        self
    }

    /// React `onToggleSplit`.
    pub(crate) fn on_toggle_split(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_toggle_split = Some(Rc::new(f));
        self
    }

    /// The export trigger's press (React flips `exportOpen`).
    pub(crate) fn on_export_toggle(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_export_toggle = Some(Rc::new(f));
        self
    }

    /// The dropdown closing (React's `setExportOpen(false)` on the click-away
    /// layer; here also Escape and leaf selection, via the kit's dismissal).
    pub(crate) fn on_export_close(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_export_close = Some(Rc::new(f));
        self
    }

    /// The export request: the seam that replaces React's `handleExport`
    /// (`exportChartAsPng`/`exportChartAsSvg` on the DOM chart element).
    pub(crate) fn on_export(mut self, f: impl Fn(&mut S, ExportFormat) + 'static) -> Self {
        self.on_export = Some(Rc::new(f));
        self
    }

    /// Builds the strip. Overlays (the export dropdown) mount into `layer`;
    /// colours and type come from `kit(dark)`.
    pub(crate) fn build(self, dark: bool, layer: &mut Vec<El>) -> El {
        let k = kit(dark);
        let c = &k.c;
        let config = self.config;

        // ---- Chart type selector -----------------------------------------
        let mut group = row()
            .key(TYPE_GROUP_KEY)
            .items_center()
            .gap(GROUP_GAP)
            .padding(GROUP_PAD)
            .radius(k.radius.md) // React `rounded-md`
            .fill(c.layer_02); // React `bg-surface` on the `surface-alt` strip
        for chart_type in ALL_CHART_TYPES {
            group = group.child(self.type_button(k, chart_type));
        }

        let mut bar = row()
            .key(ROOT_KEY)
            .w_full()
            .min_w(0.0)
            .items_center()
            .gap(SHELL_GAP)
            .px(SHELL_PAD_X)
            .py(SHELL_PAD_Y)
            .border_sides([0.0, 0.0, 1.0, 0.0], c.border_base) // `border-b border-edge`
            .fill(c.layer_01) // `bg-surface-alt` (ui/toolbar.rs mapping)
            .child(group)
            // React `mx-1 h-4 w-px bg-edge`.
            .child(separator(SEP_TYPES_KEY, c.border_base));

        // ---- Display toggles ----------------------------------------------
        bar = bar.child(self.display_toggle(
            k,
            "chart-toolbar-legend",
            t("chart.legend"),
            config.show_legend,
            Toggle::Legend,
        ));
        bar = bar.child(
            self.display_toggle(
                k,
                "chart-toolbar-grid",
                t("chart.grid"),
                config.show_grid,
                Toggle::Grid,
            )
            .ml(TOGGLE_ML),
        );
        bar = bar.child(
            self.display_toggle(
                k,
                "chart-toolbar-values",
                t("chart.values"),
                config.show_values,
                Toggle::Values,
            )
            .ml(TOGGLE_ML),
        );

        // ---- Split view toggle (both gates, as in React) ------------------
        if self.on_toggle_split.is_some() && config.y_axes.len() > 1 {
            bar = bar
                .child(separator(SEP_SPLIT_KEY, c.border_base))
                .child(self.split_button(k));
        }

        bar = bar.child(el().key(SPACER_KEY).grow(1.0).min_w(0.0)); // React `flex-1`

        // ---- Expand button -------------------------------------------------
        if let Some(on_expand) = self.on_expand.as_ref() {
            let on_expand = Rc::clone(on_expand);
            bar = bar.child(
                icon_button(k, EXPAND_KEY, t("chart.expand"), &ICON_MAXIMIZE_2)
                    .on_press(move |s: &mut S| on_expand(s)),
            );
        }

        // ---- Export dropdown -----------------------------------------------
        let mut menu = Menu::<S>::new(EXPORT_KEY)
            .trigger(self.export_trigger(k))
            .open(self.export_open)
            .placement(Placement::BottomEnd) // React `absolute right-0 top-full`
            .standard(true) // the bordered, raised panel React styles
            .min_width(EXPORT_MENU_W) // React `w-32`
            .gutter(EXPORT_GUTTER); // React `mt-1`
        for format in [ExportFormat::Png, ExportFormat::Svg] {
            let on_export = self.on_export.clone();
            menu = menu.item(
                Item::new(format.menu_label()).on_select(move |s: &mut S| {
                    if let Some(on_export) = &on_export {
                        on_export(s, format);
                    }
                }),
            );
        }
        if let Some(on_close) = self.on_export_close.as_ref() {
            let on_close = Rc::clone(on_close);
            menu = menu.on_close(move |s: &mut S| on_close(s));
        }
        bar.child(menu.build(k, layer))
    }

    /// One chart-type button: React's `data-testid="chart-type-{type}"`,
    /// `title`/`aria-label` from the label key, and the accent chip styling
    /// when selected.
    fn type_button(&self, k: &Kit, chart_type: ChartType) -> El {
        let c = &k.c;
        let label = t(chart_type_label_key(chart_type));
        let selected = self.config.chart_type == chart_type;

        let mut glyph = icon(chart_type_icon(chart_type), ICON_SIZE)
            .key(format!("chart-type-{}-icon", chart_type.as_str()))
            .color(if selected {
                c.text_accent // React `text-accent`
            } else {
                c.text_muted // React `text-fg-muted`
            });
        let mut button = row()
            .key(format!("chart-type-{}", chart_type.as_str()))
            .role("button")
            .label(label)
            .help(label)
            .items_center()
            .px(BUTTON_PAD_X)
            .py(BUTTON_PAD_Y)
            .radius(k.radius.sm) // React `rounded`
            .focusable(true)
            .focus_ring(2.0, 2.0, c.border_focus);

        if selected {
            // React `bg-accent/20` (the execution-result-panel mapping).
            button = button.fill(k.alpha(c.bg_accent, 0.2));
        } else {
            // React `hover:text-fg-secondary`: track the hover for the glyph's
            // group tint without painting a fill (React changes colour only).
            button = button.hover_group();
            glyph = glyph.group_color(c.text_weak);
        }
        button = button.child(glyph);

        if let Some(on_change) = self.on_change.as_ref() {
            let on_change = Rc::clone(on_change);
            let mut new_config = self.config.clone();
            new_config.chart_type = chart_type;
            // The press hands the caller a full replacement config; each press
            // clones it so the closure stays `Fn` (React's `onChange({...})`
            // builds a fresh object every click too).
            button = button
                .on_press(move |s: &mut S| on_change(s, new_config.clone()));
        }
        button
    }

    /// One display toggle: React's `<label><input …/>{t(key)}</label>` as the
    /// shared kit checkbox.
    fn display_toggle(&self, k: &Kit, key: &str, label: &str, checked: bool, which: Toggle) -> El {
        let mut checkbox = Checkbox::new(key, checked).label(label);
        if let Some(on_change) = self.on_change.as_ref() {
            let on_change = Rc::clone(on_change);
            let base = self.config.clone();
            checkbox = checkbox.on_toggle(move |s: &mut S| {
                // Each press flips the pristine base, exactly as React's
                // `onChange({...config, flag: e.target.checked})` builds the
                // replacement from the config it rendered.
                let mut new_config = base.clone();
                which.apply(&mut new_config);
                on_change(s, new_config);
            });
        }
        checkbox.build(k)
    }

    /// The split-view button (`LayoutGrid`, accent chip when split).
    fn split_button(&self, k: &Kit) -> El {
        let c = &k.c;
        let label = t("chart.splitView");
        let selected = self.split_view;

        let mut glyph = icon(&ICON_WORKSPACE, ICON_SIZE) // React `LayoutGrid`
            .key(format!("{SPLIT_KEY}-icon"))
            .color(if selected {
                c.text_accent
            } else {
                c.text_muted // React `text-fg-muted`
            });
        let mut button = row()
            .key(SPLIT_KEY)
            .role("button")
            .label(label)
            .help(label)
            .items_center()
            .gap(4.0) // React `gap-1` on the button's flex
            .px(BUTTON_PAD_X)
            .py(BUTTON_PAD_Y)
            .radius(k.radius.sm) // React `rounded`
            .focusable(true)
            .focus_ring(2.0, 2.0, c.border_focus);

        if selected {
            button = button.fill(k.alpha(c.bg_accent, 0.2)); // `bg-accent/20`
        } else {
            // React: `text-fg-muted hover:text-fg-secondary hover:bg-surface`.
            button = button.hover_group().hover(c.hover);
            glyph = glyph.group_color(c.text_weak);
        }
        button = button.child(glyph);

        if let Some(on_toggle_split) = self.on_toggle_split.as_ref() {
            let on_toggle_split = Rc::clone(on_toggle_split);
            button = button.on_press(move |s: &mut S| on_toggle_split(s));
        }
        button
    }

    /// The export trigger: React's Download button (icon + `chart.export`).
    fn export_trigger(&self, k: &Kit) -> El {
        let c = &k.c;
        let label = t("chart.export");
        let mut trigger = row()
            .key(EXPORT_BUTTON_KEY)
            .role("button")
            .label(label)
            .help(label)
            .items_center()
            .gap(4.0) // React `gap-1`
            .px(BUTTON_PAD_X)
            .py(BUTTON_PAD_Y)
            .radius(k.radius.sm) // React `rounded`
            .focusable(true)
            .focus_ring(2.0, 2.0, c.border_focus)
            .hover_group()
            .hover(c.hover) // React `hover:bg-surface`
            .child(
                icon(&ICON_DOWNLOAD, ICON_SIZE)
                    .key(format!("{EXPORT_BUTTON_KEY}-icon"))
                    .color(c.text_muted) // React `text-fg-muted`
                    .group_color(c.text_weak), // `hover:text-fg-secondary`
            )
            .child(
                text(label)
                    .key(format!("{EXPORT_BUTTON_KEY}-label"))
                    .font(k.ty.text_xsmall) // React `text-xs` = 12px
                    .color(c.text_muted)
                    .group_color(c.text_weak), // `hover:text-fg-secondary`
            );
        if let Some(on_export_toggle) = self.on_export_toggle.as_ref() {
            let on_export_toggle = Rc::clone(on_export_toggle);
            trigger = trigger.on_press(move |s: &mut S| on_export_toggle(s));
        }
        trigger
    }
}

/// Which display flag a toggle flips.
#[derive(Clone, Copy)]
enum Toggle {
    Legend,
    Grid,
    Values,
}

impl Toggle {
    fn apply(self, config: &mut ChartConfig) {
        match self {
            Self::Legend => config.show_legend = !config.show_legend,
            Self::Grid => config.show_grid = !config.show_grid,
            Self::Values => config.show_values = !config.show_values,
        }
    }
}

/// React's `mx-1 h-4 w-px bg-edge` span: a 16pt hairline with 4pt margins.
fn separator(key: &str, color: rune::Color) -> El {
    el()
        .key(key)
        .ml(SEP_MX)
        .mr(SEP_MX)
        .h(SEP_H)
        .w(SEP_W)
        .fill(color)
        .a11y_hidden()
}

/// A hand-built icon-only toolbar button (the expand button's shape): muted
/// glyph, hover tint, focus ring and activation.
fn icon_button(k: &Kit, key: &str, label: &str, glyph: &'static rune::Icon) -> El {
    let c = &k.c;
    row()
        .key(key)
        .role("button")
        .label(label)
        .help(label)
        .items_center()
        .px(BUTTON_PAD_X)
        .py(BUTTON_PAD_Y)
        .radius(k.radius.sm) // React `rounded`
        .focusable(true)
        .focus_ring(2.0, 2.0, c.border_focus)
        .hover_group()
        .hover(c.hover) // React `hover:bg-surface`
        .child(
            icon(glyph, ICON_SIZE)
                .key(format!("{key}-icon"))
                .color(c.text_muted) // React `text-fg-muted`
                .group_color(c.text_weak), // `hover:text-fg-secondary`
        )
}
