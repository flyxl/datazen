//! T260: the EXPLAIN panel, ported from
//! `packages/frontend/src/components/ai/ExplainPanel.tsx`.
//!
//! The AI tab beside a query's EXPLAIN output: the raw driver output
//! (a DataTable when the driver returns the result set inside
//! `planJson`, like MySQL / Kiwi; the plain-text `<pre>` for the
//! plan-only drivers), the plan tree, and the AI's analysis — the
//! summary, the bottlenecks with their severities, and the suggestions
//! with their optional SQL and expected impact — plus the analyzing
//! spinner, the failure card, and the unconfigured notice.
//!
//! | React piece | here |
//! |---|---|
//! | `ExplainPanelProps` | the [`ExplainPanelVm`] props bundle + the [`ExplainPanel`] builder |
//! | `rawTable` `useMemo` (52–70) | [`raw_explain_table`] / [`RawExplainTable`] |
//! | `handleAnalyze` (76–79) | the analyze button's press — the [`AnalyzeExplainRequest`] payload |
//! | the `aiStore` selectors (39–45) | the Vm's four read fields |
//! | the severity card's `cn(...)` pair (25–33) | [`severity_paint`] (border / wash / text) and [`severity_dot_color`] |
//! | `ExplainPlanTree` | the ported tree, threaded the host's expansion overrides and toggle |
//! | `DataTable` + `statusBar` | the ported grid with the row/column count bar |
//! | `Spinner size="lg"` | the kit spinner, `Lg`, muted |
//! | `openSettingsWindow('ai')` (234–243) | the [`ExplainPanel::on_open_ai_settings`] seam — a window open is the host's job (the AiChatPanel shell's precedent) |
//!
//! ## Key convention
//!
//! | part | key |
//! |---|---|
//! | root | `explain-panel` |
//! | raw output section / label / pre | `explain-panel-raw` / `-raw-label` / `-raw-pre` |
//! | analyze button | `explain-panel-analyze` |
//! | raw table wrapper / status bar | `explain-panel-raw-table` / `-raw-status` |
//! | plan tree section | `explain-panel-plan` |
//! | analyzing row / spinner / text | `explain-panel-analyzing` / `-analyzing-spinner` / `-analyzing-text` |
//! | error card | `explain-panel-error` |
//! | analysis block / summary label + text | `explain-panel-analysis` / `-summary-label` / `-summary-text` |
//! | bottlenecks label / card `<n>` | `explain-panel-bottlenecks-label` / `-bottleneck-<n>` (dot / node / description) |
//! | suggestions label / card `<n>` | `explain-panel-suggestions-label` / `-suggestion-<n>` (description / sql / apply / impact) |
//! | unconfigured row / text / button | `explain-panel-unconfigured` / `-unconfigured-text` / `-unconfigured-settings` |
//!
//! ## Deliberate boundaries
//!
//! * The unmount cleanup (`useEffect` 36–41: `clearExplainAnalysis` on
//!   teardown) is the **host's** job: the store action is ported, and the
//!   query window that mounts this panel calls it when the panel goes
//!   away — Rune exposes no unmount hook here, and no press ever runs
//!   that cleanup in React either.
//! * `useEffect`'s auto-runs and React's `void` promise handling live
//!   with the store wiring (the `analyzeExplain` async action): the
//!   button's press hands the host the request and the host drives the
//!   `begin/complete/fail_analyze_explain` seams.
//! * `select-text` on the error card and the suggestion prose stays the
//!   Rune default (selectable text) — the opt-outs, not the opt-ins, are
//!   what a port must restate.
//! * The raw table wrapper's `overflow-hidden` clips nothing Rune draws
//!   (the grid's own rounding carries the corners); the clip is the
//!   browser's, not a layout rule this port needs to restate.

use std::rc::Rc;

use rune::{col, row, text, El, ScrollSpec};
use rune_kit::{Button, ButtonVariant, ControlSize};
use serde_json::Value;

