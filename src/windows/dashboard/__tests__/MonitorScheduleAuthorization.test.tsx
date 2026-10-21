import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { MonitorScheduleAuthorization } from '../MonitorScheduleAuthorization';
import { dashboardCommands } from '../../../commands/dashboard';
vi.mock('../../../hooks/useI18n', () => ({ useI18n: () => ({ t: (key: string) => key }) }));
vi.mock('../../../commands/dashboard', () => ({
  dashboardCommands: {
    getMonitorScheduleGrant: vi.fn(),
    authorizeMonitorSchedule: vi.fn(),
    revokeMonitorSchedule: vi.fn(),
  },
}));
const grant = {
  id: 'backend-issued',
  expiresAt: '2099-01-01T00:00:00Z',
  revoked: false,
  workflowRevision: 'frozen',
};
beforeEach(() => {
  vi.clearAllMocks();
  vi.mocked(dashboardCommands.getMonitorScheduleGrant).mockResolvedValue(null);
  vi.mocked(dashboardCommands.authorizeMonitorSchedule).mockResolvedValue(grant);
  vi.mocked(dashboardCommands.revokeMonitorSchedule).mockResolvedValue(undefined);
});
afterEach(cleanup);
describe('Monitor schedule authorization', () => {
  it('requires explicit approval and supports revoke without running a widget', async () => {
    render(<MonitorScheduleAuthorization dashboardId="d" widgetId="w" dirty={false} />);
    await waitFor(() =>
      expect(dashboardCommands.getMonitorScheduleGrant).toHaveBeenCalledWith('d', 'w'),
    );
    expect(dashboardCommands.authorizeMonitorSchedule).not.toHaveBeenCalled();
    fireEvent.click(screen.getByText('dashboard.scheduleAuthorizeHour'));
    await waitFor(() => expect(screen.getByText('dashboard.scheduleAuthorizedUntil')).toBeTruthy());
    expect(dashboardCommands.authorizeMonitorSchedule).toHaveBeenCalledWith('d', 'w');
    fireEvent.click(screen.getByText('dashboard.scheduleRevoke'));
    await waitFor(() =>
      expect(dashboardCommands.revokeMonitorSchedule).toHaveBeenCalledWith('d', 'w'),
    );
    await waitFor(() => expect(screen.getByText('dashboard.scheduleUnauthorized')).toBeTruthy());
  });
  it('requires saved configuration and exposes backend scope rejection', async () => {
    const { rerender } = render(
      <MonitorScheduleAuthorization dashboardId="d" widgetId="w" dirty />,
    );
    expect(
      (screen.getByText('dashboard.scheduleAuthorizeHour') as HTMLButtonElement).disabled,
    ).toBe(true);
    vi.mocked(dashboardCommands.authorizeMonitorSchedule).mockRejectedValueOnce(
      'explicit database required',
    );
    rerender(<MonitorScheduleAuthorization dashboardId="d" widgetId="w" dirty={false} />);
    fireEvent.click(screen.getByText('dashboard.scheduleAuthorizeHour'));
    await waitFor(() =>
      expect(screen.getByRole('alert').textContent).toBe('explicit database required'),
    );
  });
});
