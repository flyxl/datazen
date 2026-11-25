//! Port of `packages/frontend/src/components/chart/AxisConfigurator.tsx`.
//!
//! The panel that edits `ChartConfig`: X/Y axis field mapping, group-by,
//! aggregation, sort, color scheme and the optional recommendation hint. React
//! keeps `collapsed` in local state and reports every edit through `onChange`;
//! Rune keeps `collapsed` in the host state slice and reports a whole new
//! `ChartConfig` through `on_change`, so the single React contract is preserved.
//!
//! The domain VMs (`ChartConfig`, `ChartField`, `ChartType`, `AggregationType`,
//! `ChartSort`, `InferredFieldType`) are reused from `chart_canvas` — this file
//! adds no second copy. `COLOR_PALETTES` mirrors `lib/chart/colors.ts` because
//! the swatches are data, not runtime chrome; all chrome colors come from
//! `kit(dark)` tokens, with no new CSS color syntax introduced.
//!
//! ## Keys
//!
//! React gives this panel no `data-testid`s. The keys below are Rune-side
//! observation handles named after the controls; the i18n keys are the exact
//! React ones. `chart-axis-config-toggle` is the collapsed-state button
//! (React's `ChevronRight` button with `title=chart.configPanel`).
//!
//! ## Local contract (removal stage)
//!
//! `AxisRecommendation` mirrors only the `reason` field the component reads.
//! Replace it with the shared chart contract when that lands and drop the
//! per-item allows.

use std::collections::HashMap;
use std::rc::Rc;

use rune::{col, color::hex, icon, row, text, El, Icon, Tx};

use crate::components::chart::chart_canvas::{
    AggregationType, ChartConfig, ChartField, ChartSort, ChartType, InferredFieldType,
};
use crate::i18n::t;
use crate::icons::{ICON_CHEVRON_DOWN, ICON_CHEVRON_RIGHT, ICON_PLUS, ICON_X};
use crate::ui::kit::kit;
use crate::ui::select::Select;

/// The panel's fixed React width (`w-52` = 13rem).
const PANEL_WIDTH: f32 = 208.0;

/// Mirror of `lib/chart/colors.ts` `COLOR_PALETTES`, in declaration order.
/// Eight palettes, each with at least four swatches (React shows `slice(0, 4)`).
#[allow(dead_code)] // palette data; consumed by this module's build.
pub const COLOR_PALETTES: [(&str, &[&str]); 8] = [
    (
        "default",
        &["#60a5fa", "#34d399", "#fbbf24", "#f87171", "#a78bfa", "#22d3ee", "#fb923c", "#f472b6"],
    ),
    (
        "workspace",
        &["#2563eb", "#60a5fa", "#93c5fd", "#34d399", "#5eead4", "#fbbf24", "#f87171", "#a78bfa"],
    ),
    (
        "navy",
        &["#0b1220", "#111827", "#182231", "#202b3a", "#374151", "#6b7280", "#9ca3af", "#d1d5db"],
    ),
    ("status", &["#34d399", "#fbbf24", "#f87171", "#60a5fa", "#9ca3af"]),
    (
        "ocean",
        &["#0ea5e9", "#0284c7", "#0369a1", "#075985", "#0c4a6e", "#164e63", "#155e75", "#0e7490"],
    ),
    (
        "forest",
        &["#22c55e", "#16a34a", "#15803d", "#166534", "#14532d", "#365314", "#3f6212", "#4d7c0f"],
    ),
    (
        "warm",
        &["#f97316", "#ef4444", "#f59e0b", "#ec4899", "#e11d48", "#be123c", "#9f1239", "#881337"],
    ),
    (
        "mono",
        &["#6b7280", "#4b5563", "#374151", "#1f2937", "#9ca3af", "#d1d5db", "#e5e7eb", "#f3f4f6"],
    ),
];

/// The only chart-recommendation field this panel renders.
#[allow(dead_code)] // contract mirror; remove when the shared chart contract lands.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AxisRecommendation {
    /// A translation key, passed through `t()` exactly like React.
    pub reason: String,
}