use crate::ai_store::{ExplainAnalysis, ExplainSeverity};
use crate::components::data_table::data_table::DataTable;
use crate::components::data_table::table_header::Column;
use crate::components::query::explain_plan_tree::{
    ExplainPlanNode, ExplainPlanTree, ExpandOverrides,
};
use crate::i18n::t;
use crate::icons::{
    ICON_ALERT_TRIANGLE, ICON_ARROW_DOWN_TO_LINE, ICON_SETTINGS, ICON_SPARKLES, ICON_ZAP,
};
use crate::ui::kit::kit;
use crate::ui::spinner::{Spinner, SpinnerSize, SpinnerTone};

/// React `analyzeExplain`'s request spread (76–79):
/// `{ dbSessionId, explainOutput, originalSql }`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AnalyzeExplainRequest {
    pub db_session_id: String,
    pub explain_output: String,
    pub original_sql: String,
}

/// React's `rawTable` `useMemo` (52–70): the driver's raw EXPLAIN result
/// set inside `planJson` — the column list and the row arrays, the same
/// view as running EXPLAIN directly in the SQL editor.
#[derive(Debug, Clone)]
pub struct RawExplainTable {
    /// `{ id: name, name, type: 'string' }` per string column.
    pub columns: Vec<Column>,
    /// The row arrays, filtered to the array rows as React's
    /// `rows.filter(Array.isArray)` keeps them.
    pub rows: Vec<Vec<Value>>,
}

/// React's `rawTable` derivation, field for field: a plan-`null`, a
/// non-object plan, a missing/ non-array `columns` or `rows`, or an
/// all-non-string column list all fall to `None` (the `<pre>` branch);
/// an empty column list after the string filter also falls (52–70).
pub fn raw_explain_table(plan_json: Option<&Value>) -> Option<RawExplainTable> {
    let record = plan_json?;
    let object = record.as_object()?;
    let cols = object.get("columns")?.as_array()?;
    let rows = object.get("rows")?.as_array()?;
    let columns: Vec<Column> = cols
        .iter()
        .filter(|c| c.as_str().is_some())
        .map(|name| Column::new(name.as_str().unwrap(), name.as_str().unwrap()).ty("string"))
        .collect();
    if columns.is_empty() {
        return None;
    }
    let rows: Vec<Vec<Value>> = rows
        .iter()
        .filter(|r| r.as_array().is_some())
        .map(|r| r.as_array().unwrap().clone())
        .collect();
    Some(RawExplainTable { columns, rows })
}

/// The panel's props bundle: the three inputs (the session, the SQL, the
/// raw output), the plan pair (`planJson` / `planTree`), and the four
/// `aiStore` reads (`explainAnalysis`, `isAnalyzingExplain`,
/// `explainError`, `isConfigured`).
#[derive(Debug, Clone, Default)]
pub struct ExplainPanelVm {
    pub db_session_id: String,
    pub sql: String,
    pub explain_output: String,
    pub plan_json: Option<Value>,
    pub plan_tree: Option<ExplainPlanNode>,
    /// The `aiStore` reads — the panel renders them, the host keeps them
    /// true (the store wiring drives the three analyze seams).
    pub analysis: Option<ExplainAnalysis>,
    pub is_analyzing: bool,
    pub explain_error: Option<String>,
    pub is_configured: bool,
}

/// One severity card's paints: the border, the wash, and the text —
/// React's `severityColors` pair (25–29), `high`/`medium`/`low` in the
/// kit's danger/warning/accent tokens.
fn severity_paint(k: &rune_kit::Kit, severity: ExplainSeverity) -> (rune::Color, rune::Color, rune::Color) {
    match severity {
        ExplainSeverity::High => (
            k.alpha(k.c.danger.border, 0.30),
            k.alpha(k.c.danger.bg, 0.10),
            k.c.danger.fg,
        ),
        ExplainSeverity::Medium => (
            k.alpha(k.c.warning.border, 0.30),
            k.alpha(k.c.warning.bg, 0.10),
            k.c.warning.fg,
        ),
        ExplainSeverity::Low => (
            k.alpha(k.c.bg_accent, 0.30),
            k.alpha(k.c.bg_accent, 0.10),
            k.c.text_accent,
        ),
    }
}

