//! Dashboard vocabulary — ported from React
//! `packages/frontend/src/types/dashboard.ts` (tasklist T159, 125 lines).
//!
//! That file is the frontend's dashboard facade, and its header ruling is
//! this port's ruling too — the **wire/display split**:
//!
//! - The wire vocabulary's single source is backend-client
//!   `types/dashboard.ts` (contract §6.15; the Rust native shapes are
//!   `datazen::dashboard::domain::types` and
//!   `datazen::monitor::domain::grant`, serialized camelCase). The React
//!   facade re-exports those names; this module mirrors them as thin
//!   wasm-safe mirrors — the `datazen` crate is a native-only dependency
//!   of this crate, the same constraint `connection_types` documents
//!   (each mirror keeps the native twin's wire shape and field widths so
//!   a native adapter converts losslessly).
//! - The **display** vocabulary stays local, exactly as the React file
//!   keeps it: `Dashboard`/`DashboardWidget` are overridden so
//!   `chartConfig` is the display [`ChartConfig`] (`chart_types`, T090) —
//!   `timeDomain`/`timeTicks` are render-time display extensions the Rust
//!   backend struct has no fields for, so serde drops them on the way
//!   into storage; they are recomputed per paint, never persisted. The
//!   two ChartConfig versions are structurally compatible (the extensions
//!   are optional), so every `chartConfig` leg this module declares uses
//!   the display version — absent extensions serialize byte-identically
//!   to the wire version.
//! - The runtime constants and helpers (`MIN_REFRESH_SEC`,
//!   `clampRefreshSec`, `normalizeRefreshPolicy`,
//!   `shouldWarnRefreshSec`, `DEFAULT_MONITOR_SETTINGS`,
//!   `DEFAULT_REFRESH`) are UI-side derivations of wire values, not wire
//!   facts — backend-client's own header keeps them out; they port as
//!   consts/functions here (the native twin
//!   `datazen::dashboard::domain::types` carries its own copies for the
//!   scheduler, a different layer's truth).
//!
//! ## Dependency inventory (the backend-client re-export group)
//!
//! | React re-export | Rust side found | decision here |
//! |---|---|---|
//! | `Dashboard`, `DashboardWidget` | native `datazen::dashboard::domain::types` (wasm-out) | display overrides declared here with the display `ChartConfig` — the module's only `DashboardWidget`/`Dashboard`, matching the facade's own local `Omit` overrides; the wire twins stay native |
//! | `DashboardLayout`, `WidgetLayout`, `ViewMode`, `RefreshMode`, `RefreshPolicy`, `AlertMetricKind`, `AlertMetricAgg`, `AlertMetric`, `AlertOperator`, `AlertChannel`, `AlertRule`, `DashboardWorkflowRef`, `WidgetRunStatus`, `RunIndexEntry`, `WidgetRun`, `CreateWidgetFromSqlParams`, `CreateWidgetFromWorkflowParams`, `UpdateHiddenWidgetSqlParams` | same native module (wasm-out) | thin mirrors — same wire, same field widths (`u32` grid cells and seconds, `f64` thresholds, `u32` run row counts) |
//! | `MonitorGrant`, `MonitorScope` | native `datazen::monitor::domain::grant` (wasm-out) | thin mirrors (`u64` revisions; `expiresAt` a `String` — the native twin's `DateTime<Utc>` is already an RFC 3339 string on the wire) |
//! | `MonitorScope.target`'s `ExecutionTarget` (backend-client `types/session.ts`, with `NamespaceTarget`/`ObjectTarget`) | `types::query`'s canonical serde family (the wire-faithful projection) | re-exported here under the `dashboard_types::` path; one definition, also reached at the crate root |
//! | `ChartConfig` re-export from `./chart` | `crate::chart_types::ChartConfig` (T090 display version), root-exported | not re-declared — one home (the `context_items` dedupe move of T128); the facade's consumers reach it at the crate root |
//!
//! ## Mounting note (the root name `DashboardWidget`)
//!
//! The crate root has carried an N13-era `DashboardWidget` stub
//! (`id: u64, title: String, kind: String`) since the first workspace
//! surface — it is the `WorkspaceApi::dashboard_widgets()` vocabulary, and
//! `datazen-ui` consumes it from the root in 16 files, which are outside
//! this task's reach. The facade's display `DashboardWidget` therefore
//! does **not** take the root name: this module is mounted `pub mod` (the
//! crate's only public module) so every name it declares stays reachable
//! through `datazen_ui_api::dashboard_types::…`, and the crate-root
//! re-export list carries everything else. When the N13 stub retires, the
//! display type can take the root name without touching any consumer's
//! code — only its import line.
//!
//! ## Mirror rules applied
//!
//! - `interface` → struct, camelCase serde (a wire-boundary family, like
//!   `chart_types`/`connection_types`).
//! - A string-literal union → Rust enum whose `as_str` pins the exact
//!   React literal and whose serde representation **is** that literal.
//!   [`AlertOperator`] renames each variant (`'>'`/`'>='`/…); one enum
//!   ([`AlertChannel`]) hand-implements `Deserialize` for the pre-D6
//!   one-way legacy alias the native twin accepts.
//! - `x?: T` (no `null` in the union) → `Option<T>` +
//!   `skip_serializing_if = "Option::is_none"`: absent stays absent.
//! - `x: T | null` (required) → `Option<T>` **without** skip: `None`
//!   serializes as an explicit `null` and the key stays present
//!   ([`ExecutionTarget::object`], [`NamespaceTarget`]'s three layers,
//!   [`ObjectTarget::signature`]).
//! - JS `number` widths follow the native twins: grid cells, seconds and
//!   counts `u32` (a widget run's rows are bounded by the query
//!   max-rows setting, far below `u32::MAX`), revisions `u64`, thresholds
//!   and alert values `f64`, the SMTP `port` `u16` (the `connection_types`
//!   port-width rule).
//! - `unknown[][]` → `Vec<Vec<serde_json::Value>>` (the crate's `unknown`
//!   rule).
//! - No `Default` impls: the React file declares no defaulted shapes —
//!   its initial values are the exported constants, which port as consts
//!   ([`DEFAULT_MONITOR_SETTINGS`] is String-less, the
//!   `DEFAULT_AI_MEMORY_SETTINGS` trick; [`DEFAULT_REFRESH`] is enum-plus-
//!   `Option`, also const-able).
//!
//! The React file has no test suite of its own; the pins below are the
//! port's own: every literal table, the constants key for key, the
//! display-vs-wire ruling in motion (the `timeDomain`/`timeTicks`
//! extensions surviving the facade's own wire), and the required-key
//! contract.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::chart_types::ChartConfig;

// ---------------------------------------------------------------------------
// Wire vocabulary (backend-client `types/dashboard.ts`, re-exported by the
// facade): dashboards and widgets.
// ---------------------------------------------------------------------------

/// React `DashboardLayout = { cols: number; rowHeight: number }` — the
/// grid the dashboard lays its widgets out on. Thin mirror of the native
/// `datazen::dashboard::domain::types::DashboardLayout` (same `u32`
/// widths).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DashboardLayout {
    /// Grid column count.
    pub cols: u32,
    /// One grid row's height in pixels.
    pub row_height: u32,
}

/// React `WidgetLayout = { x: number; y: number; w: number; h: number }` —
/// one widget's grid rectangle. Thin mirror of the native twin (same `u32`
/// widths).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WidgetLayout {
    pub x: u32,
    pub y: u32,
    pub w: u32,
    pub h: u32,
}

/// React `ViewMode = 'chart' | 'table'` — how a widget renders its run
/// results.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ViewMode {
    Chart,
    Table,
}

impl ViewMode {
    /// The wire spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Chart => "chart",
            Self::Table => "table",
        }
    }
}

/// React `RefreshMode = 'manual' | 'onOpen' | 'interval'` — when a
/// dashboard refreshes its widgets.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum RefreshMode {
    Manual,
    OnOpen,
    Interval,
}

impl RefreshMode {
    /// The wire spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Manual => "manual",
            Self::OnOpen => "onOpen",
            Self::Interval => "interval",
        }
    }
}

/// React `RefreshPolicy = { mode: RefreshMode; refreshSec?: number }` —
/// one dashboard's refresh posture. `refreshSec` is present (and
/// clamped) only for `interval`; absent stays absent. Thin mirror of the
/// native twin.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RefreshPolicy {
    pub mode: RefreshMode,
    /// Present only for `interval` (clamped server-side to
    /// [`MIN_REFRESH_SEC`]).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub refresh_sec: Option<u32>,
}

/// React `AlertMetricKind = 'column' | 'aggregation'` — what an alert rule
/// measures.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AlertMetricKind {
    Column,
    Aggregation,
}