type ChangeFn<S> = Rc<dyn Fn(&mut S, ChartConfig) + 'static>;
type CollapsedFn<S> = Rc<dyn Fn(&mut S, bool) + 'static>;
/// Per-select open state, keyed by the select key (React owns it inside each
/// `Select`; one shared bool would open every listbox at once).
type OpenFn<S> = Rc<dyn for<'b> Fn(&'b mut S, &str) -> &'b mut bool + 'static>;
type PickFn<S> = Rc<dyn Fn(&mut S, &str) + 'static>;
type ActionFn<S> = Rc<dyn Fn(&mut S) + 'static>;

/// Raw TS spelling of `InferredFieldType`, used in `"name (type)"` labels.
fn inferred_label(kind: InferredFieldType) -> &'static str {
    match kind {
        InferredFieldType::Numeric => "numeric",
        InferredFieldType::Datetime => "datetime",
        InferredFieldType::Categorical => "categorical",
        InferredFieldType::Boolean => "boolean",
        InferredFieldType::Unknown => "unknown",
    }
}

fn is_numeric(field: &ChartField) -> bool {
    field.inferred_type == InferredFieldType::Numeric
}

fn is_categorical(field: &ChartField) -> bool {
    matches!(
        field.inferred_type,
        InferredFieldType::Categorical | InferredFieldType::Datetime
    )
}

fn field_option(field: &ChartField) -> String {
    format!("{} ({})", field.name, inferred_label(field.inferred_type))
}

fn find_field<'a>(fields: &'a [ChartField], name: &str) -> Option<&'a ChartField> {
    fields.iter().find(|f| f.name == name)
}

/// `config.xAxis ?? ''` → the matching option label (kit selects match labels).
fn x_current(config: &ChartConfig, fields: &[ChartField]) -> String {
    match config.x_axis.as_deref() {
        None => t("chart.autoIndex").to_owned(),
        Some(name) => find_field(fields, name).map(field_option).unwrap_or_else(|| name.to_owned()),
    }
}

fn y_current(fields: &[ChartField], name: &str) -> String {
    find_field(fields, name).map(|f| f.name.clone()).unwrap_or_else(|| name.to_owned())
}

fn group_current(config: &ChartConfig, fields: &[ChartField]) -> String {
    match config.group_by.as_deref() {
        None => t("chart.noGroupBy").to_owned(),
        Some(name) => find_field(fields, name).map(|f| f.name.clone()).unwrap_or_else(|| name.to_owned()),
    }
}

fn aggregation_value(kind: AggregationType) -> &'static str {
    match kind {
        AggregationType::None => "none",
        AggregationType::Sum => "sum",
        AggregationType::Avg => "avg",
        AggregationType::Count => "count",
        AggregationType::Min => "min",
        AggregationType::Max => "max",
        AggregationType::DistinctCount => "distinct_count",
    }
}

fn from_aggregation(value: &str) -> AggregationType {
    match value {
        "sum" => AggregationType::Sum,
        "avg" => AggregationType::Avg,
        "count" => AggregationType::Count,
        "min" => AggregationType::Min,
        "max" => AggregationType::Max,
        "distinct_count" => AggregationType::DistinctCount,
        _ => AggregationType::None,
    }
}

fn sort_value(kind: ChartSort) -> &'static str {
    match kind {
        ChartSort::None => "none",
        ChartSort::XAsc => "x_asc",
        ChartSort::XDesc => "x_desc",
        ChartSort::YAsc => "y_asc",
        ChartSort::YDesc => "y_desc",
    }
}

fn from_sort(value: &str) -> ChartSort {
    match value {
        "x_asc" => ChartSort::XAsc,
        "x_desc" => ChartSort::XDesc,
        "y_asc" => ChartSort::YAsc,
        "y_desc" => ChartSort::YDesc,
        _ => ChartSort::None,
    }
}

/// Apply one React-style `{...config, field: value}` edit through the host.
fn emit<S: 'static>(
    change: &Option<ChangeFn<S>>,
    base: &ChartConfig,
    s: &mut S,
    edit: impl FnOnce(&mut ChartConfig),
) {
    if let Some(change) = change {
        let mut next = base.clone();
        edit(&mut next);
        change(s, next);
    }
}

