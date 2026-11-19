//! The state half of the T201 `WidgetEditorDrawer` port: the alert / draft
//! view models and [`WidgetEditorDrawerState`] — every `useState` of the
//! React component, the open-time props the dirty guard and the T146
//! monitor card read, the `TextEdit` editing surfaces, and the nested card
//! state. Every transition is a method; the host polls `needs_resync`,
//! applies `begin_open`, then delivers the connections and the SQL editor
//! seam's text exactly where React's effect and `onChange` handlers fired.

use rune::TextEdit;

use crate::components::chart::chart_canvas::{ChartConfig, ChartType};
use crate::views::dashboard::chart_widget_tile::TileViewMode;
use crate::views::dashboard::dashboard_panel::{
    HiddenSqlVm, PanelWidgetVm, RefreshModeVm, RefreshPolicyVm, WidgetLayoutVm,
};
use crate::views::dashboard::monitor_schedule_authorization::MonitorScheduleAuthorizationState;

use super::refresh::{
    IntervalUnit, RefreshPreset, clamp_refresh_sec, js_number, js_number_or_one,
    normalize_refresh_policy, refresh_policy_to_preset, sec_to_unit_and_value, MIN_REFRESH_SEC,
};

// ---- View models ------------------------------------------------------------

/// React `AlertOperator` (`'>' | '>=' | '<' | '<=' | '==' | '!=').
#[allow(dead_code)] // reusable port; wired by the dashboard host track
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum AlertOperatorVm {
    Gt,
    Gte,
    Lt,
    Lte,
    Eq,
    Neq,
}

#[allow(dead_code)] // reusable port; wired by the dashboard host track
impl AlertOperatorVm {
    /// The wire spelling (also the select's label, React's `ALERT_OPS`).
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Gt => ">",
            Self::Gte => ">=",
            Self::Lt => "<",
            Self::Lte => "<=",
            Self::Eq => "==",
            Self::Neq => "!=",
        }
    }

    /// Parse the wire spelling; unknown values fall back to React's default
    /// (`>`).
    pub(crate) fn from_str(value: &str) -> Self {
        match value {
            ">=" => Self::Gte,
            "<" => Self::Lt,
            "<=" => Self::Lte,
            "==" => Self::Eq,
            "!=" => Self::Neq,
            _ => Self::Gt,
        }
    }
}

/// React `AlertMetric` — `kind` / `agg` ride along unedited (the form only
/// writes `column`).
#[allow(dead_code)] // reusable port; wired by the dashboard host track
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct AlertMetricVm {
    /// `'column' | 'aggregation'` (the wire spelling).
    pub(crate) kind: String,
    pub(crate) column: String,
    /// `'last' | 'max' | 'min' | 'avg' | 'sum'` (`None` is React's `undefined`).
    pub(crate) agg: Option<String>,
}

/// React `AlertRule`; `cooldownSec` / `channels` ride along unedited.
#[allow(dead_code)] // reusable port; wired by the dashboard host track
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct AlertRuleVm {
    pub(crate) metric: AlertMetricVm,
    pub(crate) op: AlertOperatorVm,
    pub(crate) threshold: f64,
    pub(crate) cooldown_sec: u32,
    pub(crate) channels: Vec<String>,
}

/// React's `DEFAULT_ALERT`.
#[allow(dead_code)] // reusable port; wired by the dashboard host track
pub(crate) fn default_alert() -> AlertRuleVm {
    AlertRuleVm {
        metric: AlertMetricVm {
            kind: "column".to_owned(),
            column: String::new(),
            agg: None,
        },
        op: AlertOperatorVm::Gt,
        threshold: 0.0,
        cooldown_sec: 300,
        channels: vec!["app".to_owned()],
    }
}

/// The drawer's draft of React's `DashboardWidget` — the panel's
/// `PanelWidgetVm` fields plus the `alert` rule T112 left out (the tile
/// never reads it). This is both the `widget` prop's shape (the host
/// converts) and the draft's.
#[allow(dead_code)] // reusable port; wired by the dashboard host track
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct WidgetDraftVm {
    pub(crate) id: String,
    pub(crate) title: String,
    pub(crate) workflow_id: String,
    pub(crate) view_mode: TileViewMode,
    pub(crate) chart_config: Option<ChartConfig>,
    pub(crate) layout: WidgetLayoutVm,
    pub(crate) refresh: RefreshPolicyVm,
    pub(crate) enabled: bool,
    pub(crate) alert: Option<AlertRuleVm>,
}