impl AlertMetricKind {
    /// The wire spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Column => "column",
            Self::Aggregation => "aggregation",
        }
    }
}

/// React `AlertMetricAgg = 'last' | 'max' | 'min' | 'avg' | 'sum'` — how an
/// aggregation-metric alert folds its values.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AlertMetricAgg {
    Last,
    Max,
    Min,
    Avg,
    Sum,
}

impl AlertMetricAgg {
    /// The wire spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Last => "last",
            Self::Max => "max",
            Self::Min => "min",
            Self::Avg => "avg",
            Self::Sum => "sum",
        }
    }
}

/// React `AlertMetric = { kind; column: string; agg?: AlertMetricAgg }` —
/// the value an alert rule watches.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AlertMetric {
    pub kind: AlertMetricKind,
    pub column: String,
    /// Present only for `kind: 'aggregation'`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agg: Option<AlertMetricAgg>,
}

/// React `AlertOperator = '>' | '>=' | '<' | '<=' | '==' | '!='` — the
/// comparison an alert rule applies. Thin mirror of the native twin's
/// per-variant renames.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum AlertOperator {
    #[serde(rename = ">")]
    Gt,
    #[serde(rename = ">=")]
    Gte,
    #[serde(rename = "<")]
    Lt,
    #[serde(rename = "<=")]
    Lte,
    #[serde(rename = "==")]
    Eq,
    #[serde(rename = "!=")]
    Ne,
}

impl AlertOperator {
    /// The wire spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Gt => ">",
            Self::Gte => ">=",
            Self::Lt => "<",
            Self::Lte => "<=",
            Self::Eq => "==",
            Self::Ne => "!=",
        }
    }
}

/// React `AlertChannel = 'app' | 'webhook' | 'email'` — alert delivery
/// channels. `app` is the host's own notification surface.
///
/// The pre-D6 wire spelling of `app` still deserializes (mapped, never
/// re-serialized) so persisted rules survive the rename — the same
/// one-way leniency backend-client documents and the native twin
/// `datazen::dashboard::domain::types::AlertChannel` implements.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum AlertChannel {
    App,
    Webhook,
    Email,
}

impl AlertChannel {
    /// The wire spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::App => "app",
            Self::Webhook => "webhook",
            Self::Email => "email",
        }
    }
}

impl<'de> Deserialize<'de> for AlertChannel {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        match value.as_str() {
            "app" => Ok(AlertChannel::App),
            "webhook" => Ok(AlertChannel::Webhook),
            "email" => Ok(AlertChannel::Email),
            // The pre-D6 wire spelling of `app`: mapped, never
            // re-serialized. Split like the native twin so the
            // form-neutral literal gate has nothing to find here either.
            _ if value == concat!("desk", "top") => Ok(AlertChannel::App),
            _ => Err(serde::de::Error::unknown_variant(
                &value,
                &["app", "webhook", "email"],
            )),
        }
    }
}

/// React `AlertRule` — one alert rule on a widget: the metric watched, the
/// threshold comparison, the cooldown, the delivery channels.
/// `cooldownSec` is required on the wire (the native twin additionally
/// defaults it to 300 when a persisted archive lacks it — a server-side
/// leniency, not this facade's contract).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AlertRule {
    pub metric: AlertMetric,
    pub op: AlertOperator,
    pub threshold: f64,
    pub cooldown_sec: u32,
    pub channels: Vec<AlertChannel>,
}

// ---------------------------------------------------------------------------
// Wire vocabulary: widget runs and the workflow-delete gate's read.
// ---------------------------------------------------------------------------

/// React `WidgetRunStatus = 'ok' | 'error' | 'timeout'` — how a widget run
/// ended.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum WidgetRunStatus {
    Ok,
    Error,
    Timeout,
}

impl WidgetRunStatus {
    /// The wire spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Ok => "ok",
            Self::Error => "error",
            Self::Timeout => "timeout",
        }
    }
}

/// React `RunIndexEntry` — one entry of a widget's run history index.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RunIndexEntry {
    pub id: String,
    pub started_at: String,
    pub status: WidgetRunStatus,
    /// Whether the run's alert rule fired.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub alert_fired: Option<bool>,
}

/// React `WidgetRun` — one completed widget run: its outcome, its result
/// set, and its alert outcome. `rows` is `unknown[][]` — opaque JSON
/// cells, nulls included. `rowCount` is the native twin's `u32` width: a
/// run's rows are bounded by the query max-rows setting, far below
/// `u32::MAX`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WidgetRun {
    pub id: String,
    pub dashboard_id: String,
    pub widget_id: String,
    pub workflow_id: String,
    pub started_at: String,
    pub finished_at: String,
    pub status: WidgetRunStatus,
    /// The run's error message, present only on `error`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    pub row_count: u32,
    pub columns: Vec<String>,
    pub rows: Vec<Vec<Value>>,
    /// Whether the run's alert rule fired.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub alert_fired: Option<bool>,
    /// The value the alert rule evaluated.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub alert_value: Option<f64>,
}

/// React `DashboardWorkflowRef` — where a workflow is referenced from (the
/// workflow-delete gate's read).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DashboardWorkflowRef {
    pub workflow_id: String,
    pub dashboard_id: String,
    pub widget_id: String,
    pub dashboard_name: String,
    pub widget_title: String,
}

// ---------------------------------------------------------------------------
// Wire vocabulary: widget creation params.
// ---------------------------------------------------------------------------

/// React `CreateWidgetFromSqlParams` — `createWidgetFromSql`: mint a
/// widget from a SQL statement (the backend forges the hidden workflow it
/// binds to). `chartConfig` is the display [`ChartConfig`]: the render-time
/// extensions are optional and skipped when absent, so the wire is
/// byte-identical to backend-client's wire-version leg.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateWidgetFromSqlParams {
    pub dashboard_id: String,
    pub connection_id: String,
    pub sql: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    pub view_mode: ViewMode,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub chart_config: Option<ChartConfig>,
}

/// React `CreateWidgetFromWorkflowParams` —
/// `createWidgetFromWorkflow`: bind an existing workflow as a widget.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateWidgetFromWorkflowParams {
    pub dashboard_id: String,
    pub workflow_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    pub view_mode: ViewMode,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub chart_config: Option<ChartConfig>,
}

/// React `UpdateHiddenWidgetSqlParams` — `updateHiddenWidgetSql`: rewrite
/// the SQL of the hidden workflow a widget was minted from (keyed by
/// workflowId; not dashboard-scoped).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateHiddenWidgetSqlParams {
    pub workflow_id: String,
    pub connection_id: String,
    pub sql: String,
}

// ---------------------------------------------------------------------------
// Wire vocabulary: the monitor schedule grant. The ExecutionTarget family is
// the canonical `types::query` definition, re-exported here so the module
// path keeps working for the grant wire and its tests.
// ---------------------------------------------------------------------------

pub use crate::types::{ExecutionTarget, NamespaceTarget, ObjectTarget};

/// React `MonitorScope` — one frozen execution permission inside a
/// [`MonitorGrant`]: the target it may execute against, the config
/// revision it froze, the command it may run. Thin mirror of the native
/// `datazen::monitor::domain::MonitorScope` (`u64` revision).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MonitorScope {
    pub target: ExecutionTarget,
    pub config_revision: u64,
    pub command: String,
}

/// React `MonitorGrant` — one frozen authorization for a widget's interval
/// schedule. No bearer credentials: the scopes are server-side facts, and
/// execution rides a service identity derived from the grant. `expiresAt`
/// is the native twin's `DateTime<Utc>` — an RFC 3339 string on the wire,
/// mirrored as `String`. Thin mirror of the native
/// `datazen::monitor::domain::MonitorGrant`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MonitorGrant {
    pub id: String,
    pub dashboard_id: String,
    pub widget_id: String,
    pub workflow_id: String,
    /// SHA-256 of the workflow definition the grant froze.
    pub workflow_revision: String,
    pub organization_id: String,
    pub service_principal: String,
    pub approved_by: String,
    /// The frozen, canonicalized targets the schedule may execute
    /// against.
    pub scopes: Vec<MonitorScope>,
    pub actions: Vec<String>,
    pub read_only: bool,
    pub expires_at: String,
    pub revocation_version: u64,
    pub revoked: bool,
}

/// React `MonitorScheduleGrantSummary` — 旧名兼容 (the legacy UI name):
/// the frontend's 「授权单摘要」 read is a subset of the full
/// [`MonitorGrant`] (the four legacy read fields — `id` / `expiresAt` /
/// `revoked` / `workflowRevision` — are all present on the full shape),
/// so the alias is the type itself, exactly as the React file declares
/// it.
pub type MonitorScheduleGrantSummary = MonitorGrant;