/// The kit select wrapper: option values (not labels) reach `pick`.
fn select_el<S: 'static>(
    key: &str,
    labels: &[String],
    values: &[&str],
    current: &str,
    accessible_name: &str,
    open: &Option<OpenFn<S>>,
    open_values: &HashMap<String, bool>,
    dark: bool,
    layer: &mut Vec<El>,
    pick: PickFn<S>,
) -> El {
    let k = kit(dark);
    let mut select = Select::new(key, labels)
        .values(values)
        .current(Some(current))
        .label(accessible_name);
    if let Some(accessor) = open {
        let is_open = open_values.get(key).copied().unwrap_or(false);
        let owned_key = key.to_owned();
        let a = accessor.clone();
        let key = owned_key.clone();
        select = select.open(is_open).on_toggle(move |s: &mut S| *a(s, &key) = true);
        let a = accessor.clone();
        let key = owned_key.clone();
        select = select.on_close(move |s: &mut S| *a(s, &key) = false);
        let a = accessor.clone();
        select = select.on_choose(move |s: &mut S, chosen: &str, _tx: &mut Tx| {
            pick(s, chosen);
            *a(s, &owned_key) = false;
        });
    } else {
        // An enabled select needs a toggle to be a live control, not a dead one.
        select = select.on_toggle(|_s: &mut S| {});
        select = select.on_choose(move |s: &mut S, chosen: &str, _tx: &mut Tx| pick(s, chosen));
    }
    select.build(&k, layer)
}

fn icon_button<S: 'static>(
    key: &str,
    glyph: &'static Icon,
    accessible_name: String,
    dark: bool,
    action: Option<ActionFn<S>>,
) -> El {
    let k = kit(dark);
    let mut button = row()
        .key(key)
        .role("button")
        .label(accessible_name)
        .w(20.0)
        .h(20.0)
        .center()
        .focusable(true)
        .child(icon(glyph, 12.0).color(k.c.text_muted));
    if let Some(action) = action {
        button = button.on_press(move |s: &mut S| action(s));
    }
    button
}

/// Builder for the ported panel.
#[allow(dead_code)] // consumer: the chart view (not wired); remove when wired.
pub struct AxisConfigurator<'a, S: 'static> {
    fields: &'a [ChartField],
    config: &'a ChartConfig,
    recommendation: Option<&'a AxisRecommendation>,
    collapsed: bool,
    on_collapsed: Option<CollapsedFn<S>>,
    on_change: Option<ChangeFn<S>>,
    select_open: Option<OpenFn<S>>,
    open_values: HashMap<String, bool>,
}

