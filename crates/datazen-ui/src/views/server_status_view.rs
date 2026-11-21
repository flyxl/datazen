//! Port of `packages/frontend/src/windows/connection/ServerStatusView.tsx`.
//!
//! React is a data-driven server dashboard: a header (connection/version, last
//! updated, auto-refresh select, manual refresh), three view tabs (dashboard /
//! variables / details), and four mutually exclusive bodies (loading, error,
//! empty, loaded). The Rune port keeps every test id, i18n key, card/chart/table
//! row, state and action: the snapshot load, tab change and refresh period are
//! [`ServerStatusAction`]s dispatched through an injected host port — no fake
//! IPC, interval or timer lives in the view.
//!
//! ## Keys
//!
//! `server-status-last-updated`, `server-dashboard-refresh` and the three
//! `server-view-tab-*` handles are React's test ids, kept verbatim (the tab
//! strip uses the `Tabs` base with key `server-view`). The `server-status-*`
//! keys name Rune-side handles for cards, charts and table rows until the host
//! mounts the real engine-backed views.
//!
//! ## Documented deviations
//!
//! * `ChartCanvas` (recharts) is a separate engine; chart cards show the legend
//!   and a point-count marker instead of a drawn plot.
//! * `rune-kit` has no Activity/Users/Gauge glyphs; registered stand-ins carry
//!   the same semantic roles.
//!
//! ## Consumer / removal stage
//!
//! No production host mounts this view yet; the connection-page host owns the
//! VM, runs the snapshot command and maps actions to its state. Until then every
//! reusable item carries a per-item `#[allow(dead_code)]` with this stage.

use std::collections::BTreeMap;
use std::rc::Rc;

use rune::{col, icon, row, text, El};
use rune_kit::{Button, ButtonVariant, ControlSize};

use crate::i18n::t;
use crate::icons::{ICON_CLOCK, ICON_LOADER2, ICON_REFRESH};
use crate::ui::copyable::CopyableError;
use crate::ui::kit::kit;
use crate::ui::tabs::{TabItem, Tabs};

mod format;
mod overview;
mod tables;

pub(crate) use format::{
    AUTO_REFRESH_OPTIONS, DETAIL_ORDER, format_byte_rate, format_bytes, format_clock, format_rate,
    format_uptime, format_value, label_for_key, latest_series_value, metric_label_key, number_from,
};
pub(crate) use overview::{
    chart_cards, chart_point_count, metric_cards, to_chart_point_count, ChartCardVm, MetricCardVm,
};
pub(crate) use tables::detail_rows;

use overview::build as build_dashboard;
use tables::{details, variables};

const ROOT: &str = "server-status-view";
pub(crate) const LAST_UPDATED_KEY: &str = "server-status-last-updated";
pub(crate) const REFRESH_KEY: &str = "server-dashboard-refresh";
pub(crate) const TABS_KEY: &str = "server-view";
pub(crate) const LOADING_KEY: &str = "server-status-loading";
pub(crate) const ERROR_KEY: &str = "server-status-error";
pub(crate) const EMPTY_KEY: &str = "server-status-empty";

/// React `StatusRecord` value: string | number | boolean | null.
#[allow(dead_code)] // VM mirror; supplied by the host snapshot feed.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum StatusValue {
    Str(String),
    Num(f64),
    Bool(bool),
    Null,
}

impl Default for StatusValue {
    fn default() -> Self {
        Self::Null
    }
}

impl StatusValue {
    /// React `value == null || value === ''` — rendered as an em dash.
    #[allow(dead_code)] // pure helper; consumed by card/detail building.
    pub(crate) fn is_absent(&self) -> bool {
        match self {
            Self::Null => true,
            Self::Str(value) => value.is_empty(),
            _ => false,
        }
    }

    /// React `String(value)` for display.
    #[allow(dead_code)] // pure helper; consumed by card/detail building.
    pub(crate) fn display(&self) -> String {
        match self {
            Self::Str(value) => value.clone(),
            Self::Num(value) => {
                if value.fract() == 0.0 && value.abs() < 9_007_199_254_740_992.0 {
                    format!("{}", *value as i64)
                } else {
                    format!("{value}")
                }
            }
            Self::Bool(value) => value.to_string(),
            Self::Null => "—".to_owned(),
        }
    }
}

/// React `StatusVariable` / `TrendPoint`.
#[allow(dead_code)] // VM mirror; supplied by the host snapshot feed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct StatusVariableVm {
    pub name: String,
    pub value: Option<String>,
}

#[allow(dead_code)] // VM mirror; supplied by the host snapshot feed.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct TrendPointVm {
    pub t: i64,
    pub v: f64,
}

/// React `viewTab` union.
#[allow(dead_code)] // VM mirror; consumed by the tab strip and tests.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum ViewTab {
    #[default]
    Dashboard,
    Variables,
    Details,
}