/// The severity dot's fill: React's `severityIcons` (31–35),
/// `bg-danger` / `bg-warning` / `bg-accent`.
fn severity_dot_color(k: &rune_kit::Kit, severity: ExplainSeverity) -> rune::Color {
    match severity {
        ExplainSeverity::High => k.c.danger.bg,
        ExplainSeverity::Medium => k.c.warning.bg,
        ExplainSeverity::Low => k.c.bg_accent,
    }
}

/// One section label: React's `mb-1/mb-1.5 text-[11px] font-medium
/// uppercase tracking-wider text-fg-muted` heading line. Rune text has
/// no text-transform, so the uppercase is baked in (the diagnosis
/// panel's `section_label` precedent). For column headings only — a
/// `w_full` child inside a row overflows it.
fn section_label(k: &rune_kit::Kit, key: &str, label: &str) -> El {
    text(label.to_uppercase())
        .key(key)
        .w_full()
        .mb(4.0)
        .font(k.ty.small_medium)
        .color(k.c.text_muted)
}

/// One section label with its leading glyph: React's `mb-1.5 flex
/// items-center gap-1.5 text-[11px] font-medium uppercase
/// tracking-wider text-fg-muted` heading row. The text keeps its own
/// auto width — the row is the `w_full` half, so nothing overflows.
fn label_with_icon(k: &rune_kit::Kit, key: &str, icon: &'static rune::Icon, label: &str) -> El {
    row()
        .key(key)
        .w_full()
        .items_center()
        .gap(6.0)
        .mb(6.0)
        .child(
            rune::icon(icon, 12.0)
                .key(format!("{key}-icon"))
                .color(k.c.text_muted)
                .a11y_hidden(),
        )
        .child(
            text(label.to_uppercase())
                .key(format!("{key}-text"))
                .font(k.ty.small_medium)
                .color(k.c.text_muted),
        )
}

/// The EXPLAIN panel. Build it with the props bundle, the plan tree's
/// expansion wiring, and the host callbacks.
pub struct ExplainPanel<'a, S: 'static> {
    vm: &'a ExplainPanelVm,
    plan_overrides: Option<fn(&S) -> &ExpandOverrides>,
    plan_toggle: Option<fn(&mut S, String)>,
    on_analyze: Option<Rc<dyn Fn(&mut S, AnalyzeExplainRequest)>>,
    on_apply_sql: Option<Rc<dyn Fn(&mut S, &str)>>,
    on_open_ai_settings: Option<Rc<dyn Fn(&mut S)>>,
}