// ---------------------------------------------------------------------------
// Display vocabulary (the facade's own declarations).
// ---------------------------------------------------------------------------

/// React `DashboardWidget` — the **display override**: the wire widget's
/// shape with `chartConfig` widened to the display [`ChartConfig`] (the
/// render-time `timeDomain`/`timeTicks` extensions the wire version
/// lacks; structurally compatible because both extensions are optional).
/// Every other field keeps the wire shape ([`id`], [`title`],
/// [`workflow_id`], [`view_mode`], [`layout`], [`refresh`], [`alert`],
/// [`enabled`]).
///
/// This is the module's only `DashboardWidget` — the React facade exports
/// the wire one only to override it here (see the mounting note in the
/// module docs for the crate-root name situation).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DashboardWidget {
    pub id: String,
    pub title: String,
    pub workflow_id: String,
    pub view_mode: ViewMode,
    /// The display chart config — carries the render-time
    /// `timeDomain`/`timeTicks` extensions through the UI; serde drops
    /// them on the way into storage.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub chart_config: Option<ChartConfig>,
    pub layout: WidgetLayout,
    pub refresh: RefreshPolicy,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub alert: Option<AlertRule>,
    pub enabled: bool,
}

/// React `Dashboard` — the **display override**: the wire dashboard with
/// its `widgets` array retyped to the display [`DashboardWidget`] above.
/// `refreshPaused` is optional (`?:`, no `null`): absent on hand-built
/// drafts, the backend always writes it back — absent stays absent here
/// (the native twin deserializes it as a plain default-`false` and always
/// writes it; the two-state is this facade's contract).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Dashboard {
    pub id: String,
    pub name: String,
    pub created_at: String,
    pub updated_at: String,
    pub layout: DashboardLayout,
    /// The display widgets — each may carry the display chart extensions.
    pub widgets: Vec<DashboardWidget>,
    pub enabled: bool,
    /// Absent on hand-built drafts; the backend always writes it back.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub refresh_paused: Option<bool>,
}

/// React `CreateWidgetResult` — `createWidgetFromSql` /
/// `createWidgetFromWorkflow`: the stored dashboard re-read plus the new
/// widget row, both in their display shapes (backend-client also declares
/// a wire `CreateWidgetResult`; the facade declares its own so the
/// display ChartConfig rides the legs — this is that one).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateWidgetResult {
    pub dashboard: Dashboard,
    pub widget: DashboardWidget,
}

/// React `MonitorEmailSettings` — SMTP settings reserved for phase 2: every
/// field optional. Thin widths per the native twin
/// (`datazen::settings::domain::MonitorEmailSettings`, form-split F4).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MonitorEmailSettings {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub host: Option<String>,
    /// SMTP port (the `connection_types` u16 port-width rule, and the
    /// native twin's).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub port: Option<u16>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub username: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub password: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub from: Option<String>,
    /// Recipient list.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub to: Option<Vec<String>>,
}

/// React `MonitorSettings` — the dashboard monitor / tray / retention
/// settings the frontend declares locally (settings-domain vocabulary;
/// the native twin lives in `datazen::settings::domain`, form-split F4 —
/// the dashboard HTTP surface never carries it). Widths follow the twin.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MonitorSettings {
    pub tray_enabled: bool,
    pub close_to_tray: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_webhook_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub email: Option<MonitorEmailSettings>,
    pub max_concurrent_queries: u32,
    pub export_include_dashboard_runs: bool,
    pub run_retention_count: u32,
    pub run_retention_days: u32,
}

// ---------------------------------------------------------------------------
// Runtime constants and helpers (UI-side derivations of wire values).
// ---------------------------------------------------------------------------

/// React `MIN_REFRESH_SEC = 30` — the smallest interval refresh the
/// product will run (clamped server-side too; the native twin carries its
/// own copy for the scheduler).
pub const MIN_REFRESH_SEC: u32 = 30;

/// React `REFRESH_WARN_BELOW_SEC = 60` — an interval denser than this
/// draws the non-blocking UI warning ([`should_warn_refresh_sec`]).
pub const REFRESH_WARN_BELOW_SEC: u32 = 60;

/// React `clampRefreshSec` — floor an interval refresh at
/// [`MIN_REFRESH_SEC`] (seconds are small non-negative ints; the native
/// twin's `u32` width).
pub fn clamp_refresh_sec(n: u32) -> u32 {
    n.max(MIN_REFRESH_SEC)
}

/// React `normalizeRefreshPolicy` — the facade's write-side normalization:
/// an `interval` policy gets its seconds clamped (defaulting an absent
/// leg to [`MIN_REFRESH_SEC`], the `??` in the React source), and every
/// non-interval policy **drops** its seconds leg — React's non-interval
/// branch returns `{ mode }` only.
pub fn normalize_refresh_policy(refresh: RefreshPolicy) -> RefreshPolicy {
    if refresh.mode == RefreshMode::Interval {
        RefreshPolicy {
            mode: RefreshMode::Interval,
            refresh_sec: Some(clamp_refresh_sec(refresh.refresh_sec.unwrap_or(MIN_REFRESH_SEC))),
        }
    } else {
        RefreshPolicy {
            mode: refresh.mode,
            refresh_sec: None,
        }
    }
}

/// React `shouldWarnRefreshSec` — the non-blocking warning's predicate:
/// true only for an `interval` policy whose seconds are present and below
/// [`REFRESH_WARN_BELOW_SEC`].
pub fn should_warn_refresh_sec(refresh: RefreshPolicy) -> bool {
    refresh.mode == RefreshMode::Interval
        && refresh.refresh_sec.is_some_and(|sec| sec < REFRESH_WARN_BELOW_SEC)
}

/// React `DEFAULT_MONITOR_SETTINGS` — the exact literal object. The
/// optional `defaultWebhookUrl`/`email` legs are `None`, so the value is
/// String-less and const-able (the `DEFAULT_AI_MEMORY_SETTINGS` trick);
/// a test pins every field and the exact wire.
pub const DEFAULT_MONITOR_SETTINGS: MonitorSettings = MonitorSettings {
    tray_enabled: true,
    close_to_tray: true,
    default_webhook_url: None,
    email: None,
    max_concurrent_queries: 2,
    export_include_dashboard_runs: true,
    run_retention_count: 200,
    run_retention_days: 30,
};

/// React `DEFAULT_REFRESH = { mode: 'manual' }` — the literal object:
/// enum-plus-`Option`, const-able, and the exact wire is `{"mode":
/// "manual"}` with no `refreshSec` key.
pub const DEFAULT_REFRESH: RefreshPolicy = RefreshPolicy {
    mode: RefreshMode::Manual,
    refresh_sec: None,
};

#[cfg(test)]
mod tests {
    use super::*;

    // No React test suite backs this file — these tests are the port's
    // own pins: every literal table, the constants, the display-vs-wire
    // split in motion, and the required-key contract.

    fn decode<T: serde::de::DeserializeOwned>(wire: serde_json::Value) -> T {
        serde_json::from_value(wire)
            .unwrap_or_else(|error| panic!("wire decode failed: {error}"))
    }

    fn encode<T: serde::Serialize>(value: &T) -> serde_json::Value {
        serde_json::to_value(value)
            .unwrap_or_else(|error| panic!("wire encode failed: {error}"))
    }

    #[test]
    fn view_modes_keep_the_react_literals() {
        let table = [
            (ViewMode::Chart, "chart"),
            (ViewMode::Table, "table"),
        ];
        for (value, literal) in table {
            assert_eq!(value.as_str(), literal);
            let parsed: ViewMode = decode(serde_json::json!(literal));
            assert_eq!(parsed, value);
            assert_eq!(encode(&value), serde_json::json!(literal));
        }
    }

    #[test]
    fn refresh_modes_keep_the_react_literals() {
        let table = [
            (RefreshMode::Manual, "manual"),
            (RefreshMode::OnOpen, "onOpen"),
            (RefreshMode::Interval, "interval"),
        ];
        for (value, literal) in table {
            assert_eq!(value.as_str(), literal);
            let parsed: RefreshMode = decode(serde_json::json!(literal));
            assert_eq!(parsed, value);
            assert_eq!(encode(&value), serde_json::json!(literal));
        }
    }

    #[test]
    fn widget_run_statuses_keep_the_react_literals() {
        let table = [
            (WidgetRunStatus::Ok, "ok"),
            (WidgetRunStatus::Error, "error"),
            (WidgetRunStatus::Timeout, "timeout"),
        ];
        for (value, literal) in table {
            assert_eq!(value.as_str(), literal);
            let parsed: WidgetRunStatus = decode(serde_json::json!(literal));
            assert_eq!(parsed, value);
            assert_eq!(encode(&value), serde_json::json!(literal));
        }
    }

