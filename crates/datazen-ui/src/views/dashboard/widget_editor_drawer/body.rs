//! The body half of the T201 `WidgetEditorDrawer` port
//! (`packages/frontend/src/windows/dashboard/WidgetEditorDrawer.tsx`, the
//! `flex-1 space-y-4 overflow-auto p-4` column and everything inside it):
//! the title field, the hidden-SQL pair (connection select + the SQL
//! editor seam's border box) or the workflow source (select or read-only
//! field plus the open-workflow-editor link), the refresh-mode preset
//! select with the value/unit interval row and the dense-interval warning,
//! the T146 monitor-schedule authorization card, the enabled checkbox, the
//! default-view select, the chart-type and y-axes fields under the chart
//! view, and the alert-rule card. The shared state, view models, geometry
//! constants and React testid keys it reads are the parent module's
//! privates.

use std::rc::Rc;

use rune::{col, row, text, El, ScrollSpec, TextStyle, Tx};
use rune_kit::{Button, ButtonVariant, ControlSize, Kit};

use crate::components::chart::chart_canvas::ChartType;
use crate::i18n::t;
use crate::ui::field::TextField;
use crate::ui::palette;
use crate::ui::select::Select;
use crate::ui::toggle::Checkbox;
use crate::views::dashboard::chart_widget_tile::TileViewMode;
use crate::views::dashboard::monitor_schedule_authorization::MonitorScheduleAuthorization;

use super::{
    refresh_policy_to_preset, should_warn_refresh_sec, AlertOperatorVm, IntervalUnit,
    RefreshPreset, WidgetEditorDrawer, WidgetEditorDrawerState, WidgetEditorSelect, BODY_GAP,
    BODY_PAD, MIN_REFRESH_SEC, REFRESH_WARN_BELOW_SEC, ROOT_KEY,
};

// ---- Geometry (Tailwind, verbatim) ------------------------------------------

/// React label blocks `space-y-1`.
const LABEL_GAP: f32 = 4.0;
/// React `Input` (`packages/ui/src/Input.tsx`): `h-10`.
const INPUT_H: f32 = 40.0;
/// React interval value input `w-24` / unit select `w-28`.
const INTERVAL_VALUE_W: f32 = 96.0;
const INTERVAL_UNIT_W: f32 = 112.0;
/// React interval row and checkbox labels `gap-2`.
const ROW_GAP: f32 = 8.0;
/// React SQL editor box `h-40`.
const SQL_BOX_H: f32 = 160.0;
/// React workflow block `space-y-2`.
const WF_BLOCK_GAP: f32 = 8.0;
/// React alert card `space-y-3` / `p-3` (and its fields' `space-y-3`).
const ALERT_GAP: f32 = 12.0;
const ALERT_PAD: f32 = 12.0;
/// React read-only field `px-3` / `rounded-[9px]` (the `Input` shell).
const READONLY_PX: f32 = 12.0;
const READONLY_RADIUS: f32 = 9.0;
/// React hint / warn lines `text-[11px]`.
const HINT_SIZE: f32 = 11.0;

/// React `Input`'s `font-mono text-[13px]`.
const MONO_INPUT: TextStyle = TextStyle::new(13.0, 400.0).mono();
/// The read-only workflow field's `font-mono text-xs`.
const MONO_READONLY: TextStyle = TextStyle::new(12.0, 400.0).mono();

/// React `text-xs` (12px / 400).
fn type_xs() -> TextStyle {
    TextStyle::new(12.0, 400.0)
}

/// React `text-[11px]` (11px / 400).
fn type_hint() -> TextStyle {
    TextStyle::new(HINT_SIZE, 400.0)
}

/// React's `CHART_TYPES` (the select's option order).
const CHART_TYPES: [ChartType; 5] =
    [ChartType::Bar, ChartType::Line, ChartType::Pie, ChartType::Scatter, ChartType::Area];