#[allow(dead_code)] // consumer: the chart view (not wired); remove when wired.
impl<'a, S: 'static> AxisConfigurator<'a, S> {
    pub fn new(
        fields: &'a [ChartField],
        config: &'a ChartConfig,
        recommendation: Option<&'a AxisRecommendation>,
    ) -> Self {
        Self {
            fields,
            config,
            recommendation,
            collapsed: false,
            on_collapsed: None,
            on_change: None,
            select_open: None,
            open_values: HashMap::new(),
        }
    }

    /// React's local `collapsed` state at build time.
    pub fn collapsed(mut self, collapsed: bool) -> Self {
        self.collapsed = collapsed;
        self
    }

    pub fn on_collapsed(mut self, f: impl Fn(&mut S, bool) + 'static) -> Self {
        self.on_collapsed = Some(Rc::new(f));
        self
    }

    pub fn on_change(mut self, f: impl Fn(&mut S, ChartConfig) + 'static) -> Self {
        self.on_change = Some(Rc::new(f));
        self
    }

    /// Per-select open state accessor and its current values (React keeps this
    /// inside each `Select`).
    pub fn select_open(
        mut self,
        values: HashMap<String, bool>,
        f: impl for<'b> Fn(&'b mut S, &str) -> &'b mut bool + 'static,
    ) -> Self {
        self.open_values = values;
        self.select_open = Some(Rc::new(f));
        self
    }

    pub fn build(self, dark: bool, layer: &mut Vec<El>) -> El {
        let k = kit(dark);
        let collapsed = self.collapsed;
        let toggle_action = {
            let cb = self.on_collapsed.clone();
            Some(Rc::new(move |s: &mut S| {
                if let Some(cb) = &cb {
                    cb(s, !collapsed);
                }
            }) as ActionFn<S>)
        };
        if collapsed {
            return icon_button(
                "chart-axis-config-toggle",
                &ICON_CHEVRON_RIGHT,
                t("chart.configPanel").to_owned(),
                dark,
                toggle_action,
            );
        }

        let mut panel = col()
            .key("chart-axis-configurator")
            .w(PANEL_WIDTH)
            .h_full()
            .min_h(0.0)
            .border(1.0, k.c.border_base)
            .child(
                row()
                    .key("chart-axis-config-header")
                    .w_full()
                    .h(28.0)
                    .gap(6.0)
                    .center_y()
                    .padding(8.0)
                    .child(
                        text(t("chart.configPanel"))
                            .key("chart-axis-config-title")
                            .font(k.ty.text_xsmall)
                            .color(k.c.text_base),
                    )
                    .child(
                        row()
                            .w_full()
                            .child(icon_button(
                                "chart-axis-config-collapse",
                                &ICON_CHEVRON_DOWN,
                                t("chart.configPanel").to_owned(),
                                dark,
                                toggle_action,
                            )),
                    ),
            );

        let mut body = col().key("chart-axis-config-body").w_full().gap(12.0).padding(12.0);

        // X axis — every field plus the "auto (row index)" empty option.
        let numeric: Vec<&ChartField> = self.fields.iter().filter(|f| is_numeric(f)).collect();
        let categorical: Vec<&ChartField> = self.fields.iter().filter(|f| is_categorical(f)).collect();
        let unused_y: Vec<&ChartField> = numeric
            .iter()
            .copied()
            .filter(|f| !self.config.y_axes.iter().any(|y| y == &f.name))
            .collect();

        let mut all_labels = vec![t("chart.autoIndex").to_owned()];
        all_labels.extend(self.fields.iter().map(field_option));
        let mut all_values = vec![""];
        all_values.extend(self.fields.iter().map(|f| f.name.as_str()));
        let cb = self.on_change.clone();
        let base = self.config.clone();
        let pick: PickFn<S> = Rc::new(move |s, chosen| {
            emit(&cb, &base, s, |c| {
                c.x_axis = if chosen.is_empty() {
                    None
                } else {
                    Some(chosen.to_owned())
                }
            });
        });
        body = body.child(
            col()
                .key("chart-axis-x-block")
                .w_full()
                .gap(4.0)
                .child(
                    text(t("chart.xAxis"))
                        .key("chart-axis-x-label")
                        .font(k.ty.text_xsmall)
                        .color(k.c.text_muted),
                )
                .child(select_el(
                    "chart-axis-x-select",
                    &all_labels,
                    &all_values,
                    &x_current(self.config, self.fields),
                    t("chart.xAxis"),
                    &self.select_open,
                    &self.open_values,
                    dark,
                    layer,
                    pick,
                )),
        );

        // Y axes — one select per chosen series; the remove control only exists
        // when more than one series is selected (React's `length > 1`).
        let num_labels: Vec<String> = numeric.iter().map(|f| f.name.clone()).collect();
        let num_values: Vec<&str> = numeric.iter().map(|f| f.name.as_str()).collect();
        let mut y_block = col()
            .key("chart-axis-y-block")
            .w_full()
            .gap(4.0)
            .child(
                text(t("chart.yAxis"))
                    .key("chart-axis-y-label")
                    .font(k.ty.text_xsmall)
                    .color(k.c.text_muted),
            );
        for (idx, y_key) in self.config.y_axes.iter().enumerate() {
            let cb = self.on_change.clone();
            let base = self.config.clone();
            let pick: PickFn<S> = Rc::new(move |s, value| {
                emit(&cb, &base, s, |c| {
                    if let Some(slot) = c.y_axes.get_mut(idx) {
                        *slot = value.to_owned();
                    }
                });
            });
            let mut row_el = row()
                .key(format!("chart-axis-y-row-{idx}"))
                .w_full()
                .gap(4.0)
                .child(select_el(
                    &format!("chart-axis-y-select-{idx}"),
                    &num_labels,
                    &num_values,
                    &y_current(self.fields, y_key),
                    t("chart.yAxis"),
                    &self.select_open,
                    &self.open_values,
                    dark,
                    layer,
                    pick,
                ));
            if self.config.y_axes.len() > 1 {
                let cb = self.on_change.clone();
                let base = self.config.clone();
                let action: ActionFn<S> = Rc::new(move |s| {
                    emit(&cb, &base, s, |c| {
                        c.y_axes = c
                            .y_axes
                            .iter()
                            .enumerate()
                            .filter(|(i, _)| *i != idx)
                            .map(|(_, v)| v.clone())
                            .collect();
                    });
                });
                row_el = row_el.child(icon_button(
                    &format!("chart-axis-y-remove-{idx}"),
                    &ICON_X,
                    t("chart.yAxis").to_owned(),
                    dark,
                    Some(action),
                ));
            }
            y_block = y_block.child(row_el);
        }
        let can_add_series = !unused_y.is_empty()
            && self.config.chart_type != ChartType::Pie
            && self.config.chart_type != ChartType::Scatter;
        if can_add_series {
            let cb = self.on_change.clone();
            let base = self.config.clone();
            let next = unused_y[0].name.clone();
            let add: ActionFn<S> = Rc::new(move |s| {
                emit(&cb, &base, s, |c| c.y_axes.push(next.clone()));
            });
            y_block = y_block.child(
                row()
                    .key("chart-axis-add-series")
                    .role("button")
                    .label(t("chart.addSeries"))
                    .gap(4.0)
                    .focusable(true)
                    .child(icon(&ICON_PLUS, 12.0).color(k.c.text_accent))
                    .child(
                        text(t("chart.addSeries"))
                            .key("chart-axis-add-series-label")
                            .font(k.ty.text_xsmall)
                            .color(k.c.text_accent)
                            .wrap(),
                    )
                    .on_press(move |s: &mut S| add(s)),
            );
        }
        body = body.child(y_block);

        // Group by — hidden for pie/scatter or when there are no categorical fields.
        if self.config.chart_type != ChartType::Pie
            && self.config.chart_type != ChartType::Scatter
            && !categorical.is_empty()
        {
            let mut group_labels = vec![t("chart.noGroupBy").to_owned()];
            group_labels.extend(categorical.iter().map(|f| f.name.clone()));
            let mut group_values = vec![""];
            group_values.extend(categorical.iter().map(|f| f.name.as_str()));
            let cb = self.on_change.clone();
            let base = self.config.clone();
            let pick: PickFn<S> = Rc::new(move |s, chosen| {
                emit(&cb, &base, s, |c| {
                    c.group_by = if chosen.is_empty() {
                        None
                    } else {
                        Some(chosen.to_owned())
                    }
                });
            });
            body = body.child(
                col()
                    .key("chart-axis-group-block")
                    .w_full()
                    .gap(4.0)
                    .child(
                        text(t("chart.groupBy"))
                            .key("chart-axis-group-label")
                            .font(k.ty.text_xsmall)
                            .color(k.c.text_muted),
                    )
                    .child(select_el(
                        "chart-axis-group-select",
                        &group_labels,
                        &group_values,
                        &group_current(self.config, self.fields),
                        t("chart.groupBy"),
                        &self.select_open,
                        &self.open_values,
                        dark,
                        layer,
                        pick,
                    )),
            );
        }

        // Aggregation — the six React options (no invented `distinct_count`).
        let agg_values = ["none", "sum", "avg", "count", "min", "max"];
        let agg_labels: Vec<String> = agg_values
            .iter()
            .map(|v| t(&format!("chart.agg.{v}")).to_owned())
            .collect();
        let cb = self.on_change.clone();
        let base = self.config.clone();
        let pick: PickFn<S> = Rc::new(move |s, chosen| {
            emit(&cb, &base, s, |c| c.aggregation = from_aggregation(chosen));
        });
        body = body.child(
            col()
                .key("chart-axis-agg-block")
                .w_full()
                .gap(4.0)
                .child(
                    text(t("chart.aggregation"))
                        .key("chart-axis-agg-label")
                        .font(k.ty.text_xsmall)
                        .color(k.c.text_muted),
                )
                .child(select_el(
                    "chart-axis-agg-select",
                    &agg_labels,
                    &agg_values,
                    t(&format!("chart.agg.{}", aggregation_value(self.config.aggregation))),
                    t("chart.aggregation"),
                    &self.select_open,
                    &self.open_values,
                    dark,
                    layer,
                    pick,
                )),
        );

        // Sort — the five React options.
        let sort_values = ["none", "x_asc", "x_desc", "y_asc", "y_desc"];
        let sort_keys = [
            "chart.sort.none",
            "chart.sort.xAsc",
            "chart.sort.xDesc",
            "chart.sort.yAsc",
            "chart.sort.yDesc",
        ];
        let sort_labels: Vec<String> = sort_keys.iter().map(|key| t(key).to_owned()).collect();
        let cb = self.on_change.clone();
        let base = self.config.clone();
        let pick: PickFn<S> = Rc::new(move |s, chosen| {
            emit(&cb, &base, s, |c| c.sort_by = from_sort(chosen));
        });
        let current_sort = match self.config.sort_by {
            ChartSort::None => "chart.sort.none",
            ChartSort::XAsc => "chart.sort.xAsc",
            ChartSort::XDesc => "chart.sort.xDesc",
            ChartSort::YAsc => "chart.sort.yAsc",
            ChartSort::YDesc => "chart.sort.yDesc",
        };
        body = body.child(
            col()
                .key("chart-axis-sort-block")
                .w_full()
                .gap(4.0)
                .child(
                    text(t("chart.sortBy"))
                        .key("chart-axis-sort-label")
                        .font(k.ty.text_xsmall)
                        .color(k.c.text_muted),
                )
                .child(select_el(
                    "chart-axis-sort-select",
                    &sort_labels,
                    &sort_values,
                    t(current_sort),
                    t("chart.sortBy"),
                    &self.select_open,
                    &self.open_values,
                    dark,
                    layer,
                    pick,
                )),
        );

        // Color scheme — one pressable swatch group per palette.
        let mut palettes = col()
            .key("chart-axis-color-block")
            .w_full()
            .gap(4.0)
            .child(
                text(t("chart.colorScheme"))
                    .key("chart-axis-color-label")
                    .font(k.ty.text_xsmall)
                    .color(k.c.text_muted),
            );
        let mut swatches = row().key("chart-axis-color-options").w_full().gap(6.0).flex_wrap();
        for (name, colors) in COLOR_PALETTES {
            let selected = self.config.color_scheme == *name;
            let cb = self.on_change.clone();
            let base = self.config.clone();
            let palette = (*name).to_owned();
            let choose: ActionFn<S> = Rc::new(move |s| {
                emit(&cb, &base, s, |c| c.color_scheme = palette.clone());
            });
            let mut button = row()
                .key(format!("chart-axis-color-{name}"))
                .role("button")
                .label((*name).to_owned())
                .gap(2.0)
                .padding(4.0)
                .radius(4.0)
                .border(1.0, if selected { k.c.text_accent } else { k.c.border_base })
                .focusable(true)
                .on_press(move |s: &mut S| choose(s));
            for (i, color) in colors.iter().take(4).enumerate() {
                button = button.child(
                    row()
                        .key(format!("chart-axis-color-{name}-swatch-{i}"))
                        .w(12.0)
                        .h(12.0)
                        .radius(3.0)
                        .fill(hex(color).unwrap_or(k.c.border_base)),
                );
            }
            swatches = swatches.child(button);
        }
        body = body.child(palettes.child(swatches));

        // Recommendation hint — the lightbulb glyph is omitted: the icon
        // registry has no lightbulb, and no new glyph is invented here.
        if let Some(recommendation) = self.recommendation {
            body = body.child(
                col()
                    .key("chart-axis-recommendation")
                    .w_full()
                    .gap(4.0)
                    .padding(8.0)
                    .radius(4.0)
                    .border(1.0, k.c.border_base)
                    .child(
                        text(t("chart.recommended"))
                            .key("chart-axis-recommendation-label")
                            .font(k.ty.text_xsmall)
                            .color(k.c.text_accent),
                    )
                    .child(
                        text(t(&recommendation.reason))
                            .key("chart-axis-recommendation-reason")
                            .font(k.ty.text_xsmall)
                            .color(k.c.text_muted)
                            .wrap(),
                    ),
            );
        }

        panel.child(body)
    }
}

// Leftover L1 (removal stage): when the chart view mounts this panel, bind
// `select_open` / `on_collapsed` to its state slice, map `on_change` into its
// config setter, replace `AxisRecommendation` with the shared chart contract,
// and drop the per-item allows.