#[allow(dead_code)] // reusable port; wired by the dashboard host track
impl WidgetDraftVm {
    /// From the panel's widget flavour (the host supplies the alert the
    /// panel does not carry).
    pub(crate) fn from_panel(widget: &PanelWidgetVm, alert: Option<AlertRuleVm>) -> Self {
        Self {
            id: widget.id.clone(),
            title: widget.title.clone(),
            workflow_id: widget.workflow_id.clone(),
            view_mode: widget.view_mode,
            chart_config: widget.chart_config.clone(),
            layout: widget.layout,
            refresh: widget.refresh.clone(),
            enabled: widget.enabled,
            alert,
        }
    }

    /// Back to the panel's widget flavour (the alert travels beside it).
    pub(crate) fn to_panel(&self) -> PanelWidgetVm {
        PanelWidgetVm {
            id: self.id.clone(),
            title: self.title.clone(),
            workflow_id: self.workflow_id.clone(),
            view_mode: self.view_mode,
            chart_config: self.chart_config.clone(),
            layout: self.layout,
            refresh: self.refresh.clone(),
            enabled: self.enabled,
        }
    }
}

/// React's `emptyDraft()` — the fallback when `widget` is null (the panel
/// always passes its own draft, whose layout `nextWidgetLayout` placed).
/// The `crypto.randomUUID()` id is the host's (`new_widget_id`).
#[allow(dead_code)] // reusable port; wired by the dashboard host track
pub(crate) fn empty_draft(new_widget_id: &str) -> WidgetDraftVm {
    WidgetDraftVm {
        id: new_widget_id.to_owned(),
        title: String::new(),
        workflow_id: String::new(),
        view_mode: TileViewMode::Chart,
        // `{ ...DEFAULT_CHART_CONFIG, yAxes: ['v'] }`.
        chart_config: Some(ChartConfig {
            y_axes: vec!["v".to_owned()],
            ..Default::default()
        }),
        layout: WidgetLayoutVm { x: 0, y: 0, w: 6, h: 4 },
        refresh: RefreshPolicyVm::default(), // DEFAULT_REFRESH { mode: 'manual' }
        enabled: true,
        alert: None,
    }
}

/// The hidden-SQL drawer's seed when the prop is absent (React's inline
/// default).
#[allow(dead_code)] // reusable port; wired by the dashboard host track
pub(crate) fn default_hidden_sql() -> HiddenSqlVm {
    HiddenSqlVm { connection_id: String::new(), sql: "SELECT 1 AS v".to_owned() }
}

/// The slice of the connection-store entry the drawer reads (id + name; the
/// SQL editor's `databaseType` is the host's seam-side lookup).
#[allow(dead_code)] // reusable port; wired by the dashboard host track
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct ConnectionOptionVm {
    pub(crate) id: String,
    pub(crate) name: String,
}

/// `handleSave`'s payload: the normalized widget (trimmed title, clamped
/// refresh) plus its alert rule, and the hidden-SQL pair when the drawer
/// edits a hidden SQL widget.
#[allow(dead_code)] // reusable port; wired by the dashboard host track
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct WidgetSaveVm {
    pub(crate) widget: PanelWidgetVm,
    pub(crate) alert: Option<AlertRuleVm>,
    pub(crate) hidden_sql: Option<HiddenSqlVm>,
}

/// Which of the drawer's listboxes is open (one at a time — React's
/// `ui/Select` keeps its own open state; closing on a choice is the choose
/// handler's job, reproduced by every `set_*` method below).
#[allow(dead_code)] // reusable port; wired by the dashboard host track
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum WidgetEditorSelect {
    Connection,
    Workflow,
    RefreshPreset,
    IntervalUnit,
    ViewMode,
    ChartType,
    AlertOperator,
}

// ---- State ------------------------------------------------------------------