impl<'a, S: 'static> ExplainPanel<'a, S> {
    /// The panel for `vm` — the props plus the `aiStore` reads.
    pub fn new(vm: &'a ExplainPanelVm) -> Self {
        Self {
            vm,
            plan_overrides: None,
            plan_toggle: None,
            on_analyze: None,
            on_apply_sql: None,
            on_open_ai_settings: None,
        }
    }

    /// The plan tree's expansion state: the host reads the overrides and
    /// toggles a node (the tree component's own contract).
    pub fn plan_expansions(
        mut self,
        read: fn(&S) -> &ExpandOverrides,
        toggle: fn(&mut S, String),
    ) -> Self {
        self.plan_overrides = Some(read);
        self.plan_toggle = Some(toggle);
        self
    }

    /// React `handleAnalyze`: `analyzeExplain({ dbSessionId,
    /// explainOutput, originalSql })` — the host's async action receives
    /// this request and drives the store's analyze seams.
    pub fn on_analyze(
        mut self,
        f: impl Fn(&mut S, AnalyzeExplainRequest) + 'static,
    ) -> Self {
        self.on_analyze = Some(Rc::new(f));
        self
    }

    /// React `onApplySql` — a suggestion's apply button.
    pub fn on_apply_sql(mut self, f: impl Fn(&mut S, &str) + 'static) -> Self {
        self.on_apply_sql = Some(Rc::new(f));
        self
    }

    /// `openSettingsWindow('ai')` — the unconfigured notice's configure
    /// button; a window open is the host's job.
    pub fn on_open_ai_settings(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_open_ai_settings = Some(Rc::new(f));
        self
    }

    /// The panel: the raw output section, the plan tree section, the
    /// four analysis states, and the unconfigured notice.
    pub fn build(self, s: &S, dark: bool, layer: &mut Vec<El>) -> El {
        let k = kit(dark);
        let mut root = col()
            .key("explain-panel")
            .w_full()
            .grow(1.0)
            .min_h(0.0)
            // React 82: `flex min-h-0 flex-1 flex-col overflow-auto`.
            .scroll(ScrollSpec { stick_bottom: false })
            .items_stretch()
            .child(self.raw_output_section(s, &k, dark, layer))
            .child(self.plan_tree_section(s, dark))
            .child(self.analyzing_row(&k, dark))
            .child(self.error_section(&k));
        if let Some(analysis) = self.vm.analysis.clone() {
            root = root.child(self.analysis_sections(&k, &analysis));
        }
        if !self.vm.is_configured {
            root = root.child(self.unconfigured_row(&k));
        }
        root
    }

    /// React 84–128: the raw output — the label row with the analyze
    /// button, and the DataTable or the `<pre>`.
    fn raw_output_section(
        &self,
        s: &S,
        k: &rune_kit::Kit,
        dark: bool,
        layer: &mut Vec<El>,
    ) -> El {
        // React 85–90: `mb-1.5 flex items-center justify-between` — the
        // label and (when configured and idle) the analyze button. The
        // label is the row's auto-width half, not a `w_full` heading —
        // a full-width first child would squeeze the button out of the
        // row.
        let mut head = row()
            .key("explain-panel-raw-head")
            .items_center()
            .justify_between()
            .mb(6.0)
            .child(
                text(t("explain.rawOutput").to_uppercase())
                    .key("explain-panel-raw-label")
                    .font(k.ty.small_medium)
                    .color(k.c.text_muted),
            );
        if self.vm.is_configured && self.vm.analysis.is_none() && !self.vm.is_analyzing {
            head = head.child(self.analyze_button(k));
        }

        // React 104–126: the DataTable branch when the driver returns
        // the result set, the `<pre>` branch otherwise.
        let body = match raw_explain_table(self.vm.plan_json.as_ref()) {
            Some(raw) => self.raw_table(s, k, dark, layer, &raw),
            None => {
                // React 122–124: `max-h-[200px] overflow-auto rounded
                // border border-edge bg-surface-alt p-2 font-mono
                // text-xs text-fg-secondary`.
                text(self.vm.explain_output.clone())
                    .key("explain-panel-raw-pre")
                    .w_full()
                    .max_h(200.0)
                    .scroll(ScrollSpec { stick_bottom: false })
                    .px(8.0)
                    .py(8.0)
                    .radius(k.radius.sm)
                    .border(1.0, k.c.border_base)
                    .fill(k.c.layer_01)
                    .font(k.ty.text_xsmall.mono())
                    .color(k.c.text_muted)
                    .wrap()
            }
        };

        col()
            .key("explain-panel-raw")
            .w_full()
            .items_stretch()
            // React 83: `border-b border-edge p-3`.
            .border_sides([0.0, 0.0, 1.0, 0.0], k.c.border_base)
            .px(12.0)
            .py(12.0)
            .child(head)
            .child(body)
    }

    /// React 87–102: the analyze button — `h-6 gap-1 px-2 text-[11px]`
    /// primary with the 12pt sparkles.
    fn analyze_button(&self, k: &rune_kit::Kit) -> El {
        let mut button = Button::new(t("explain.analyze").to_owned())
            .variant(ButtonVariant::Contrast)
            .control_size(ControlSize::Small)
            .icon(&ICON_SPARKLES)
            .help(t("explain.analyze"))
            .build(k)
            .key("explain-panel-analyze");
        if let Some(on_analyze) = self.on_analyze.clone() {
            let request = AnalyzeExplainRequest {
                db_session_id: self.vm.db_session_id.clone(),
                explain_output: self.vm.explain_output.clone(),
                original_sql: self.vm.sql.clone(),
            };
            button = button.on_press(move |s: &mut S| on_analyze(s, request.clone()));
        }
        button
    }

    /// React 104–121: the raw result set — the wrapped DataTable with
    /// the row/column count status bar in the grid's own status slot.
    fn raw_table(
        &self,
        s: &S,
        k: &rune_kit::Kit,
        dark: bool,
        layer: &mut Vec<El>,
        raw: &RawExplainTable,
    ) -> El {
        // React 110–118: `flex items-center gap-3 border-b border-edge
        // bg-surface-alt px-3 py-1.5 text-xs text-fg-secondary` — the
        // count bar, in the grid's `statusBar` slot.
        let status = row()
            .key("explain-panel-raw-status")
            .items_center()
            .gap(12.0)
            .w_full()
            .border_sides([0.0, 0.0, 1.0, 0.0], k.c.border_base)
            .fill(k.c.layer_01)
            .px(12.0)
            .py(6.0)
            .child(
                text(format!("{} {}", raw.rows.len(), t("common.rows")))
                    .key("explain-panel-raw-status-rows")
                    .font(k.ty.text_xsmall)
                    .color(k.c.text_muted),
            )
            .child(
                text("|")
                    .key("explain-panel-raw-status-sep")
                    .font(k.ty.text_xsmall)
                    .color(k.c.border_base),
            )
            .child(
                text(format!("{} {}", raw.columns.len(), t("common.columns")))
                    .key("explain-panel-raw-status-columns")
                    .font(k.ty.text_xsmall)
                    .color(k.c.text_muted),
            );

        // React 103: the wrapper `overflow-hidden rounded border
        // border-edge` around the grid.
        col()
            .key("explain-panel-raw-table")
            .w_full()
            .items_stretch()
            .radius(k.radius.sm)
            .border(1.0, k.c.border_base)
            .child(
                DataTable::new(raw.columns.clone(), &raw.rows)
                    .status_bar(status)
                    .build(s, dark, layer),
            )
    }

    /// React 130–135: the plan tree section — mounted only while either
    /// the tree or the plan JSON exists (`{planTree != null ||
    /// planJson != null} && <div…>`); the tree itself renders nothing
    /// for a `null` tree. The tree needs the host's expansion wiring;
    /// without it the section mounts empty, the same nothing the tree
    /// renders for a plan it cannot read.
    fn plan_tree_section(&self, s: &S, dark: bool) -> El {
        if self.vm.plan_tree.is_none() && self.vm.plan_json.is_none() {
            return rune::el();
        }
        let k = kit(dark);
        let mut section = col()
            .key("explain-panel-plan")
            .w_full()
            .items_stretch()
            .border_sides([0.0, 0.0, 1.0, 0.0], k.c.border_base)
            .px(12.0)
            .py(12.0);
        if let (Some(read), Some(toggle)) = (self.plan_overrides, self.plan_toggle) {
            let tree = ExplainPlanTree::new(self.vm.plan_tree.clone(), read, toggle);
            section = section.child(tree.build(s, dark));
        }
        section
    }

    /// React 137–142: `isAnalyzing` — the spinner row.
    fn analyzing_row(&self, k: &rune_kit::Kit, dark: bool) -> El {
        if !self.vm.is_analyzing {
            return rune::el();
        }
        row()
            .key("explain-panel-analyzing")
            .items_center()
            .gap(8.0)
            .px(16.0)
            .py(16.0)
            .child(
                Spinner::new("explain-panel-analyzing-spinner")
                    .size(SpinnerSize::Lg)
                    .tone(SpinnerTone::Muted)
                    .label(t("explain.analyzing"))
                    .build(dark),
            )
            .child(
                text(t("explain.analyzing"))
                    .key("explain-panel-analyzing-text")
                    .font(k.ty.text_xsmall)
                    .color(k.c.text_muted),
            )
    }

    /// React 144–151: `explainError` — the red card inside its `p-3`.
    fn error_section(&self, k: &rune_kit::Kit) -> El {
        let Some(error) = self.vm.explain_error.as_deref() else {
            return rune::el();
        };
        col()
            .key("explain-panel-error-section")
            .w_full()
            .px(12.0)
            .py(12.0)
            .child(
                text(error)
                    .key("explain-panel-error")
                    .w_full()
                    .font(k.ty.text_xsmall)
                    .color(k.c.danger.fg)
                    .wrap()
                    .px(12.0)
                    .py(8.0)
                    .radius(k.radius.sm)
                    .border(1.0, k.alpha(k.c.danger.fg, 0.2))
                    .fill(k.alpha(k.c.danger.fg, 0.1)),
            )
    }

    /// React 153–220: the analysis — the summary card, the bottlenecks,
    /// the suggestions.
    fn analysis_sections(&self, k: &rune_kit::Kit, analysis: &ExplainAnalysis) -> El {
        let mut sections = col()
            .key("explain-panel-analysis")
            .w_full()
            .items_stretch()
            .gap(12.0)
            .px(12.0)
            .py(12.0);

        // React 155–161: the summary card.
        sections = sections.child(
            col()
                .key("explain-panel-summary")
                .w_full()
                .items_stretch()
                .px(12.0)
                .py(8.0)
                .radius(k.radius.sm)
                .border(1.0, k.c.border_base)
                .fill(k.c.layer_01)
                .child(section_label(k, "explain-panel-summary-label", t("explain.summary")))
                .child(
                    text(analysis.summary.clone())
                        .key("explain-panel-summary-text")
                        .w_full()
                        // React: `text-sm text-fg`.
                        .font(k.ty.body)
                        .color(k.c.text_base)
                        .wrap(),
                ),
        );

        // React 163–190: the bottlenecks — the icon + label row, then one
        // card per bottleneck.
        if !analysis.bottlenecks.is_empty() {
            sections = sections
                .child(label_with_icon(
                    k,
                    "explain-panel-bottlenecks-label",
                    &ICON_ALERT_TRIANGLE,
                    t("explain.bottlenecks"),
                ))
                .child(self.bottleneck_cards(k, analysis));
        }

        // React 192–219: the suggestions — the icon + label row, then one
        // card per suggestion.
        if !analysis.suggestions.is_empty() {
            sections = sections
                .child(label_with_icon(
                    k,
                    "explain-panel-suggestions-label",
                    &ICON_ZAP,
                    t("explain.suggestions"),
                ))
                .child(self.suggestion_cards(k, analysis));
        }
        sections
    }

    /// React 171–189: one card per bottleneck, the severity's own
    /// border/wash/text and its dot.
    fn bottleneck_cards(&self, k: &rune_kit::Kit, analysis: &ExplainAnalysis) -> El {
        let mut list = col()
            .key("explain-panel-bottlenecks")
            .w_full()
            .items_stretch()
            .gap(6.0);
        for (index, bottleneck) in analysis.bottlenecks.iter().enumerate() {
            let (border, wash, ink) = severity_paint(k, bottleneck.severity);
            let dot = severity_dot_color(k, bottleneck.severity);
            list = list.child(
                col()
                    .key(format!("explain-panel-bottleneck-{index}"))
                    .w_full()
                    .items_stretch()
                    .px(12.0)
                    .py(8.0)
                    .radius(k.radius.sm)
                    .border(1.0, border)
                    .fill(wash)
                    .child(
                        row()
                            .key(format!("explain-panel-bottleneck-{index}-head"))
                            .items_center()
                            .gap(8.0)
                            // React 176–181: the 2-point dot.
                            .child(
                                row()
                                    .key(format!("explain-panel-bottleneck-{index}-dot"))
                                    .w(8.0)
                                    .h(8.0)
                                    .radius(4.0)
                                    .fill(dot),
                            )
                            .child(
                                text(bottleneck.node.clone())
                                    .key(format!("explain-panel-bottleneck-{index}-node"))
                                    .font(k.ty.text_xsmall_medium)
                                    .color(ink),
                            ),
                    )
                    .child(
                        text(bottleneck.description.clone())
                            .key(format!("explain-panel-bottleneck-{index}-description"))
                            .w_full()
                            .mt(4.0)
                            // React 188: `mt-1 text-xs opacity-80`.
                            .font(k.ty.text_xsmall)
                            .color(k.alpha(ink, 0.8))
                            .wrap(),
                    ),
            );
        }
        list
    }

    /// React 196–218: one card per suggestion — the description, the
    /// optional SQL pre with its apply button, and the impact line.
    fn suggestion_cards(&self, k: &rune_kit::Kit, analysis: &ExplainAnalysis) -> El {
        let mut list = col()
            .key("explain-panel-suggestions")
            .w_full()
            .items_stretch()
            .gap(6.0);
        for (index, suggestion) in analysis.suggestions.iter().enumerate() {
            let mut card = col()
                .key(format!("explain-panel-suggestion-{index}"))
                .w_full()
                .items_stretch()
                .px(12.0)
                .py(8.0)
                .radius(k.radius.sm)
                // React 202: `rounded border border-green-500/20
                // bg-green-500/5`.
                .border(1.0, k.alpha(k.c.success.border, 0.2))
                .fill(k.alpha(k.c.success.bg, 0.05))
                .child(
                    text(suggestion.description.clone())
                        .key(format!("explain-panel-suggestion-{index}-description"))
                        .w_full()
                        .font(k.ty.text_xsmall)
                        .color(k.c.text_muted)
                        .wrap(),
                );
            if let Some(sql) = suggestion.sql.clone() {
                let mut sql_block = col()
                    .key(format!("explain-panel-suggestion-{index}-sql-block"))
                    .w_full()
                    .items_stretch()
                    .mt(6.0)
                    .child(
                        text(sql.clone())
                            .key(format!("explain-panel-suggestion-{index}-sql"))
                            .w_full()
                            .px(8.0)
                            .py(8.0)
                            .radius(k.radius.sm)
                            .font(k.ty.mono)
                            .color(k.c.success.fg)
                            .border(1.0, k.c.border_base)
                            .fill(k.c.bg_base)
                            .wrap(),
                    );
                if self.on_apply_sql.is_some() {
                    sql_block = sql_block.child(self.apply_button(k, index, &sql));
                }
                card = card.child(sql_block);
            }
            card = card.child(
                text(format!("{}: {}", t("explain.expectedImpact"), suggestion.impact))
                    .key(format!("explain-panel-suggestion-{index}-impact"))
                    .w_full()
                    .mt(4.0)
                    .font(k.ty.small)
                    .color(k.c.text_muted)
                    .wrap(),
            );
            list = list.child(card);
        }
        list
    }

    /// React 210–217: the apply button — `h-5 gap-1 px-1.5 text-[10px]`
    /// ghost with the 10pt arrow glyph.
    fn apply_button(&self, k: &rune_kit::Kit, index: usize, sql: &str) -> El {
        let mut button = Button::new(t("nl2sql.apply").to_owned())
            .variant(ButtonVariant::Ghost)
            .control_size(ControlSize::Small)
            .icon(&ICON_ARROW_DOWN_TO_LINE)
            .help(t("nl2sql.apply"))
            .build(k)
            .key(format!("explain-panel-suggestion-{index}-apply"));
        if let Some(on_apply) = self.on_apply_sql.clone() {
            let sql = sql.to_owned();
            button = button.on_press(move |s: &mut S| on_apply(s, &sql));
        }
        button
    }

    /// React 222–243: the unconfigured notice — the sparkles line and
    /// the configure button.
    fn unconfigured_row(&self, k: &rune_kit::Kit) -> El {
        let mut settings = Button::new(t("settings.ai.goToConfigure").to_owned())
            .variant(ButtonVariant::Contrast)
            .control_size(ControlSize::Small)
            .icon(&ICON_SETTINGS)
            .help(t("settings.ai.goToConfigure"))
            .build(k)
            .key("explain-panel-unconfigured-settings");
        if let Some(open) = self.on_open_ai_settings.clone() {
            settings = settings.on_press(move |s: &mut S| open(s));
        }

        row()
            .key("explain-panel-unconfigured")
            .items_center()
            .gap(8.0)
            .px(16.0)
            .py(16.0)
            .child(
                rune::icon(&ICON_SPARKLES, 14.0)
                    .key("explain-panel-unconfigured-icon")
                    .color(k.c.text_muted)
                    .a11y_hidden(),
            )
            .child(
                text(t("common.aiNotConfigured"))
                    .key("explain-panel-unconfigured-text")
                    .grow(1.0)
                    .font(k.ty.text_xsmall)
                    .color(k.c.text_muted),
            )
            .child(settings)
    }
}