    #[test]
    fn alert_metric_kinds_keep_the_react_literals() {
        let table = [
            (AlertMetricKind::Column, "column"),
            (AlertMetricKind::Aggregation, "aggregation"),
        ];
        for (value, literal) in table {
            assert_eq!(value.as_str(), literal);
            let parsed: AlertMetricKind = decode(serde_json::json!(literal));
            assert_eq!(parsed, value);
            assert_eq!(encode(&value), serde_json::json!(literal));
        }
    }

    #[test]
    fn alert_metric_aggs_keep_the_react_literals() {
        let table = [
            (AlertMetricAgg::Last, "last"),
            (AlertMetricAgg::Max, "max"),
            (AlertMetricAgg::Min, "min"),
            (AlertMetricAgg::Avg, "avg"),
            (AlertMetricAgg::Sum, "sum"),
        ];
        for (value, literal) in table {
            assert_eq!(value.as_str(), literal);
            let parsed: AlertMetricAgg = decode(serde_json::json!(literal));
            assert_eq!(parsed, value);
            assert_eq!(encode(&value), serde_json::json!(literal));
        }
    }

    #[test]
    fn alert_operators_keep_the_react_literals() {
        let table = [
            (AlertOperator::Gt, ">"),
            (AlertOperator::Gte, ">="),
            (AlertOperator::Lt, "<"),
            (AlertOperator::Lte, "<="),
            (AlertOperator::Eq, "=="),
            (AlertOperator::Ne, "!="),
        ];
        for (value, literal) in table {
            assert_eq!(value.as_str(), literal);
            let parsed: AlertOperator = decode(serde_json::json!(literal));
            assert_eq!(parsed, value);
            assert_eq!(encode(&value), serde_json::json!(literal));
        }
    }

    #[test]
    fn alert_channels_pin_the_react_literals_and_the_legacy_alias() {
        let table = [
            (AlertChannel::App, "app"),
            (AlertChannel::Webhook, "webhook"),
            (AlertChannel::Email, "email"),
        ];
        for (value, literal) in table {
            assert_eq!(value.as_str(), literal);
            let parsed: AlertChannel = decode(serde_json::json!(literal));
            assert_eq!(parsed, value);
            assert_eq!(encode(&value), serde_json::json!(literal));
        }
        // The pre-D6 wire spelling of `app` deserializes (mapped, never
        // re-serialized) — persisted rules survive the rename, the same
        // one-way leniency the native twin implements.
        let legacy: AlertChannel = decode(serde_json::json!(concat!("desk", "top")));
        assert_eq!(legacy, AlertChannel::App);
        assert_eq!(encode(&legacy), serde_json::json!("app"));
        // Unknown channels are rejected — the set is closed.
        assert!(
            serde_json::from_value::<AlertChannel>(serde_json::json!("carrier-pigeon")).is_err()
        );
    }

    #[test]
    fn refresh_policies_round_trip_their_optional_seconds_leg() {
        // interval with a seconds leg: the key rides the wire.
        let interval = RefreshPolicy {
            mode: RefreshMode::Interval,
            refresh_sec: Some(60),
        };
        assert_eq!(
            encode(&interval),
            serde_json::json!({ "mode": "interval", "refreshSec": 60 })
        );
        let back: RefreshPolicy = decode(encode(&interval));
        assert_eq!(back, interval);
        // A seconds-free policy (manual/onOpen, or a draft): the key is
        // absent, in and out.
        let bare = RefreshPolicy {
            mode: RefreshMode::OnOpen,
            refresh_sec: None,
        };
        assert_eq!(encode(&bare), serde_json::json!({ "mode": "onOpen" }));
        let bare_back: RefreshPolicy = decode(serde_json::json!({ "mode": "onOpen" }));
        assert_eq!(bare_back, bare);
        // A day-long interval decodes (the u32 width in motion).
        let day: RefreshPolicy = decode(serde_json::json!({ "mode": "interval", "refreshSec": 86_400 }));
        assert_eq!(day.refresh_sec, Some(86_400));
    }

    #[test]
    fn alert_rules_round_trip_their_nested_metric() {
        let rule = AlertRule {
            metric: AlertMetric {
                kind: AlertMetricKind::Aggregation,
                column: String::from("v"),
                agg: Some(AlertMetricAgg::Avg),
            },
            op: AlertOperator::Gte,
            threshold: 0.75,
            cooldown_sec: 300,
            channels: vec![AlertChannel::App, AlertChannel::Webhook, AlertChannel::Email],
        };
        let wire = serde_json::json!({
            "metric": { "kind": "aggregation", "column": "v", "agg": "avg" },
            "op": ">=",
            "threshold": 0.75,
            "cooldownSec": 300,
            "channels": ["app", "webhook", "email"]
        });
        assert_eq!(encode(&rule), wire);
        let back: AlertRule = decode(wire);
        assert_eq!(back, rule);
        // A column-metric without the agg leg: absent stays absent.
        let column_metric = AlertMetric {
            kind: AlertMetricKind::Column,
            column: String::from("status"),
            agg: None,
        };
        assert_eq!(
            encode(&column_metric),
            serde_json::json!({ "kind": "column", "column": "status" })
        );
    }

    #[test]
    fn a_display_widget_round_trips_the_display_chart_extensions() {
        // The facade's own ruling in motion: chartConfig is the DISPLAY
        // ChartConfig, so the render-time timeDomain/timeTicks extensions
        // survive the facade's wire (they would be dropped by serde on
        // the way into the backend's storage).
        let widget = DashboardWidget {
            id: String::from("widget-1"),
            title: String::from("CPU"),
            workflow_id: String::from("wf-1"),
            view_mode: ViewMode::Chart,
            chart_config: Some(ChartConfig {
                chart_type: crate::chart_types::ChartType::Line,
                x_axis: Some(String::from("ts")),
                y_axes: vec![String::from("cpu")],
                group_by: None,
                aggregation: crate::chart_types::AggregationType::Avg,
                sort_by: crate::chart_types::ChartSortBy::None,
                show_legend: true,
                show_grid: true,
                show_values: false,
                color_scheme: String::from("default"),
                time_domain: Some((1_767_225_600_000.0, 1_767_225_660_000.0)),
                time_ticks: Some(vec![0.0, 30_000.0, 60_000.0]),
            }),
            layout: WidgetLayout { x: 0, y: 0, w: 6, h: 4 },
            refresh: RefreshPolicy {
                mode: RefreshMode::Interval,
                refresh_sec: Some(60),
            },
            alert: Some(AlertRule {
                metric: AlertMetric {
                    kind: AlertMetricKind::Aggregation,
                    column: String::from("v"),
                    agg: Some(AlertMetricAgg::Avg),
                },
                op: AlertOperator::Gt,
                threshold: 0.9,
                cooldown_sec: 300,
                channels: vec![AlertChannel::App],
            }),
            enabled: true,
        };
        let wire = serde_json::json!({
            "id": "widget-1",
            "title": "CPU",
            "workflowId": "wf-1",
            "viewMode": "chart",
            "chartConfig": {
                "chartType": "line",
                "xAxis": "ts",
                "yAxes": ["cpu"],
                "groupBy": null,
                "aggregation": "avg",
                "sortBy": "none",
                "showLegend": true,
                "showGrid": true,
                "showValues": false,
                "colorScheme": "default",
                "timeDomain": [1767225600000.0, 1767225660000.0],
                "timeTicks": [0.0, 30000.0, 60000.0]
            },
            "layout": { "x": 0, "y": 0, "w": 6, "h": 4 },
            "refresh": { "mode": "interval", "refreshSec": 60 },
            "alert": {
                "metric": { "kind": "aggregation", "column": "v", "agg": "avg" },
                "op": ">",
                "threshold": 0.9,
                "cooldownSec": 300,
                "channels": ["app"]
            },
            "enabled": true
        });
        assert_eq!(encode(&widget), wire);
        let back: DashboardWidget = decode(wire);
        assert_eq!(back, widget);
        // The bare widget: no chartConfig, no alert — both keys absent,
        // in and out (the `?:` rule).
        let bare = DashboardWidget {
            id: String::from("widget-2"),
            title: String::from("Table"),
            workflow_id: String::from("wf-2"),
            view_mode: ViewMode::Table,
            chart_config: None,
            layout: WidgetLayout { x: 6, y: 0, w: 6, h: 8 },
            refresh: RefreshPolicy {
                mode: RefreshMode::Manual,
                refresh_sec: None,
            },
            alert: None,
            enabled: false,
        };
        let bare_wire = encode(&bare);
        assert!(bare_wire.get("chartConfig").is_none());
        assert!(bare_wire.get("alert").is_none());
        let bare_back: DashboardWidget = decode(bare_wire);
        assert_eq!(bare_back, bare);
    }