/// Every `useState` of the React component, the open-time props the dirty
/// guard and the monitor card read, the `TextEdit` editing surfaces, and
/// the nested T146 card state — kept in the owning state `S`.
#[allow(dead_code)] // reusable port; wired by the dashboard host track
#[derive(Clone, Debug)]
pub(crate) struct WidgetEditorDrawerState {
    /// React's `open` prop; `false` builds nothing at all.
    pub(crate) open: bool,
    /// React's `widget` prop at open — the effect's seed, the dirty guard's
    /// reference and the monitor card's id pair. `None` is React's `null`.
    pub(crate) original: Option<WidgetDraftVm>,
    /// React's `hiddenSql` prop at open; `None` when the drawer edits a
    /// workflow-bound widget ([`Self::hidden_mode`]).
    pub(crate) original_hidden_sql: Option<HiddenSqlVm>,
    /// React's `isNew` prop.
    pub(crate) is_new: bool,
    /// React's `dashboardId` prop, landed at open (the panel never switches
    /// boards under an open editor).
    pub(crate) dashboard_id: Option<String>,
    /// React's `draft`.
    pub(crate) draft: WidgetDraftVm,
    /// React's `hiddenSql`.
    pub(crate) hidden_sql: HiddenSqlVm,
    /// React's `forceCustomRefresh`.
    pub(crate) force_custom_refresh: bool,
    // ---- the controlled inputs' TextEdits ----
    pub(crate) title_edit: TextEdit,
    /// The read-only workflow field's display (seeded at open; the branch
    /// with no workflows cannot change the id).
    pub(crate) workflow_readonly_edit: TextEdit,
    pub(crate) interval_edit: TextEdit,
    pub(crate) y_axes_edit: TextEdit,
    pub(crate) alert_column_edit: TextEdit,
    pub(crate) threshold_edit: TextEdit,
    /// The one open listbox.
    pub(crate) open_select: Option<WidgetEditorSelect>,
    /// The connection store's `connections` entry.
    pub(crate) connections: Vec<ConnectionOptionVm>,
    /// The nested T146 card state.
    pub(crate) monitor: MonitorScheduleAuthorizationState,
}

impl Default for WidgetEditorDrawerState {
    fn default() -> Self {
        Self {
            open: false,
            original: None,
            original_hidden_sql: None,
            is_new: false,
            dashboard_id: None,
            draft: empty_draft(""),
            hidden_sql: default_hidden_sql(),
            force_custom_refresh: false,
            title_edit: TextEdit::new(String::new()),
            workflow_readonly_edit: TextEdit::new(String::new()),
            interval_edit: TextEdit::new(String::new()),
            y_axes_edit: TextEdit::new(String::new()),
            alert_column_edit: TextEdit::new(String::new()),
            threshold_edit: TextEdit::new(String::new()),
            open_select: None,
            connections: Vec::new(),
            monitor: MonitorScheduleAuthorizationState::default(),
        }
    }
}

#[allow(dead_code)] // reusable port; wired by the dashboard host track
impl WidgetEditorDrawerState {
    /// Whether the drawer edits a hidden SQL widget (React's
    /// `hiddenSqlProp != null`).
    pub(crate) fn hidden_mode(&self) -> bool {
        self.original_hidden_sql.is_some()
    }

    /// React's dep-array check for the open effect (`[open, widget,
    /// hiddenSqlProp]`, mount included): the drawer should be open but the
    /// state's seed differs — a closed state, or a different widget /
    /// hidden-SQL pair (object identity in React; value equality here, the
    /// T146/T175 stance).
    pub(crate) fn needs_resync(
        &self,
        open: bool,
        widget: Option<&WidgetDraftVm>,
        hidden_sql: Option<&HiddenSqlVm>,
    ) -> bool {
        open && (!self.open
            || self.original.as_ref() != widget
            || self.original_hidden_sql.as_ref() != hidden_sql)
    }