/// React's `ALERT_OPS` (the select's option order).
const ALERT_OPS: [AlertOperatorVm; 6] = [
    AlertOperatorVm::Gt,
    AlertOperatorVm::Gte,
    AlertOperatorVm::Lt,
    AlertOperatorVm::Lte,
    AlertOperatorVm::Eq,
    AlertOperatorVm::Neq,
];

// ---- Keys -------------------------------------------------------------------

/// The SQL editor box (React `data-testid="dashboard-sql-editor"`).
const SQL_BOX_KEY: &str = "dashboard-sql-editor";
/// The open-workflow-editor link (React testid).
const OPEN_WF_KEY: &str = "widget-open-workflow-editor";
/// The dense-interval warning (React testid).
const WARN_KEY: &str = "refresh-sec-warn";

impl<'a, S: 'static> WidgetEditorDrawer<'a, S> {
    // ---- the body -------------------------------------------------------------

    /// React's `flex-1 space-y-4 overflow-auto p-4` form column.
    pub(super) fn body(&self, k: &Kit, dark: bool, layer: &mut Vec<El>) -> El {
        let mut body = col()
            .key(format!("{ROOT_KEY}-body"))
            .w_full()
            .grow(1.0)
            .min_h(0.0)
            .scroll(ScrollSpec { stick_bottom: false }) // overflow-auto
            .padding(BODY_PAD)
            .gap(BODY_GAP) // space-y-4
            .child(self.title_block(k, dark));
        // React's `hiddenSqlProp != null ? (…) : (…)`: the hidden pair are
        // siblings of the column (the fragment), the workflow branch is its
        // own `space-y-2` block.
        if self.state.hidden_mode() {
            body = body.child(self.connection_block(k, layer)).child(self.sql_block(k));
        } else {
            body = body.child(self.workflow_block(k, layer));
        }
        body = body.child(self.refresh_mode_block(k, layer));
        if self.state.interval_row_visible() {
            body = body.child(self.interval_block(k, dark, layer));
        }
        if let Some(card) = self.monitor_card(dark) {
            body = body.child(card);
        }
        body = body
            .child(self.enabled_row(k))
            .child(self.view_mode_block(k, layer));
        // React's `draft.viewMode === 'chart' && draft.chartConfig` gate.
        if self.state.draft.view_mode == TileViewMode::Chart
            && self.state.draft.chart_config.is_some()
        {
            body = body.child(self.chart_type_block(k, layer)).child(self.y_axes_block(k, dark));
        }
        body.child(self.alert_card(k, dark, layer))
    }

    /// The title field: `<label className="block space-y-1">` with the
    /// muted caption and the `Input`.
    fn title_block(&self, k: &Kit, dark: bool) -> El {
        label_block(
            "widget-editor-title",
            t("dashboard.widgetTitle"),
            k,
            self.title_field(dark),
        )
    }

    /// The title `Input` — the kit field at React's `h-10` in the mono face.
    fn title_field(&self, dark: bool) -> El {
        let mut field = TextField::new("widget-editor-title-field", &self.state.title_edit)
            .text_style(MONO_INPUT)
            .height(INPUT_H)
            .w_full()
            .placeholder(t("dashboard.widgetTitlePlaceholder"));
        match &self.state_fn {
            Some(state_fn) => {
                let edit_fn = Rc::clone(state_fn);
                field = field.on_edit(move |s: &mut S| &mut edit_fn(s).title_edit);
                let change_fn = Rc::clone(state_fn);
                field = field
                    .on_change(move |s: &mut S, _tx: &mut Tx| change_fn(s).apply_title_text());
            }
            None => {
                // The unwired host: the kit's field contract keeps the caret
                // out (a documented rendering, not a crash).
                field = field.disabled(true);
            }
        }
        field.build(dark)
    }

    // ---- the hidden-SQL branch -------------------------------------------------

    /// The connection select's label block.
    fn connection_block(&self, k: &Kit, layer: &mut Vec<El>) -> El {
        let options: Vec<(String, String)> = self
            .state
            .connections
            .iter()
            .map(|c| (c.name.clone(), c.id.clone()))
            .collect();
        let current = (!self.state.hidden_sql.connection_id.is_empty())
            .then(|| self.state.hidden_sql.connection_id.as_str());
        let select = self.select_field(
            "widget-editor-connection-select",
            options,
            current,
            Some(t("common.selectConnection")),
            None,
            WidgetEditorSelect::Connection,
            |state, value| state.set_hidden_connection(value),
            k,
            layer,
        );
        label_block("widget-editor-connection", t("dashboard.connection"), k, select)
    }

    /// The SQL editor's label block: React's `h-40 overflow-hidden
    /// rounded-md border border-edge` box with the seam's surface inside.
    fn sql_block(&self, k: &Kit) -> El {
        let mut box_el = col()
            .key(SQL_BOX_KEY)
            .w_full()
            .h(SQL_BOX_H)
            .radius(k.radius.md) // rounded-md
            .border(1.0, k.c.border_base) // border border-edge
            .clip_soft(0.0); // overflow-hidden (the T069 mapping)
        if let Some(editor) = &self.sql_editor {
            box_el = box_el.child(editor.clone());
        }
        label_block("widget-editor-sql", t("dashboard.sql"), k, box_el)
    }

    // ---- the workflow branch ---------------------------------------------------

    /// The `space-y-2` workflow block: the source field (select while the
    /// host lists workflows, else the read-only field) and, when the host
    /// offers the editor and a workflow is bound, the open-editor link.
    fn workflow_block(&self, k: &Kit, layer: &mut Vec<El>) -> El {
        let control = if self.user_workflows.is_empty() {
            self.workflow_readonly_field(k)
        } else {
            let options: Vec<(String, String)> = self
                .user_workflows
                .iter()
                .map(|w| (w.name.clone(), w.id.clone()))
                .collect();
            let current =
                (!self.state.draft.workflow_id.is_empty()).then(|| self.state.draft.workflow_id.as_str());
            self.select_field(
                "widget-editor-workflow-select",
                options,
                current,
                Some(t("dashboard.selectWorkflow")),
                None,
                WidgetEditorSelect::Workflow,
                |state, value| state.set_workflow(value),
                k,
                layer,
            )
        };
        let mut block = col()
            .key("widget-editor-workflow-block")
            .w_full()
            .gap(WF_BLOCK_GAP)
            .child(label_block(
                "widget-editor-workflow",
                t("dashboard.workflowSource"),
                k,
                control,
            ));
        // React's `{onOpenWorkflowEditor && draft.workflowId && …}`.
        if self.on_open_workflow_editor.is_some() && !self.state.draft.workflow_id.is_empty() {
            block = block.child(self.open_workflow_editor_button(k));
        }
        block
    }

    /// React's read-only `Input value={draft.workflowId} readOnly
    /// className="font-mono text-xs"` — a static mono box (Rune fields
    /// without `on_edit` cannot take a caret).
    fn workflow_readonly_field(&self, k: &Kit) -> El {
        row()
            .key("widget-editor-workflow-readonly")
            .w_full()
            .h(INPUT_H)
            .items_center()
            .px(READONLY_PX)
            .radius(READONLY_RADIUS)
            .border(1.0, k.c.border_base)
            .fill(k.c.surface_weak) // React bg-surface-inset
            .child(
                text(self.state.workflow_readonly_edit.text.clone())
                    .font(MONO_READONLY)
                    .color(k.c.text_base)
                    .truncate(),
            )
    }

    /// The open-workflow-editor link: `variant="ghost" className="h-7 px-2
    /// text-xs"`, `data-testid="widget-open-workflow-editor"`.
    fn open_workflow_editor_button(&self, k: &Kit) -> El {
        let mut button = Button::new(t("dashboard.openWorkflowEditor"))
            .variant(ButtonVariant::Ghost)
            .control_size(ControlSize::Large); // React h-7 (the area mapping)
        if let Some(open) = &self.on_open_workflow_editor {
            let open = Rc::clone(open);
            button = button.on_press(move |s: &mut S| open(s));
        }
        button.build(k).key(OPEN_WF_KEY)
    }

    // ---- the refresh policy ----------------------------------------------------

    /// The refresh-mode preset select's label block.
    fn refresh_mode_block(&self, k: &Kit, layer: &mut Vec<El>) -> El {
        let options: Vec<(String, String)> = RefreshPreset::ALL
            .iter()
            .map(|preset| (t(preset.label_key()).to_owned(), preset.as_str().to_owned()))
            .collect();
        // React's `value`: the forced custom flag, else the derived preset.
        let current = if self.state.force_custom_refresh {
            RefreshPreset::Custom.as_str().to_owned()
        } else {
            refresh_policy_to_preset(
                self.state.draft.refresh.mode,
                self.state.draft.refresh.refresh_sec,
            )
            .as_str()
            .to_owned()
        };
        let select = self.select_field(
            "widget-editor-refresh-mode-select",
            options,
            Some(current.as_str()),
            None,
            None,
            WidgetEditorSelect::RefreshPreset,
            |state, value| state.set_refresh_preset(RefreshPreset::from_str(value)),
            k,
            layer,
        );
        label_block("widget-editor-refresh-mode", t("dashboard.refreshMode"), k, select)
    }

    /// The interval block (`space-y-1`): the caption, the value/unit row,
    /// the minimum hint, and (while the policy warns) the amber line.
    fn interval_block(&self, k: &Kit, dark: bool, layer: &mut Vec<El>) -> El {
        let (unit, _) = self.state.derived_interval();
        let mut block = col()
            .key("widget-editor-refresh-interval")
            .w_full()
            .gap(LABEL_GAP)
            .child(
                text(t("dashboard.refreshInterval"))
                    .key("widget-editor-refresh-interval-label")
                    .font(type_xs())
                    .color(k.c.text_muted),
            )
            .child(
                row()
                    .key("widget-editor-refresh-interval-row")
                    .items_center()
                    .gap(ROW_GAP)
                    .child(self.interval_value_field(dark))
                    .child(self.interval_unit_select(unit, k, layer)),
            )
            .child(
                // t('dashboard.refreshSecHint', { min: MIN_REFRESH_SEC })
                text(t("dashboard.refreshSecHint").replace("{min}", &MIN_REFRESH_SEC.to_string()))
                    .key("widget-editor-interval-hint")
                    .font(type_hint())
                    .color(k.c.text_muted),
            );
        // React's `{refreshWarn && …}` amber line.
        if should_warn_refresh_sec(&self.state.draft.refresh) {
            block = block.child(
                // t('dashboard.refreshSecWarn', { sec: REFRESH_WARN_BELOW_SEC })
                text(
                    t("dashboard.refreshSecWarn")
                        .replace("{sec}", &REFRESH_WARN_BELOW_SEC.to_string()),
                )
                .key(WARN_KEY)
                .font(type_hint())
                .color(palette::alert_amber(dark)), // React text-amber-500
            );
        }
        block
    }

    /// The interval value `Input type="number" min={1}` at `w-24`.
    fn interval_value_field(&self, dark: bool) -> El {
        let mut field = TextField::new("widget-editor-interval-value", &self.state.interval_edit)
            .text_style(MONO_INPUT)
            .height(INPUT_H)
            .width(INTERVAL_VALUE_W);
        match &self.state_fn {
            Some(state_fn) => {
                let edit_fn = Rc::clone(state_fn);
                field = field.on_edit(move |s: &mut S| &mut edit_fn(s).interval_edit);
                let change_fn = Rc::clone(state_fn);
                field = field
                    .on_change(move |s: &mut S, _tx: &mut Tx| change_fn(s).apply_interval_text());
            }
            None => field = field.disabled(true),
        }
        field.build(dark)
    }

    /// The unit `Select` at `w-28`, over the derived unit.
    fn interval_unit_select(&self, unit: IntervalUnit, k: &Kit, layer: &mut Vec<El>) -> El {
        let options: Vec<(String, String)> = IntervalUnit::ALL
            .iter()
            .map(|unit| (t(unit.label_key()).to_owned(), unit.as_str().to_owned()))
            .collect();
        self.select_field(
            "widget-editor-interval-unit",
            options,
            Some(unit.as_str()),
            None,
            Some(INTERVAL_UNIT_W),
            WidgetEditorSelect::IntervalUnit,
            |state, value| state.set_interval_unit(IntervalUnit::from_str(value)),
            k,
            layer,
        )
    }

    // ---- the monitor card ------------------------------------------------------

    /// The T146 card, mounted under React's gate (the saved widget's
    /// interval mode; see [`super::WidgetEditorDrawerState::monitor_card_pair`]).
    fn monitor_card(&self, dark: bool) -> Option<El> {
        self.state.monitor_card_pair()?;
        let mut card = MonitorScheduleAuthorization::<S>::new(&self.state.monitor, self.now_ms)
            .dirty(self.state.is_dirty());
        if let Some(authorize) = &self.on_monitor_authorize {
            let authorize = Rc::clone(authorize);
            card = card.on_authorize(move |s: &mut S| authorize(s));
        }
        if let Some(revoke) = &self.on_monitor_revoke {
            let revoke = Rc::clone(revoke);
            card = card.on_revoke(move |s: &mut S| revoke(s));
        }
        Some(card.build(dark))
    }

    // ---- the rest of the form --------------------------------------------------

    /// The enabled checkbox row: `flex items-center gap-2 text-xs text-fg`.
    fn enabled_row(&self, k: &Kit) -> El {
        let mut checkbox =
            Checkbox::new("widget-editor-enabled", self.state.draft.enabled)
                .label(t("dashboard.widgetEnabled"));
        if let Some(state_fn) = &self.state_fn {
            let state_fn = Rc::clone(state_fn);
            checkbox = checkbox.on_toggle(move |s: &mut S| {
                let enabled = !state_fn(s).draft.enabled;
                state_fn(s).set_enabled(enabled);
            });
        }
        checkbox.build(k)
    }

    /// The default-view select's label block.
    fn view_mode_block(&self, k: &Kit, layer: &mut Vec<El>) -> El {
        let options = vec![
            (t("dashboard.viewChart").to_owned(), TileViewMode::Chart.as_str().to_owned()),
            (t("dashboard.viewTable").to_owned(), TileViewMode::Table.as_str().to_owned()),
        ];
        let select = self.select_field(
            "widget-editor-view-mode-select",
            options,
            Some(self.state.draft.view_mode.as_str()),
            None,
            None,
            WidgetEditorSelect::ViewMode,
            |state, value| state.set_view_mode(TileViewMode::from_str(value)),
            k,
            layer,
        );
        label_block("widget-editor-view-mode", t("dashboard.defaultView"), k, select)
    }

    /// The chart-type select's label block — the raw React labels
    /// (`bar` / `line` / …, never localized).
    fn chart_type_block(&self, k: &Kit, layer: &mut Vec<El>) -> El {
        let options: Vec<(String, String)> = CHART_TYPES
            .iter()
            .map(|ct| (ct.as_str().to_owned(), ct.as_str().to_owned()))
            .collect();
        let current = self
            .state
            .draft
            .chart_config
            .as_ref()
            .map(|config| config.chart_type.as_str().to_owned());
        let select = self.select_field(
            "widget-editor-chart-type-select",
            options,
            current.as_deref(),
            None,
            None,
            WidgetEditorSelect::ChartType,
            |state, value| state.set_chart_type(ChartType::from_str(value)),
            k,
            layer,
        );
        label_block("widget-editor-chart-type", t("dashboard.chartType"), k, select)
    }

    /// The y-axes `Input` (placeholder `v, count`) — React's
    /// `value={yAxes.join(', ')}` over the split/trim/filter parse.
    fn y_axes_block(&self, k: &Kit, dark: bool) -> El {
        let mut field = TextField::new("widget-editor-y-axes-field", &self.state.y_axes_edit)
            .text_style(MONO_INPUT)
            .height(INPUT_H)
            .w_full()
            .placeholder("v, count");
        match &self.state_fn {
            Some(state_fn) => {
                let edit_fn = Rc::clone(state_fn);
                field = field.on_edit(move |s: &mut S| &mut edit_fn(s).y_axes_edit);
                let change_fn = Rc::clone(state_fn);
                field = field
                    .on_change(move |s: &mut S, _tx: &mut Tx| change_fn(s).apply_y_axes_text());
            }
            None => field = field.disabled(true),
        }
        label_block("widget-editor-y-axes", t("dashboard.yAxis"), k, field.build(dark))
    }

    /// The alert card: `space-y-3 rounded-md border border-edge p-3` with
    /// the enable checkbox and (while a rule is armed) the column /
    /// operator / threshold fields.
    fn alert_card(&self, k: &Kit, dark: bool, layer: &mut Vec<El>) -> El {
        let mut checkbox = Checkbox::new("widget-editor-alert-enabled", self.state.draft.alert.is_some())
            .label(t("dashboard.alertEnabled"));
        if let Some(state_fn) = &self.state_fn {
            let state_fn = Rc::clone(state_fn);
            checkbox = checkbox.on_toggle(move |s: &mut S| {
                let enabled = state_fn(s).draft.alert.is_none();
                state_fn(s).set_alert_enabled(enabled);
            });
        }
        let mut card = col()
            .key("widget-editor-alert")
            .w_full()
            .gap(ALERT_GAP) // space-y-3
            .radius(k.radius.md) // rounded-md
            .border(1.0, k.c.border_base) // border border-edge
            .padding(ALERT_PAD) // p-3
            .child(checkbox.build(k));
        // React's `{draft.alert && …}` fields block.
        if self.state.draft.alert.is_some() {
            card = card.child(self.alert_fields(k, dark, layer));
        }
        card
    }

    /// The armed rule's `space-y-3` fields block.
    fn alert_fields(&self, k: &Kit, dark: bool, layer: &mut Vec<El>) -> El {
        col()
            .key("widget-editor-alert-fields")
            .w_full()
            .gap(ALERT_GAP) // space-y-3
            .child(self.alert_column_block(k, dark))
            .child(self.alert_operator_block(k, layer))
            .child(self.alert_threshold_block(k, dark))
    }

    /// The alert metric column `Input` (placeholder `v`).
    fn alert_column_block(&self, k: &Kit, dark: bool) -> El {
        let mut field = TextField::new(
            "widget-editor-alert-column-field",
            &self.state.alert_column_edit,
        )
        .text_style(MONO_INPUT)
        .height(INPUT_H)
        .w_full()
        .placeholder("v");
        match &self.state_fn {
            Some(state_fn) => {
                let edit_fn = Rc::clone(state_fn);
                field = field.on_edit(move |s: &mut S| &mut edit_fn(s).alert_column_edit);
                let change_fn = Rc::clone(state_fn);
                field = field.on_change(move |s: &mut S, _tx: &mut Tx| {
                    change_fn(s).apply_alert_column_text();
                });
            }
            None => field = field.disabled(true),
        }
        label_block("widget-editor-alert-column", t("dashboard.alertColumn"), k, field.build(dark))
    }

    /// The operator select — the raw React labels (`>`, `>=`, …).
    fn alert_operator_block(&self, k: &Kit, layer: &mut Vec<El>) -> El {
        let options: Vec<(String, String)> = ALERT_OPS
            .iter()
            .map(|op| (op.as_str().to_owned(), op.as_str().to_owned()))
            .collect();
        let current = self
            .state
            .draft
            .alert
            .as_ref()
            .map(|alert| alert.op.as_str().to_owned());
        let select = self.select_field(
            "widget-editor-alert-operator-select",
            options,
            current.as_deref(),
            None,
            None,
            WidgetEditorSelect::AlertOperator,
            |state, value| state.set_alert_operator(AlertOperatorVm::from_str(value)),
            k,
            layer,
        );
        label_block("widget-editor-alert-operator", t("dashboard.alertOperator"), k, select)
    }

    /// The threshold `Input type="number"`.
    fn alert_threshold_block(&self, k: &Kit, dark: bool) -> El {
        let mut field = TextField::new(
            "widget-editor-alert-threshold-field",
            &self.state.threshold_edit,
        )
        .text_style(MONO_INPUT)
        .height(INPUT_H)
        .w_full();
        match &self.state_fn {
            Some(state_fn) => {
                let edit_fn = Rc::clone(state_fn);
                field = field.on_edit(move |s: &mut S| &mut edit_fn(s).threshold_edit);
                let change_fn = Rc::clone(state_fn);
                field = field.on_change(move |s: &mut S, _tx: &mut Tx| {
                    change_fn(s).apply_threshold_text();
                });
            }
            None => field = field.disabled(true),
        }
        label_block("widget-editor-alert-threshold", t("dashboard.alertThreshold"), k, field.build(dark))
    }

    // ---- the shared select -----------------------------------------------------

    /// One kit select over `(label, value)` pairs. `apply` is the value
    /// applier the choose handler runs through the state projection; every
    /// `set_*` applier closes the listbox, as React's `ui/Select` does.
    /// Without the projection the handlers are no-ops — the presses land
    /// nothing (the tile precedent).
    fn select_field(
        &self,
        key: &str,
        options: Vec<(String, String)>,
        current: Option<&str>,
        placeholder: Option<&str>,
        width: Option<f32>,
        which: WidgetEditorSelect,
        apply: fn(&mut WidgetEditorDrawerState, &str),
        k: &Kit,
        layer: &mut Vec<El>,
    ) -> El {
        let labels: Vec<&str> = options.iter().map(|(label, _)| label.as_str()).collect();
        let values: Vec<&str> = options.iter().map(|(_, value)| value.as_str()).collect();
        // The trigger shows the MATCHED OPTION'S LABEL, not the wire value —
        // React's `ui/Select` renders the option label for the current
        // value (the batch_export / history_filter_bar ruling). An unmatched
        // value falls back to showing itself.
        let current_label = current.map(|value| {
            options
                .iter()
                .find(|(_, v)| v == value)
                .map(|(label, _)| label.as_str())
                .unwrap_or(value)
                .to_owned()
        });
        let mut select = Select::new(key, &labels)
            .values(&values)
            .current(current_label.as_deref())
            .open(self.state.open_select == Some(which));
        if let Some(placeholder) = placeholder {
            select = select.placeholder(placeholder);
        }
        if let Some(width) = width {
            select = select.width(width);
        }
        // The kit's contract: an enabled select must be able to open (the
        // handlers no-op without the projection).
        let toggle = self.state_fn.clone();
        select = select.on_toggle(move |s: &mut S| {
            if let Some(state_fn) = &toggle {
                state_fn(s).toggle_select(which);
            }
        });
        let close = self.state_fn.clone();
        select = select.on_close(move |s: &mut S| {
            if let Some(state_fn) = &close {
                state_fn(s).close_select();
            }
        });
        let choose = self.state_fn.clone();
        select = select.on_choose(move |s: &mut S, value: &str, _tx: &mut Tx| {
            if let Some(state_fn) = &choose {
                apply(state_fn(s), value);
            }
        });
        select.build(k, layer)
    }
}

/// React's `<label className="block space-y-1">`: the muted `text-xs`
/// caption above the control (the caption carries `{key}-label` so audits
/// can read it).
fn label_block(key: &str, label: &str, k: &Kit, control: El) -> El {
    col()
        .key(key)
        .w_full()
        .gap(LABEL_GAP) // space-y-1
        .child(
            text(label)
                .key(format!("{key}-label"))
                .font(type_xs())
                .color(k.c.text_muted), // text-fg-muted
        )
        .child(control)
}