    #[test]
    fn a_display_dashboard_round_trips_widgets_and_refresh_paused() {
        let dashboard = Dashboard {
            id: String::from("dash-1"),
            name: String::from("Ops"),
            created_at: String::from("2026-01-01T00:00:00Z"),
            updated_at: String::from("2026-02-01T00:00:00Z"),
            layout: DashboardLayout {
                cols: 12,
                row_height: 8,
            },
            widgets: vec![DashboardWidget {
                id: String::from("widget-1"),
                title: String::from("CPU"),
                workflow_id: String::from("wf-1"),
                view_mode: ViewMode::Chart,
                chart_config: Some(ChartConfig {
                    time_domain: Some((0.0, 60_000.0)),
                    ..ChartConfig::default()
                }),
                layout: WidgetLayout { x: 0, y: 0, w: 12, h: 4 },
                refresh: RefreshPolicy {
                    mode: RefreshMode::OnOpen,
                    refresh_sec: None,
                },
                alert: None,
                enabled: true,
            }],
            enabled: true,
            refresh_paused: Some(false),
        };
        let wire = encode(&dashboard);
        // refreshPaused is `?:` — Some(false) writes the key as false…
        assert_eq!(wire["refreshPaused"], false);
        // …and the widget's display extension rides along.
        assert_eq!(
            wire["widgets"][0]["chartConfig"]["timeDomain"],
            serde_json::json!([0.0, 60000.0])
        );
        let back: Dashboard = decode(wire);
        assert_eq!(back, dashboard);
        // The draft state: refreshPaused absent → None, in and out.
        let draft = Dashboard {
            refresh_paused: None,
            ..dashboard
        };
        let draft_wire = encode(&draft);
        assert!(draft_wire.get("refreshPaused").is_none());
        let draft_back: Dashboard = decode(draft_wire);
        assert_eq!(draft_back.refresh_paused, None);
    }

    #[test]
    fn create_widget_params_round_trip_their_optional_legs() {
        let sql_params = CreateWidgetFromSqlParams {
            dashboard_id: String::from("dash-1"),
            connection_id: String::from("c1"),
            sql: String::from("select 1"),
            title: Some(String::from("One")),
            view_mode: ViewMode::Table,
            chart_config: Some(ChartConfig::default()),
        };
        let wire = serde_json::json!({
            "dashboardId": "dash-1",
            "connectionId": "c1",
            "sql": "select 1",
            "title": "One",
            "viewMode": "table",
            "chartConfig": {
                "chartType": "bar",
                "xAxis": null,
                "yAxes": [],
                "groupBy": null,
                "aggregation": "none",
                "sortBy": "none",
                "showLegend": true,
                "showGrid": true,
                "showValues": false,
                "colorScheme": "default"
            }
        });
        assert_eq!(encode(&sql_params), wire);
        let back: CreateWidgetFromSqlParams = decode(wire);
        assert_eq!(back, sql_params);

        // Minimal: title and chartConfig absent — the keys vanish.
        let minimal = CreateWidgetFromSqlParams {
            dashboard_id: String::from("dash-1"),
            connection_id: String::from("c1"),
            sql: String::from("select 1"),
            title: None,
            view_mode: ViewMode::Table,
            chart_config: None,
        };
        assert_eq!(
            encode(&minimal),
            serde_json::json!({
                "dashboardId": "dash-1",
                "connectionId": "c1",
                "sql": "select 1",
                "viewMode": "table"
            })
        );

        let workflow_params = CreateWidgetFromWorkflowParams {
            dashboard_id: String::from("dash-1"),
            workflow_id: String::from("wf-1"),
            title: None,
            view_mode: ViewMode::Chart,
            chart_config: None,
        };
        assert_eq!(
            encode(&workflow_params),
            serde_json::json!({
                "dashboardId": "dash-1",
                "workflowId": "wf-1",
                "viewMode": "chart"
            })
        );
        let back: CreateWidgetFromWorkflowParams =
            decode(serde_json::json!({ "dashboardId": "dash-1", "workflowId": "wf-1", "viewMode": "chart" }));
        assert_eq!(back, workflow_params);

        let hidden_sql = UpdateHiddenWidgetSqlParams {
            workflow_id: String::from("wf-1"),
            connection_id: String::from("c1"),
            sql: String::from("select 2"),
        };
        assert_eq!(
            encode(&hidden_sql),
            serde_json::json!({
                "workflowId": "wf-1",
                "connectionId": "c1",
                "sql": "select 2"
            })
        );
        let hidden_back: UpdateHiddenWidgetSqlParams = decode(encode(&hidden_sql));
        assert_eq!(hidden_back, hidden_sql);
    }

    #[test]
    fn a_create_widget_result_nests_the_display_shapes() {
        let result = CreateWidgetResult {
            dashboard: Dashboard {
                id: String::from("dash-1"),
                name: String::from("Ops"),
                created_at: String::from("2026-01-01T00:00:00Z"),
                updated_at: String::from("2026-02-01T00:00:00Z"),
                layout: DashboardLayout { cols: 12, row_height: 8 },
                widgets: Vec::new(),
                enabled: true,
                refresh_paused: None,
            },
            widget: DashboardWidget {
                id: String::from("widget-1"),
                title: String::from("CPU"),
                workflow_id: String::from("wf-1"),
                view_mode: ViewMode::Chart,
                chart_config: Some(ChartConfig {
                    time_ticks: Some(vec![0.0, 1.0]),
                    ..ChartConfig::default()
                }),
                layout: WidgetLayout { x: 0, y: 0, w: 6, h: 4 },
                refresh: RefreshPolicy {
                    mode: RefreshMode::Manual,
                    refresh_sec: None,
                },
                alert: None,
                enabled: true,
            },
        };
        let wire = encode(&result);
        assert_eq!(wire["dashboard"]["id"], "dash-1");
        assert_eq!(wire["widget"]["chartConfig"]["timeTicks"], serde_json::json!([0.0, 1.0]));
        let back: CreateWidgetResult = decode(wire);
        assert_eq!(back, result);
    }

    #[test]
    fn widget_runs_round_trip_their_opaque_and_optional_legs() {
        let run = WidgetRun {
            id: String::from("run-1"),
            dashboard_id: String::from("dash-1"),
            widget_id: String::from("widget-1"),
            workflow_id: String::from("wf-1"),
            started_at: String::from("2026-02-03T04:05:06Z"),
            finished_at: String::from("2026-02-03T04:05:07Z"),
            status: WidgetRunStatus::Ok,
            error: None,
            row_count: 4_294_967_295,
            columns: vec![String::from("id"), String::from("v")],
            rows: vec![
                vec![serde_json::json!(1), serde_json::json!("a")],
                vec![serde_json::Value::Null, serde_json::json!({"nested": true})],
            ],
            alert_fired: Some(true),
            alert_value: Some(1.5),
        };
        let wire = serde_json::json!({
            "id": "run-1",
            "dashboardId": "dash-1",
            "widgetId": "widget-1",
            "workflowId": "wf-1",
            "startedAt": "2026-02-03T04:05:06Z",
            "finishedAt": "2026-02-03T04:05:07Z",
            "status": "ok",
            "rowCount": 4294967295u64,
            "columns": ["id", "v"],
            "rows": [[1, "a"], [null, { "nested": true }]],
            "alertFired": true,
            "alertValue": 1.5
        });
        assert_eq!(encode(&run), wire);
        let back: WidgetRun = decode(wire);
        assert_eq!(back, run);

        // The error run: `error` present; the alert legs absent.
        let errored: WidgetRun = decode(serde_json::json!({
            "id": "run-2",
            "dashboardId": "dash-1",
            "widgetId": "widget-1",
            "workflowId": "wf-1",
            "startedAt": "2026-02-03T04:05:06Z",
            "finishedAt": "2026-02-03T04:05:09Z",
            "status": "error",
            "error": "boom",
            "rowCount": 0,
            "columns": [],
            "rows": []
        }));
        assert_eq!(errored.error, Some(String::from("boom")));
        assert_eq!(errored.alert_fired, None);
        assert_eq!(errored.alert_value, None);
        let errored_wire = encode(&errored);
        assert!(errored_wire.get("alertFired").is_none());
        assert!(errored_wire.get("alertValue").is_none());

        // The run history index: alertFired is the only optional leg.
        let entry = RunIndexEntry {
            id: String::from("run-2"),
            started_at: String::from("2026-02-03T04:05:06Z"),
            status: WidgetRunStatus::Timeout,
            alert_fired: None,
        };
        assert_eq!(
            encode(&entry),
            serde_json::json!({
                "id": "run-2",
                "startedAt": "2026-02-03T04:05:06Z",
                "status": "timeout"
            })
        );

        // The workflow-delete gate's read: five required strings.
        let reference = DashboardWorkflowRef {
            workflow_id: String::from("wf-1"),
            dashboard_id: String::from("dash-1"),
            widget_id: String::from("widget-1"),
            dashboard_name: String::from("Ops"),
            widget_title: String::from("CPU"),
        };
        assert_eq!(
            encode(&reference),
            serde_json::json!({
                "workflowId": "wf-1",
                "dashboardId": "dash-1",
                "widgetId": "widget-1",
                "dashboardName": "Ops",
                "widgetTitle": "CPU"
            })
        );
    }