    /// The open effect's synchronous half: re-seed the drafts, reset the
    /// custom flag and close any open listbox (the host then fetches the
    /// connections and delivers them). `new_widget_id` is the
    /// `crypto.randomUUID()` of React's `emptyDraft()`, used only when
    /// `widget` is `None`.
    pub(crate) fn begin_open(
        &mut self,
        widget: Option<&WidgetDraftVm>,
        hidden_sql: Option<&HiddenSqlVm>,
        is_new: bool,
        dashboard_id: Option<&str>,
        new_widget_id: &str,
    ) {
        self.open = true;
        self.is_new = is_new;
        self.dashboard_id = dashboard_id.filter(|id| !id.is_empty()).map(str::to_owned);
        self.original = widget.cloned();
        self.original_hidden_sql = hidden_sql.cloned();
        self.draft = widget.cloned().unwrap_or_else(|| empty_draft(new_widget_id));
        self.hidden_sql = hidden_sql.cloned().unwrap_or_else(default_hidden_sql);
        self.force_custom_refresh = false;
        self.open_select = None;
        self.title_edit = TextEdit::new(self.draft.title.clone());
        self.workflow_readonly_edit = TextEdit::new(self.draft.workflow_id.clone());
        self.sync_interval_edit();
        self.sync_y_axes_edit();
        self.sync_alert_edits();
    }

    /// The `open` prop turning false (header close, footer cancel — React's
    /// scrim has no handler). The drafts survive exactly as React's do; the
    /// next open's effect re-seeds them.
    pub(crate) fn close(&mut self) {
        self.open = false;
        self.open_select = None;
    }

    /// The `fetchConnections` answer (the store's `connections` entry —
    /// token-free, like the subscription React reads).
    pub(crate) fn deliver_connections(&mut self, connections: Vec<ConnectionOptionVm>) {
        self.connections = connections;
    }

    // ---- field transitions (React's onChange handlers) ----

    /// The title input's `onChange` (run from the field's `on_change`, after
    /// the kit lands the text).
    pub(crate) fn apply_title_text(&mut self) {
        self.draft.title = self.title_edit.text.clone();
    }

    /// The connection select's `onChange` (hidden-SQL mode).
    pub(crate) fn set_hidden_connection(&mut self, id: &str) {
        self.hidden_sql.connection_id = id.to_owned();
        self.open_select = None;
    }

    /// The SQL editor seam's text delivery (React's `SqlEditor onChange`).
    pub(crate) fn set_hidden_sql(&mut self, sql: &str) {
        self.hidden_sql.sql = sql.to_owned();
    }

    /// The workflow select's `onChange`.
    pub(crate) fn set_workflow(&mut self, id: &str) {
        self.draft.workflow_id = id.to_owned();
        self.open_select = None;
    }

    /// The refresh preset select's `onChange` — React's four literals plus
    /// the custom arm's clamp of the standing seconds.
    pub(crate) fn set_refresh_preset(&mut self, preset: RefreshPreset) {
        self.force_custom_refresh = preset == RefreshPreset::Custom;
        self.draft.refresh = match preset {
            RefreshPreset::Manual => {
                RefreshPolicyVm { mode: RefreshModeVm::Manual, refresh_sec: None }
            }
            RefreshPreset::OnOpen => {
                RefreshPolicyVm { mode: RefreshModeVm::OnOpen, refresh_sec: None }
            }
            RefreshPreset::Daily => {
                RefreshPolicyVm { mode: RefreshModeVm::Interval, refresh_sec: Some(86400) }
            }
            RefreshPreset::Hourly => {
                RefreshPolicyVm { mode: RefreshModeVm::Interval, refresh_sec: Some(3600) }
            }
            RefreshPreset::Custom => RefreshPolicyVm {
                mode: RefreshModeVm::Interval,
                refresh_sec: Some(clamp_refresh_sec(
                    self.draft.refresh.refresh_sec.unwrap_or(MIN_REFRESH_SEC),
                )),
            },
        };
        self.open_select = None;
        self.sync_interval_edit();
    }

    /// The interval value input's `onChange`: `val = Math.max(1,
    /// Number(v) || 1)` against the currently derived unit, then
    /// `refreshSec = clampRefreshSec(val * multiplier)`. The fractional
    /// tail a React number input can deliver truncates to the whole second
    /// (`RefreshPolicyVm`'s `u32`, T112's stance).
    pub(crate) fn apply_interval_text(&mut self) {
        let val = js_number_or_one(&self.interval_edit.text);
        let (unit, _) = self.derived_interval();
        let total = (val * unit.multiplier() as f64).max(MIN_REFRESH_SEC as f64) as u32;
        self.draft.refresh.refresh_sec = Some(total);
        // React's controlled input re-derives its display; one round
        // converges (the clamp of a derived value is the value itself).
        self.sync_interval_edit();
    }

