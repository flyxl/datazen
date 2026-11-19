//! The `windows/dashboard` subtree ports (AddToDashboardDialog is its first;
//! the drawers / DashboardPanel land with their own tasks).

pub(crate) mod add_to_dashboard_dialog;
// T069: the chart widget tile (header switches, the eight body states, the
// expand overlay with the axis-configurator seam).
pub(crate) mod chart_widget_tile;
// T146: the scheduled-refresh authorization card (grant load, the
// authorized-until status line, the authorize/revoke guards).
pub(crate) mod monitor_schedule_authorization;