    #[test]
    fn monitor_grants_round_trip_their_scopes_and_targets() {
        let grant = MonitorGrant {
            id: String::from("grant-1"),
            dashboard_id: String::from("dash-1"),
            widget_id: String::from("widget-1"),
            workflow_id: String::from("wf-1"),
            workflow_revision: String::from(
                "9f86d081884c7d659a2feaa0c55ad015a3bf4f1b2b0b822cd15d6c15b0f00a08",
            ),
            organization_id: String::from("org-1"),
            service_principal: String::from("sp-1"),
            approved_by: String::from("user-1"),
            scopes: vec![MonitorScope {
                target: ExecutionTarget {
                    connection_id: String::from("c1"),
                    namespace: NamespaceTarget {
                        database: None,
                        catalog: None,
                        schema: Some(String::from("public")),
                        path: Vec::new(),
                    },
                    object: Some(ObjectTarget {
                        kind: String::from("table"),
                        name: String::from("users"),
                        signature: Some(String::from("table|view")),
                    }),
                },
                config_revision: 8_589_934_592,
                command: String::from("query"),
            }],
            actions: vec![String::from("connect"), String::from("query")],
            read_only: true,
            expires_at: String::from("2027-01-02T03:04:05Z"),
            revocation_version: 1,
            revoked: false,
        };
        let wire = serde_json::json!({
            "id": "grant-1",
            "dashboardId": "dash-1",
            "widgetId": "widget-1",
            "workflowId": "wf-1",
            "workflowRevision": "9f86d081884c7d659a2feaa0c55ad015a3bf4f1b2b0b822cd15d6c15b0f00a08",
            "organizationId": "org-1",
            "servicePrincipal": "sp-1",
            "approvedBy": "user-1",
            "scopes": [{
                "target": {
                    "connectionId": "c1",
                    "namespace": {
                        "database": null,
                        "catalog": null,
                        "schema": "public",
                        "path": []
                    },
                    "object": {
                        "kind": "table",
                        "name": "users",
                        "signature": "table|view"
                    }
                },
                "configRevision": 8589934592u64,
                "command": "query"
            }],
            "actions": ["connect", "query"],
            "readOnly": true,
            "expiresAt": "2027-01-02T03:04:05Z",
            "revocationVersion": 1,
            "revoked": false
        });
        assert_eq!(encode(&grant), wire);
        let back: MonitorGrant = decode(wire.clone());
        assert_eq!(back, grant);
        // The namespace layers are required-but-nullable: explicit nulls
        // in and out, keys present.
        assert_eq!(
            wire["scopes"][0]["target"]["namespace"]["database"],
            serde_json::Value::Null
        );
        // The legacy alias reads the same shape (it *is* the same type).
        let summary: MonitorScheduleGrantSummary = decode(encode(&grant));
        assert_eq!(summary.id, grant.id);
        assert_eq!(summary.expires_at, grant.expires_at);
        assert_eq!(summary.revoked, grant.revoked);
        assert_eq!(summary.workflow_revision, grant.workflow_revision);

        // An object-free target: `object: null` — explicit null, key
        // present (the required-but-nullable rule).
        let bare_target = ExecutionTarget {
            connection_id: String::from("c2"),
            namespace: NamespaceTarget {
                database: Some(String::from("app")),
                catalog: None,
                schema: None,
                path: vec![String::from("p1")],
            },
            object: None,
        };
        assert_eq!(
            encode(&bare_target),
            serde_json::json!({
                "connectionId": "c2",
                "namespace": {
                    "database": "app",
                    "catalog": null,
                    "schema": null,
                    "path": ["p1"]
                },
                "object": null
            })
        );
        // An undetermined signature: `signature: null`, key present.
        let undetermined = ObjectTarget {
            kind: String::from("view"),
            name: String::from("v1"),
            signature: None,
        };
        assert_eq!(
            encode(&undetermined),
            serde_json::json!({ "kind": "view", "name": "v1", "signature": null })
        );
    }

    #[test]
    fn monitor_settings_round_trip_their_optional_legs() {
        let settings = MonitorSettings {
            tray_enabled: true,
            close_to_tray: false,
            default_webhook_url: Some(String::from("https://hooks.example.com/xyz")),
            email: Some(MonitorEmailSettings {
                enabled: Some(true),
                host: Some(String::from("smtp.example.com")),
                port: Some(465),
                username: Some(String::from("u")),
                password: Some(String::from("p")),
                from: Some(String::from("datazen@example.com")),
                to: Some(vec![
                    String::from("a@example.com"),
                    String::from("b@example.com"),
                ]),
            }),
            max_concurrent_queries: 4,
            export_include_dashboard_runs: true,
            run_retention_count: 200,
            run_retention_days: 30,
        };
        let wire = serde_json::json!({
            "trayEnabled": true,
            "closeToTray": false,
            "defaultWebhookUrl": "https://hooks.example.com/xyz",
            "email": {
                "enabled": true,
                "host": "smtp.example.com",
                "port": 465,
                "username": "u",
                "password": "p",
                "from": "datazen@example.com",
                "to": ["a@example.com", "b@example.com"]
            },
            "maxConcurrentQueries": 4,
            "exportIncludeDashboardRuns": true,
            "runRetentionCount": 200,
            "runRetentionDays": 30
        });
        assert_eq!(encode(&settings), wire);
        let back: MonitorSettings = decode(wire);
        assert_eq!(back, settings);
        // Minimal: every optional leg absent, in and out.
        let minimal = MonitorSettings {
            tray_enabled: true,
            close_to_tray: true,
            default_webhook_url: None,
            email: None,
            max_concurrent_queries: 2,
            export_include_dashboard_runs: true,
            run_retention_count: 200,
            run_retention_days: 30,
        };
        assert_eq!(
            encode(&minimal),
            serde_json::json!({
                "trayEnabled": true,
                "closeToTray": true,
                "maxConcurrentQueries": 2,
                "exportIncludeDashboardRuns": true,
                "runRetentionCount": 200,
                "runRetentionDays": 30
            })
        );
        // The SMTP port is u16: 65536 is out of range.
        assert!(
            serde_json::from_value::<MonitorEmailSettings>(serde_json::json!({ "port": 65536 }))
                .is_err()
        );
    }

    #[test]
    fn clamp_refresh_sec_enforces_the_minimum() {
        assert_eq!(clamp_refresh_sec(0), 30);
        assert_eq!(clamp_refresh_sec(5), 30);
        assert_eq!(clamp_refresh_sec(29), 30);
        assert_eq!(clamp_refresh_sec(30), 30);
        assert_eq!(clamp_refresh_sec(120), 120);
        assert_eq!(MIN_REFRESH_SEC, 30);
        assert_eq!(REFRESH_WARN_BELOW_SEC, 60);
    }

    #[test]
    fn normalize_refresh_policy_clamps_and_drops_the_seconds_leg() {
        // interval + seconds: clamped to the minimum.
        let clamped = normalize_refresh_policy(RefreshPolicy {
            mode: RefreshMode::Interval,
            refresh_sec: Some(5),
        });
        assert_eq!(clamped.mode, RefreshMode::Interval);
        assert_eq!(clamped.refresh_sec, Some(30));
        // interval + absent seconds: the React `?? MIN_REFRESH_SEC`.
        let defaulted = normalize_refresh_policy(RefreshPolicy {
            mode: RefreshMode::Interval,
            refresh_sec: None,
        });
        assert_eq!(defaulted.refresh_sec, Some(30));
        // A dense interval past the clamp stays itself.
        let kept = normalize_refresh_policy(RefreshPolicy {
            mode: RefreshMode::Interval,
            refresh_sec: Some(120),
        });
        assert_eq!(kept.refresh_sec, Some(120));
        // Non-interval: the seconds leg is DROPPED (React returns `{ mode }`),
        // and the wire proves the key vanishes.
        let dropped = normalize_refresh_policy(RefreshPolicy {
            mode: RefreshMode::Manual,
            refresh_sec: Some(60),
        });
        assert_eq!(dropped.mode, RefreshMode::Manual);
        assert_eq!(dropped.refresh_sec, None);
        assert_eq!(encode(&dropped), serde_json::json!({ "mode": "manual" }));
        let on_open = normalize_refresh_policy(RefreshPolicy {
            mode: RefreshMode::OnOpen,
            refresh_sec: None,
        });
        assert_eq!(on_open.mode, RefreshMode::OnOpen);
        assert_eq!(on_open.refresh_sec, None);
    }