    /// The unit select's `onChange`: the derived value re-based onto the
    /// new unit, clamped (45 seconds chosen as minutes clamps to 30, and
    /// both the field and the select re-derive — React's do too).
    pub(crate) fn set_interval_unit(&mut self, unit: IntervalUnit) {
        let (_, value) = self.derived_interval();
        let total = clamp_refresh_sec(value.saturating_mul(unit.multiplier()));
        self.draft.refresh.refresh_sec = Some(total);
        self.open_select = None;
        self.sync_interval_edit();
    }

    /// The enabled checkbox's `onChange`.
    pub(crate) fn set_enabled(&mut self, enabled: bool) {
        self.draft.enabled = enabled;
    }

    /// The view-mode select's `onChange`.
    pub(crate) fn set_view_mode(&mut self, mode: TileViewMode) {
        self.draft.view_mode = mode;
        self.open_select = None;
    }

    /// The chart-type select's `onChange` — over the standing config, or
    /// the default when the draft carries none (React's
    /// `d.chartConfig ?? DEFAULT_CHART_CONFIG`).
    pub(crate) fn set_chart_type(&mut self, chart_type: ChartType) {
        let config = self.draft.chart_config.clone().unwrap_or_default();
        self.draft.chart_config = Some(ChartConfig { chart_type, ..config });
        self.open_select = None;
    }