impl ViewTab {
    /// Narrow React's bare `Tabs` string through `SUB_TABS.find(...)`.
    #[allow(dead_code)] // pure helper; consumed by the tab change handler.
    pub(crate) fn from_id(id: &str) -> Option<Self> {
        match id {
            "dashboard" => Some(Self::Dashboard),
            "variables" => Some(Self::Variables),
            "details" => Some(Self::Details),
            _ => None,
        }
    }

    #[allow(dead_code)] // pure helper; consumed by the tab strip.
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Dashboard => "dashboard",
            Self::Variables => "variables",
            Self::Details => "details",
        }
    }
}

/// The full view snapshot the host fills each frame.
#[allow(dead_code)] // VM surface; owned by the connection-page host.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct ServerStatusVm {
    pub db_session_id: String,
    pub connection_name: String,
    /// Whether `initialData.status` was present (React's `!initialData?.status`
    /// guard on the loading body).
    pub has_initial_status: bool,
    pub loading: bool,
    /// Only the manual refresh spins the button (React `buttonLoading`).
    pub button_loading: bool,
    pub last_updated_at: Option<i64>,
    pub error: String,
    pub status: Option<BTreeMap<String, StatusValue>>,
    pub variables: Vec<StatusVariableVm>,
    pub history: BTreeMap<String, Vec<TrendPointVm>>,
    pub auto_refresh: u64,
    /// Rune-only dropdown state for the auto-refresh select.
    pub auto_refresh_open: bool,
    pub view_tab: ViewTab,
}

/// Every host intent the view can emit.
#[allow(dead_code)] // host port; remove when the connection host wires the panel.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum ServerStatusAction {
    /// Initial/session load and auto poll (`manual = false`).
    Load { db_session_id: String, manual: bool },
    /// Manual refresh from the toolbar button.
    Refresh { db_session_id: String },
    SetAutoRefresh(u64),
    ToggleAutoRefreshOptions,
    SelectTab(ViewTab),
}

type ActionFn<S> = Rc<dyn Fn(&mut S, ServerStatusAction) + 'static>;

fn emit<S: 'static>(on_action: &Option<ActionFn<S>>, action: ServerStatusAction) -> impl Fn(&mut S) + Clone {
    let callback = on_action.clone();
    move |s: &mut S| {
        if let Some(callback) = &callback {
            callback(s, action.clone());
        }
    }
}

/// Build the view into one `El`.
#[allow(dead_code)] // view entry; remove when the connection host wires the panel.
pub(crate) fn build<S: 'static>(
    vm: &ServerStatusVm,
    on_action: Option<ActionFn<S>>,
    dark: bool,
) -> El {
    // The three guard bodies are mutually exclusive, matching React's order.
    if vm.loading && vm.status.is_none() && !vm.has_initial_status {
        return loading_body(dark);
    }
    if !vm.error.is_empty() && vm.status.is_none() {
        return error_body::<S>(&vm.error, dark);
    }
    let status = match &vm.status {
        Some(status) => status,
        None => return empty_body(dark),
    };

    let k = kit(dark);
    let mut root = col().key(ROOT).w_full().h_full().min_h(0.0).gap(0.0);

    // ── Header ──────────────────────────────────────────────────────────────
    let mut header = row()
        .key("server-status-header")
        .w_full()
        .h(46.0)
        .shrink()
        .px(14.0)
        .gap(8.0)
        .center_y()
        .border_sides([0.0, 0.0, 1.0, 0.0], k.c.border_base)
        .child(icon(&ICON_REFRESH, 16.0).color(k.c.text_accent))
        .child(text(t("serverStatus.dashboardTitle")).font(k.ty.text_small).color(k.c.text_base));
    if !vm.connection_name.is_empty() {
        header = header.child(
            row().key("server-status-connection").px(6.0).py(2.0).fill(k.c.layer_02).radius(4.0)
                .child(text(vm.connection_name.clone()).font(k.ty.text_xsmall).color(k.c.text_base)),
        );
    }
    if let Some(version) = status.get("version").filter(|v| !v.is_absent()) {
        header = header.child(
            row().key("server-status-version").px(6.0).py(2.0).fill(k.c.layer_02).radius(4.0)
                .child(text(version.display()).font(k.ty.text_xsmall).color(k.c.text_muted)),
        );
    }
    header = header.child(row().grow(1.0));
    if let Some(updated) = vm.last_updated_at {
        header = header.child(
            row().key(LAST_UPDATED_KEY).gap(4.0).center_y().help(t("serverStatus.lastUpdatedTitle"))
                .child(icon(&ICON_CLOCK, 12.0).color(k.c.text_muted))
                .child(text(format_clock(updated)).font(k.ty.text_xsmall).color(k.c.text_muted)),
        );
    }
    header = header.child(auto_refresh_trigger(vm, &on_action, dark));
    header = header.child(refresh_button(vm, &on_action, dark));
    root = root.child(header);

    // The dropdown is a Rune-only stand-in for the native Select; mounting it
    // beside the header (not inside it) keeps the 46pt header strip intact.
    if vm.auto_refresh_open {
        let mut menu = row()
            .key("server-status-auto-refresh-menu")
            .role("menu")
            .w_full()
            .h(24.0)
            .gap(4.0)
            .center_y()
            .justify_end()
            .px(14.0);
        for (value, key) in AUTO_REFRESH_OPTIONS {
            menu = menu.child(
                row()
                    .key(format!("server-status-auto-refresh-{value}"))
                    .role("menuitem")
                    .focusable(true)
                    .px(6.0)
                    .h(22.0)
                    .center_y()
                    .child(text(t(key).to_owned()).font(k.ty.text_xsmall).color(k.c.text_base))
                    .on_press(emit(&on_action, ServerStatusAction::SetAutoRefresh(value))),
            );
        }
        root = root.child(menu);
    }

    // ── Tabs (bar-only, same three test ids) ────────────────────────────────
    let items = vec![
        TabItem::new("dashboard", t("serverStatus.dashboardTitle")),
        TabItem::new("variables", t("serverStatus.statusVarsTitle")),
        TabItem::new("details", t("serverStatus.detailTitle")),
    ];
    let mut tabs = Tabs::new(TABS_KEY, items, vm.view_tab.as_str())
        .aria_label(t("serverStatus.dashboardTitle"));
    if let Some(on_action) = on_action.clone() {
        tabs = tabs.on_change(move |s: &mut S, id: String| {
            if let Some(tab) = ViewTab::from_id(&id) {
                on_action(s, ServerStatusAction::SelectTab(tab));
            }
        });
    }
    root = root.child(tabs.build(dark).border_sides([0.0, 0.0, 1.0, 0.0], k.c.border_base));

    // ── Active body ─────────────────────────────────────────────────────────
    let body = match vm.view_tab {
        ViewTab::Dashboard => build_dashboard(vm, dark),
        ViewTab::Variables => variables(vm, dark),
        ViewTab::Details => details(vm, dark),
    };
    root.child(
        row().key("server-status-body").w_full().grow(1.0).min_h(0.0).padding(14.0).child(body),
    )
}