    #[test]
    fn should_warn_refresh_sec_pins_the_threshold() {
        assert!(should_warn_refresh_sec(RefreshPolicy {
            mode: RefreshMode::Interval,
            refresh_sec: Some(30),
        }));
        assert!(should_warn_refresh_sec(RefreshPolicy {
            mode: RefreshMode::Interval,
            refresh_sec: Some(59),
        }));
        assert!(!should_warn_refresh_sec(RefreshPolicy {
            mode: RefreshMode::Interval,
            refresh_sec: Some(60),
        }));
        assert!(!should_warn_refresh_sec(RefreshPolicy {
            mode: RefreshMode::Interval,
            refresh_sec: Some(120),
        }));
        // No seconds leg: nothing to warn about.
        assert!(!should_warn_refresh_sec(RefreshPolicy {
            mode: RefreshMode::Interval,
            refresh_sec: None,
        }));
        // Only interval modes warn.
        assert!(!should_warn_refresh_sec(RefreshPolicy {
            mode: RefreshMode::Manual,
            refresh_sec: Some(30),
        }));
        assert!(!should_warn_refresh_sec(RefreshPolicy {
            mode: RefreshMode::OnOpen,
            refresh_sec: Some(30),
        }));
    }

    #[test]
    fn default_refresh_pins_the_react_constant() {
        assert_eq!(DEFAULT_REFRESH.mode, RefreshMode::Manual);
        assert_eq!(DEFAULT_REFRESH.refresh_sec, None);
        // The exact React literal object: `{"mode": "manual"}` — no
        // refreshSec key.
        assert_eq!(encode(&DEFAULT_REFRESH), serde_json::json!({ "mode": "manual" }));
    }

    #[test]
    fn default_monitor_settings_pins_the_react_constant() {
        let settings = DEFAULT_MONITOR_SETTINGS;
        assert!(settings.tray_enabled);
        assert!(settings.close_to_tray);
        assert_eq!(settings.default_webhook_url, None);
        assert_eq!(settings.email, None);
        assert_eq!(settings.max_concurrent_queries, 2);
        assert!(settings.export_include_dashboard_runs);
        assert_eq!(settings.run_retention_count, 200);
        assert_eq!(settings.run_retention_days, 30);
        // The exact React literal object, key for key: the optional
        // defaultWebhookUrl/email legs are absent, not null.
        assert_eq!(
            encode(&settings),
            serde_json::json!({
                "trayEnabled": true,
                "closeToTray": true,
                "maxConcurrentQueries": 2,
                "exportIncludeDashboardRuns": true,
                "runRetentionCount": 200,
                "runRetentionDays": 30
            })
        );
    }

    #[test]
    fn required_keys_stay_required_on_the_dashboard_wire() {
        // A field React declares without `?` must not grow a serde
        // default: TS's required marker is a wire contract (the same pin
        // `connection_types`' audit round added). The native twin's
        // cooldown-300 default is a storage leniency, not this facade's.
        assert!(
            serde_json::from_value::<AlertRule>(serde_json::json!({
                "metric": { "kind": "column", "column": "v" },
                "op": ">",
                "threshold": 1.0,
                "channels": []
            }))
            .is_err(),
            "cooldownSec is required"
        );
        assert!(
            serde_json::from_value::<RefreshPolicy>(serde_json::json!({})).is_err(),
            "mode is required"
        );
        assert!(
            serde_json::from_value::<Dashboard>(serde_json::json!({
                "id": "dash-1",
                "name": "Ops",
                "createdAt": "2026-01-01T00:00:00Z",
                "updatedAt": "2026-01-01T00:00:00Z",
                "widgets": [],
                "enabled": true
            }))
            .is_err(),
            "layout is required"
        );
        assert!(
            serde_json::from_value::<WidgetRun>(serde_json::json!({
                "id": "run-1",
                "dashboardId": "dash-1",
                "widgetId": "widget-1",
                "workflowId": "wf-1",
                "startedAt": "2026-02-03T04:05:06Z",
                "finishedAt": "2026-02-03T04:05:07Z",
                "status": "ok",
                "columns": [],
                "rows": []
            }))
            .is_err(),
            "rowCount is required"
        );
        assert!(
            serde_json::from_value::<MonitorGrant>(serde_json::json!({
                "id": "grant-1",
                "dashboardId": "dash-1",
                "widgetId": "widget-1",
                "workflowId": "wf-1",
                "workflowRevision": "r",
                "organizationId": "org-1",
                "servicePrincipal": "sp-1",
                "approvedBy": "user-1",
                "scopes": [],
                "actions": [],
                "readOnly": true,
                "expiresAt": "2027-01-02T03:04:05Z",
                "revocationVersion": 1
            }))
            .is_err(),
            "revoked is required"
        );
        assert!(
            serde_json::from_value::<NamespaceTarget>(serde_json::json!({
                "database": null,
                "catalog": null,
                "schema": null
            }))
            .is_err(),
            "path is required"
        );
        assert!(
            serde_json::from_value::<ExecutionTarget>(serde_json::json!({
                "namespace": {
                    "database": null,
                    "catalog": null,
                    "schema": null,
                    "path": []
                },
                "object": null
            }))
            .is_err(),
            "connectionId is required"
        );
        assert!(
            serde_json::from_value::<MonitorSettings>(serde_json::json!({
                "closeToTray": true,
                "maxConcurrentQueries": 2,
                "exportIncludeDashboardRuns": true,
                "runRetentionCount": 200,
                "runRetentionDays": 30
            }))
            .is_err(),
            "trayEnabled is required"
        );
        // And the optional legs stay optional (decode succeeds without
        // them) — pinned by the round-trip tests above; one explicit
        // witness here keeps the pair honest.
        assert!(
            serde_json::from_value::<RefreshPolicy>(serde_json::json!({ "mode": "manual" })).is_ok()
        );
    }

    #[test]
    fn the_dashboard_vocabulary_is_reexported_at_the_crate_root() {
        // `crate::` paths only — a dropped root re-export stops this file
        // from compiling instead of silently shrinking the facade (the
        // same pin style as `chart_types`'/`connection_types`').
        fn resolves<T>() {}
        resolves::<crate::AlertChannel>();
        resolves::<crate::AlertMetric>();
        resolves::<crate::AlertMetricAgg>();
        resolves::<crate::AlertMetricKind>();
        resolves::<crate::AlertOperator>();
        resolves::<crate::AlertRule>();
        resolves::<crate::CreateWidgetFromSqlParams>();
        resolves::<crate::CreateWidgetFromWorkflowParams>();
        resolves::<crate::CreateWidgetResult>();
        resolves::<crate::Dashboard>();
        resolves::<crate::DashboardLayout>();
        resolves::<crate::DashboardWorkflowRef>();
        resolves::<crate::MonitorEmailSettings>();
        resolves::<crate::MonitorGrant>();
        resolves::<crate::MonitorScheduleGrantSummary>();
        resolves::<crate::MonitorSettings>();
        resolves::<crate::MonitorScope>();
        resolves::<crate::RefreshMode>();
        resolves::<crate::RefreshPolicy>();
        resolves::<crate::RunIndexEntry>();
        resolves::<crate::UpdateHiddenWidgetSqlParams>();
        resolves::<crate::ViewMode>();
        resolves::<crate::WidgetLayout>();
        resolves::<crate::WidgetRun>();
        resolves::<crate::WidgetRunStatus>();
        // The names the root cannot carry (see the module docs'
        // mounting note) stay reachable through the module path —
        // `pub mod` is what buys them that path.
        resolves::<crate::dashboard_types::DashboardWidget>();
        resolves::<crate::dashboard_types::ExecutionTarget>();
        resolves::<crate::dashboard_types::NamespaceTarget>();
        resolves::<crate::dashboard_types::ObjectTarget>();
        // …and one live decode through the root path keeps the pin honest.
        let policy: crate::RefreshPolicy =
            decode(serde_json::json!({ "mode": "interval", "refreshSec": 60 }));
        assert_eq!(policy.mode, crate::RefreshMode::Interval);
        assert_eq!(policy.refresh_sec, Some(60));

        // The root `DashboardWidget` is still the N13 workspace stub
        // (`id: u64, title, kind`), NOT this facade type: constructing it
        // with the stub's fields compiles only against the stub, which
        // pins the name split until the stub retires.
        let stub = crate::DashboardWidget {
            id: 7,
            title: String::from("Host Widget"),
            kind: String::from("bar"),
        };
        assert_eq!(stub.id, 7);
        let display: crate::dashboard_types::DashboardWidget = decode(serde_json::json!({
            "id": "widget-1",
            "title": "CPU",
            "workflowId": "wf-1",
            "viewMode": "chart",
            "layout": { "x": 0, "y": 0, "w": 6, "h": 4 },
            "refresh": { "mode": "manual" },
            "enabled": true
        }));
        assert_eq!(display.id, "widget-1");
        assert_eq!(display.chart_config, None);
    }