    /// The y-axes input's `onChange`: split on commas, trim, drop blanks —
    /// then the controlled value re-derives (`yAxes.join(', ')`), so a
    /// trailing comma vanishes as it is typed (React's does too).
    pub(crate) fn apply_y_axes_text(&mut self) {
        let axes: Vec<String> = self
            .y_axes_edit
            .text
            .split(',')
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_owned)
            .collect();
        if let Some(config) = self.draft.chart_config.as_mut() {
            config.y_axes = axes;
        }
        self.sync_y_axes_edit();
    }

    /// The alert checkbox's `onChange`: on keeps a standing rule or seeds
    /// `DEFAULT_ALERT`; off clears it.
    pub(crate) fn set_alert_enabled(&mut self, enabled: bool) {
        self.draft.alert = if enabled {
            Some(self.draft.alert.clone().unwrap_or_else(default_alert))
        } else {
            None
        };
        self.sync_alert_edits();
    }

    /// The alert column input's `onChange` (the metric's `kind` / `agg`
    /// ride along, as React's spread keeps them).
    pub(crate) fn apply_alert_column_text(&mut self) {
        if let Some(alert) = self.draft.alert.as_mut() {
            alert.metric.column = self.alert_column_edit.text.clone();
        }
    }

    /// The operator select's `onChange`.
    pub(crate) fn set_alert_operator(&mut self, op: AlertOperatorVm) {
        if let Some(alert) = self.draft.alert.as_mut() {
            alert.op = op;
        }
        self.open_select = None;
    }

    /// The threshold input's `onChange`: JS `Number(text)` — blank text is
    /// 0, and the controlled value re-renders `String(threshold)` (a typed
    /// `3.` normalizes to `3`, React's does too).
    pub(crate) fn apply_threshold_text(&mut self) {
        if let Some(alert) = self.draft.alert.as_mut() {
            let n = js_number(&self.threshold_edit.text).unwrap_or(0.0);
            alert.threshold = n;
            let text = format!("{n}");
            if self.threshold_edit.text != text {
                self.threshold_edit.replace_text(text);
            }
        }
    }

    // ---- listbox plumbing ----

    /// A select trigger's press: flip the one open listbox.
    pub(crate) fn toggle_select(&mut self, which: WidgetEditorSelect) {
        self.open_select = if self.open_select == Some(which) { None } else { Some(which) };
    }

    /// Outside press / Escape while a listbox is open.
    pub(crate) fn close_select(&mut self) {
        self.open_select = None;
    }

    // ---- derivations ----

    /// The interval pair the value field and the unit select display.
    pub(crate) fn derived_interval(&self) -> (IntervalUnit, u32) {
        sec_to_unit_and_value(self.draft.refresh.refresh_sec.unwrap_or(MIN_REFRESH_SEC))
    }

    /// Whether the interval row is mounted: interval mode and (the custom
    /// flag or a non-preset seconds value) — React's gate verbatim.
    pub(crate) fn interval_row_visible(&self) -> bool {
        matches!(self.draft.refresh.mode, RefreshModeVm::Interval)
            && (self.force_custom_refresh
                || refresh_policy_to_preset(
                    self.draft.refresh.mode,
                    self.draft.refresh.refresh_sec,
                ) == RefreshPreset::Custom)
    }

    /// The monitor card's mount gate: `dashboardId && widget && !isNew &&
    /// widget.refresh.mode === 'interval'` — the **saved** widget's mode,
    /// not the draft's (React reads the `widget` prop).
    pub(crate) fn monitor_card_pair(&self) -> Option<(&str, &str)> {
        if self.is_new {
            return None;
        }
        let widget = self.original.as_ref()?;
        if !matches!(widget.refresh.mode, RefreshModeVm::Interval) {
            return None;
        }
        let dashboard_id = self.dashboard_id.as_deref().filter(|id| !id.is_empty())?;
        Some((dashboard_id, widget.id.as_str()))
    }

    /// The monitor card's `dirty` prop: the draft against the original
    /// widget, or the hidden-SQL pair against its original-or-default (a
    /// widget-less drawer is always dirty — React's `JSON.stringify(draft)
    /// !== JSON.stringify(null)`).
    pub(crate) fn is_dirty(&self) -> bool {
        let widget_dirty = match &self.original {
            Some(original) => &self.draft != original,
            None => true,
        };
        let original_hidden =
            self.original_hidden_sql.clone().unwrap_or_else(default_hidden_sql);
        widget_dirty || self.hidden_sql != original_hidden
    }

    /// `handleSave`'s payload: `None` while a guard blocks the save (a
    /// blank title, or a hidden-SQL drawer without a connection or blank
    /// SQL — React's early `return`), else the normalized widget (trimmed
    /// title, clamped refresh) with the alert rule and, in hidden-SQL
    /// mode, the pair.
    pub(crate) fn save_payload(&self) -> Option<WidgetSaveVm> {
        if self.draft.title.trim().is_empty() {
            return None;
        }
        if self.hidden_mode()
            && (self.hidden_sql.connection_id.is_empty() || self.hidden_sql.sql.trim().is_empty())
        {
            return None;
        }
        let mut widget = self.draft.to_panel();
        widget.title = self.draft.title.trim().to_owned();
        widget.refresh = normalize_refresh_policy(&self.draft.refresh);
        Some(WidgetSaveVm {
            widget,
            alert: self.draft.alert.clone(),
            hidden_sql: self.hidden_mode().then(|| self.hidden_sql.clone()),
        })
    }

    // ---- the controlled displays' re-seeds ----

    /// Park the interval field's text on the derived value (React's
    /// `value={secToUnitAndValue(...).value}`).
    fn sync_interval_edit(&mut self) {
        let (_, value) = self.derived_interval();
        let text = value.to_string();
        if self.interval_edit.text != text {
            self.interval_edit.replace_text(text);
        }
    }

    /// Park the y-axes field's text on the joined axes.
    fn sync_y_axes_edit(&mut self) {
        let joined = self
            .draft
            .chart_config
            .as_ref()
            .map_or_else(String::new, |config| config.y_axes.join(", "));
        if self.y_axes_edit.text != joined {
            self.y_axes_edit.replace_text(joined);
        }
    }

    /// Park the alert fields' texts (blank / 0 while no rule is armed).
    fn sync_alert_edits(&mut self) {
        let (column, threshold) = match &self.draft.alert {
            Some(alert) => (alert.metric.column.clone(), alert.threshold),
            None => (String::new(), 0.0),
        };
        if self.alert_column_edit.text != column {
            self.alert_column_edit.replace_text(column);
        }
        let threshold = format!("{threshold}");
        if self.threshold_edit.text != threshold {
            self.threshold_edit.replace_text(threshold);
        }
    }
}
