import { dashboardCommands } from '../commands/dashboard';
import type { Dashboard } from '../types/dashboard';

export function runDashboardWidgetsOnce(dashboard: Dashboard): void {
  void Promise.all(
    dashboard.widgets
      .filter((widget) => widget.enabled)
      .map((widget) => dashboardCommands.runDashboardWidget(dashboard.id, widget.id).catch(() => undefined)),
  );
}