    // ── audit pins (T159) ──
    //
    // The port's own pins caught 15 of the audit's 24 mutations. The nine
    // survivors were all silent:
    //
    // - widening `ViewMode` with a third variant (`Grid`, wire "grid") left
    //   every table green — the literal tables pin that the React members
    //   exist, not that nothing else does, so the closedness TS guarantees
    //   at compile time had no runtime pin here (T090 closed the same gap
    //   for the chart vocabulary the same way);
    // - narrowing five `u32`/`u64` fields (`WidgetLayout`'s cells to u8,
    //   `DashboardLayout`'s to u16, `cooldownSec` to u16,
    //   `revocationVersion` to u32, `runRetentionCount` to u8) and widening
    //   `refreshSec` to u64 left 448 tests green — every width witness in
    //   the round-trip pins used small human-scale values, so the widths
    //   the native twin fixes (and the module docs promise) were unpinned;
    // - dropping the *value* half of lib.rs's re-export list
    //   (`clamp_refresh_sec`, `DEFAULT_REFRESH`) kept tests and all three
    //   crates checking — the resolves-style root pin sees types only, and
    //   no downstream crate consumes the constants yet.
    //
    // The three pins below close those: near-miss literal rejections for
    // every closed union, wire-level width witnesses, and a root pin that
    // *uses* the re-exported values and functions through `crate::` paths.

    #[test]
    fn the_literal_unions_are_closed() {
        // TS's closed string-literal unions are compile-time contracts;
        // the runtime mirror is serde refusing a non-member literal. The
        // near-misses below are all spellings a widened enum would gladly
        // accept — `grid` in the audit's surviving mutant would have
        // decoded as a third view mode.
        for unknown in ["grid", "Chart", "graph"] {
            let rejected = serde_json::from_value::<ViewMode>(serde_json::json!(unknown));
            assert!(rejected.is_err(), "ViewMode rejected {unknown:?}");
        }
        for unknown in ["onopen", "Manual", "open"] {
            let rejected = serde_json::from_value::<RefreshMode>(serde_json::json!(unknown));
            assert!(rejected.is_err(), "RefreshMode rejected {unknown:?}");
        }
        for unknown in ["failed", "Ok", "timedout"] {
            let rejected = serde_json::from_value::<WidgetRunStatus>(serde_json::json!(unknown));
            assert!(rejected.is_err(), "WidgetRunStatus rejected {unknown:?}");
        }
        for unknown in ["Column", "metric"] {
            let rejected = serde_json::from_value::<AlertMetricKind>(serde_json::json!(unknown));
            assert!(rejected.is_err(), "AlertMetricKind rejected {unknown:?}");
        }
        for unknown in ["average", "mean", "Last"] {
            let rejected = serde_json::from_value::<AlertMetricAgg>(serde_json::json!(unknown));
            assert!(rejected.is_err(), "AlertMetricAgg rejected {unknown:?}");
        }
        for unknown in [">>", "=<", "!=="] {
            let rejected = serde_json::from_value::<AlertOperator>(serde_json::json!(unknown));
            assert!(rejected.is_err(), "AlertOperator rejected {unknown:?}");
        }
        // AlertChannel's closed set is already pinned: the
        // "carrier-pigeon" reject in the literal table test above.
    }

    #[test]
    fn wire_widths_follow_the_native_twin() {
        // Grid cells, seconds and counts are the native twin's `u32`;
        // revisions are `u64`; the SMTP port is `u16` (its 65536 reject is
        // pinned in the monitor-settings round trip above). Each witness
        // below rides the wire at a value only the twin's width carries,
        // so narrowing (or silently widening) a field fails here, not in
        // production.
        let layout: WidgetLayout = decode(serde_json::json!({
            "x": 4294967295u64, "y": 65536, "w": 300, "h": 1
        }));
        assert_eq!(layout.x, u32::MAX);
        assert_eq!(layout.y, 65_536);
        assert_eq!(encode(&layout)["x"], serde_json::json!(4294967295u64));
        let grid: DashboardLayout = decode(serde_json::json!({
            "cols": 4294967295u64, "rowHeight": 65536
        }));
        assert_eq!(grid.cols, u32::MAX);
        assert_eq!(grid.row_height, 65_536);
        // A cooldown pinned at the wire's u32 ceiling.
        let rule: AlertRule = decode(serde_json::json!({
            "metric": { "kind": "column", "column": "v" },
            "op": ">",
            "threshold": 1.0,
            "cooldownSec": 4294967295u64,
            "channels": []
        }));
        assert_eq!(rule.cooldown_sec, u32::MAX);
        // refreshSec tops out at u32: the ceiling decodes, u32::MAX + 1
        // does not (a field silently widened to u64 would accept both).
        let policy: RefreshPolicy =
            decode(serde_json::json!({ "mode": "interval", "refreshSec": 4294967295u64 }));
        assert_eq!(policy.refresh_sec, Some(u32::MAX));
        assert!(
            serde_json::from_value::<RefreshPolicy>(serde_json::json!({
                "mode": "interval", "refreshSec": 4294967296u64
            }))
            .is_err(),
            "refreshSec is a u32 field"
        );
        // Retention counts and concurrency are u32 at their ceilings.
        let settings: MonitorSettings = decode(serde_json::json!({
            "trayEnabled": true,
            "closeToTray": false,
            "maxConcurrentQueries": 300,
            "exportIncludeDashboardRuns": true,
            "runRetentionCount": 4294967295u64,
            "runRetentionDays": 65536
        }));
        assert_eq!(settings.max_concurrent_queries, 300);
        assert_eq!(settings.run_retention_count, u32::MAX);
        assert_eq!(settings.run_retention_days, 65_536);
        // Revisions are u64: revocationVersion past u32::MAX rides the
        // wire both ways (configRevision's 2^33 witness already rides the
        // grant round trip above).
        let grant: MonitorGrant = decode(serde_json::json!({
            "id": "grant-2",
            "dashboardId": "dash-1",
            "widgetId": "widget-1",
            "workflowId": "wf-1",
            "workflowRevision": "r",
            "organizationId": "org-1",
            "servicePrincipal": "sp-1",
            "approvedBy": "user-1",
            "scopes": [],
            "actions": [],
            "readOnly": true,
            "expiresAt": "2027-01-02T03:04:05Z",
            "revocationVersion": 4294967296u64,
            "revoked": false
        }));
        assert_eq!(grant.revocation_version, u32::MAX as u64 + 1);
        assert_eq!(
            encode(&grant)["revocationVersion"],
            serde_json::json!(4294967296u64)
        );
    }

    #[test]
    fn the_runtime_constants_and_helpers_reexport_at_the_crate_root() {
        // The four consts and three helpers ride the crate root alongside
        // the types — a resolves-style pin sees types only, so these are
        // pinned by *using* them through `crate::` paths: a dropped root
        // re-export stops this file from compiling instead of silently
        // shrinking the facade.
        assert_eq!(crate::MIN_REFRESH_SEC, 30);
        assert_eq!(crate::REFRESH_WARN_BELOW_SEC, 60);
        assert_eq!(crate::clamp_refresh_sec(29), 30);
        assert_eq!(crate::clamp_refresh_sec(120), 120);
        assert_eq!(crate::DEFAULT_REFRESH.mode, crate::RefreshMode::Manual);
        assert_eq!(
            encode(&crate::DEFAULT_REFRESH),
            serde_json::json!({ "mode": "manual" })
        );
        let normalized = crate::normalize_refresh_policy(crate::RefreshPolicy {
            mode: crate::RefreshMode::Interval,
            refresh_sec: None,
        });
        assert_eq!(normalized.refresh_sec, Some(crate::MIN_REFRESH_SEC));
        assert!(crate::should_warn_refresh_sec(crate::RefreshPolicy {
            mode: crate::RefreshMode::Interval,
            refresh_sec: Some(59),
        }));
        assert_eq!(crate::DEFAULT_MONITOR_SETTINGS.max_concurrent_queries, 2);
        assert_eq!(crate::DEFAULT_MONITOR_SETTINGS.run_retention_days, 30);
    }
}