fn auto_refresh_trigger<S: 'static>(
    vm: &ServerStatusVm,
    on_action: &Option<ActionFn<S>>,
    dark: bool,
) -> El {
    let k = kit(dark);
    let current = AUTO_REFRESH_OPTIONS
        .iter()
        .find(|(value, _)| *value == vm.auto_refresh)
        .map(|(_, key)| t(key))
        .unwrap_or("");
    row()
        .key("server-status-auto-refresh-trigger")
        .w(64.0)
        .h(24.0)
        .px(6.0)
        .center_y()
        .fill(k.c.layer_01)
        .border(1.0, k.c.border_base)
        .radius(4.0)
        .focusable(true)
        .child(text(t("serverStatus.autoRefresh")).font(k.ty.text_xsmall).color(k.c.text_muted))
        .child(text(current.to_owned()).font(k.ty.text_xsmall).color(k.c.text_base))
        .on_press(emit(on_action, ServerStatusAction::ToggleAutoRefreshOptions))
}

fn refresh_button<S: 'static>(
    vm: &ServerStatusVm,
    on_action: &Option<ActionFn<S>>,
    dark: bool,
) -> El {
    let k = kit(dark);
    let mut button = Button::new(t("serverStatus.refresh"))
        .variant(ButtonVariant::Outline)
        .control_size(ControlSize::Small)
        .icon(if vm.button_loading { &ICON_LOADER2 } else { &ICON_REFRESH })
        .disabled(vm.button_loading);
    if let Some(on_action) = on_action.clone() {
        let session = vm.db_session_id.clone();
        button = button.on_press(move |s: &mut S| {
            on_action(s, ServerStatusAction::Refresh { db_session_id: session.clone() })
        });
    }
    button.build(k).key(REFRESH_KEY)
}

fn loading_body(dark: bool) -> El {
    let k = kit(dark);
    col()
        .key(LOADING_KEY)
        .w_full()
        .h_full()
        .center()
        .gap(8.0)
        .child(icon(&ICON_LOADER2, 18.0).color(k.c.text_muted))
        .child(text(t("serverStatus.loading")).font(k.ty.text_small).color(k.c.text_muted))
}

fn error_body<S: 'static>(error: &str, dark: bool) -> El {
    let k = kit(dark);
    col()
        .key(ERROR_KEY)
        .w_full()
        .h_full()
        .center()
        .child(CopyableError::new("server-status-error-copy", error.to_owned()).build::<S>(k))
}

fn empty_body(dark: bool) -> El {
    let k = kit(dark);
    col()
        .key(EMPTY_KEY)
        .w_full()
        .h_full()
        .center()
        .child(text(t("serverStatus.empty")).font(k.ty.text_small).color(k.c.text_muted))
}
