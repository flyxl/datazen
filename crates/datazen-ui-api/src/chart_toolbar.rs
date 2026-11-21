#[cfg(test)]
mod tests;

use crate::{ChartConfig, ChartType, Translate};

pub const KEY_LEGEND: &str = "chart.legend";
pub const KEY_GRID: &str = "chart.grid";
pub const KEY_VALUES: &str = "chart.values";
pub const KEY_SPLIT_VIEW: &str = "chart.splitView";
pub const KEY_EXPAND: &str = "chart.expand";
pub const KEY_EXPORT: &str = "chart.export";
pub const ACTIVE_CLASS: &str = "bg-accent/20 text-accent";
pub const INACTIVE_CLASS: &str = "text-fg-muted hover:text-fg-secondary";
pub const TOOLBAR_SHELL_CLASS: &str = "gap-1 px-2 py-1";

pub const CHART_TYPES: [ChartType; 5] = [
    ChartType::Bar,
    ChartType::Line,
    ChartType::Pie,
    ChartType::Scatter,
    ChartType::Area,
];

pub fn chart_type_label_key(chart_type: ChartType) -> &'static str {
    match chart_type {
        ChartType::Bar => "chart.type.bar",
        ChartType::Line => "chart.type.line",
        ChartType::Pie => "chart.type.pie",
        ChartType::Scatter => "chart.type.scatter",
        ChartType::Area => "chart.type.area",
    }
}

pub fn chart_type_test_id(chart_type: ChartType) -> &'static str {
    match chart_type {
        ChartType::Bar => "chart-type-bar",
        ChartType::Line => "chart-type-line",
        ChartType::Pie => "chart-type-pie",
        ChartType::Scatter => "chart-type-scatter",
        ChartType::Area => "chart-type-area",
    }
}

pub fn chart_type_class(active: bool) -> &'static str {
    if active {
        ACTIVE_CLASS
    } else {
        INACTIVE_CLASS
    }
}

pub fn split_class(active: bool) -> &'static str {
    if active {
        ACTIVE_CLASS
    } else {
        "text-fg-muted hover:text-fg-secondary hover:bg-surface"
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChartExportFormat {
    Png,
    Svg,
}

impl ChartExportFormat {
    pub fn as_str(self) -> &'static str {
        match self {
            ChartExportFormat::Png => "png",
            ChartExportFormat::Svg => "svg",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ToolbarControl {
    ChartType(ChartType),
    ShowLegend(bool),
    ShowGrid(bool),
    ShowValues(bool),
    Split,
    Expand,
    ExportButton,
    ExportOverlay,
    ExportPng,
    ExportSvg,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ChartToolbarState {
    pub export_open: bool,
}

impl Default for ChartToolbarState {
    fn default() -> Self {
        Self { export_open: false }
    }
}

pub struct ToolbarContext<'a> {
    pub config: &'a ChartConfig,
    pub on_toggle_split: bool,
    pub on_expand: bool,
    pub chart_available: bool,
    pub now_ms: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ToolbarEffect {
    ToggleSplit,
    Expand,
    Export {
        format: ChartExportFormat,
        filename: String,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub struct ToolbarOutcome {
    pub state: ChartToolbarState,
    pub config: Option<ChartConfig>,
    pub effects: Vec<ToolbarEffect>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ToolbarLabels {
    pub legend: String,
    pub grid: String,
    pub values: String,
    pub split_view: String,
    pub expand: String,
    pub export_label: String,
    pub chart_types: Vec<(ChartType, String)>,
}

pub fn resolve_toolbar_labels(t: &dyn Translate) -> ToolbarLabels {
    ToolbarLabels {
        legend: t.translate(KEY_LEGEND, &[]),
        grid: t.translate(KEY_GRID, &[]),
        values: t.translate(KEY_VALUES, &[]),
        split_view: t.translate(KEY_SPLIT_VIEW, &[]),
        expand: t.translate(KEY_EXPAND, &[]),
        export_label: t.translate(KEY_EXPORT, &[]),
        chart_types: CHART_TYPES
            .iter()
            .map(|chart_type| (*chart_type, t.translate(chart_type_label_key(*chart_type), &[])))
            .collect(),
    }
}

pub fn split_available(ctx: &ToolbarContext<'_>) -> bool {
    ctx.on_toggle_split && ctx.config.y_axes.len() > 1
}

pub fn visible_controls(
    ctx: &ToolbarContext<'_>,
    state: &ChartToolbarState,
) -> Vec<ToolbarControl> {
    let mut controls: Vec<ToolbarControl> = CHART_TYPES
        .iter()
        .map(|chart_type| ToolbarControl::ChartType(*chart_type))
        .collect();
    controls.extend([
        ToolbarControl::ShowLegend(ctx.config.show_legend),
        ToolbarControl::ShowGrid(ctx.config.show_grid),
        ToolbarControl::ShowValues(ctx.config.show_values),
    ]);
    if split_available(ctx) {
        controls.push(ToolbarControl::Split);
    }
    if ctx.on_expand {
        controls.push(ToolbarControl::Expand);
    }
    controls.push(ToolbarControl::ExportButton);
    if state.export_open {
        controls.extend([
            ToolbarControl::ExportOverlay,
            ToolbarControl::ExportPng,
            ToolbarControl::ExportSvg,
        ]);
    }
    controls
}

pub fn reduce_toolbar(
    state: &ChartToolbarState,
    control: ToolbarControl,
    ctx: &ToolbarContext<'_>,
) -> ToolbarOutcome {
    let mut next = *state;
    let mut config = None;
    let mut effects = Vec::new();
    match control {
        ToolbarControl::ChartType(chart_type) => {
            let mut updated = ctx.config.clone();
            updated.chart_type = chart_type;
            config = Some(updated);
        }
        ToolbarControl::ShowLegend(value) => {
            let mut updated = ctx.config.clone();
            updated.show_legend = value;
            config = Some(updated);
        }
        ToolbarControl::ShowGrid(value) => {
            let mut updated = ctx.config.clone();
            updated.show_grid = value;
            config = Some(updated);
        }
        ToolbarControl::ShowValues(value) => {
            let mut updated = ctx.config.clone();
            updated.show_values = value;
            config = Some(updated);
        }
        ToolbarControl::Split => {
            if split_available(ctx) {
                effects.push(ToolbarEffect::ToggleSplit);
            }
        }
        ToolbarControl::Expand => {
            if ctx.on_expand {
                effects.push(ToolbarEffect::Expand);
            }
        }
        ToolbarControl::ExportButton => {
            next.export_open = !state.export_open;
        }
        ToolbarControl::ExportOverlay => {
            next.export_open = false;
        }
        ToolbarControl::ExportPng | ToolbarControl::ExportSvg => {
            next.export_open = false;
            if ctx.chart_available {
                effects.push(ToolbarEffect::Export {
                    format: if control == ToolbarControl::ExportPng {
                        ChartExportFormat::Png
                    } else {
                        ChartExportFormat::Svg
                    },
                    filename: format!("chart-{}", ctx.now_ms),
                });
            }
        }
    }
    ToolbarOutcome {
        state: next,
        config,
        effects,
    }
}
