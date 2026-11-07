import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { act, cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
import type { UpdateCheckResult, UpdateProgress } from '../../../platform/contracts';

const mocks = vi.hoisted(() => ({
  channel: vi.fn(),
  check: vi.fn(),
  install: vi.fn(),
}));

vi.mock('../../../hooks/useI18n', () => ({
  useI18n: () => ({
    t: (key: string) => {
      const labels: Record<string, string> = {
        'settings.updater.sidebarAvailable': 'Update {version}',
        'settings.updater.downloading': 'Downloading… {pct}%',
        'settings.updater.installing': 'Installing…',
        'settings.updater.sidebarInstalling': 'Updating…',
      };
      return labels[key] ?? key;
    },
  }),
}));

vi.mock('../../../components/ThemedIcon', () => ({
  ThemedIcon: () => <span aria-hidden="true" />,
}));

vi.mock('../../../lib/updater', () => ({
  getUpdateChannel: mocks.channel,
  checkForUpdates: mocks.check,
  downloadAndInstallUpdate: mocks.install,
}));

import { WorkspaceModeSidebar } from '../WorkspaceModeSidebar';
import { useUpdateReminderStore } from '../../../stores/updateReminderStore';
import { startUpdateReminderSchedule } from '../../../lib/updateReminderSchedule';

const props = {
  workspaceMode: 'connections' as const,
  sidebarExpanded: true,
  onSetWorkspaceMode: vi.fn(),
  onOpenWorkflow: vi.fn(),
  onOpenDashboard: vi.fn(),
  onOpenSettings: vi.fn(),
  onToggleSidebarMode: vi.fn(),
};
let stopSchedule: (() => void) | undefined;

function renderSidebar() {
  return render(<WorkspaceModeSidebar {...props} />);
}

beforeEach(() => {
  vi.resetAllMocks();
  mocks.channel.mockReturnValue('auto');
  mocks.check.mockResolvedValue({ status: 'upToDate' } satisfies UpdateCheckResult);
  useUpdateReminderStore.setState(useUpdateReminderStore.getInitialState(), true);
});

afterEach(() => {
  stopSchedule?.();
  stopSchedule = undefined;
  cleanup();
  vi.useRealTimers();
});

describe('WorkspaceModeSidebar update reminder', () => {
  it('keeps the reminder hidden until the store finds a newer version', () => {
    renderSidebar();
    expect(screen.queryByTestId('workspace-nav-update')).toBeNull();

    act(() => useUpdateReminderStore.setState({ availableVersion: '0.3.1' }));
    expect(screen.getByTestId('workspace-nav-update')).toHaveAttribute(
      'aria-label',
      'Update 0.3.1',
    );
    expect(
      screen
        .getByTestId('workspace-nav-update')
        .compareDocumentPosition(screen.getByTestId('workspace-nav-settings')) &
        Node.DOCUMENT_POSITION_FOLLOWING,
    ).toBeTruthy();
  });

  it('automatically surfaces the reminder from the startup update check', async () => {
    vi.useFakeTimers();
    vi.setSystemTime(Date.parse('2026-11-06T00:00:00Z'));
    mocks.check.mockResolvedValueOnce({ status: 'available', version: '0.3.1' });
    renderSidebar();

    stopSchedule = startUpdateReminderSchedule();
    await vi.advanceTimersByTimeAsync(0);

    expect(mocks.check).toHaveBeenCalledOnce();
    expect(screen.getByTestId('workspace-nav-update')).toHaveAttribute(
      'aria-label',
      'Update 0.3.1',
    );
  });

  it('shows download progress, blocks repeat clicks, and removes the reminder after install', async () => {
    let finishInstall: ((result: UpdateCheckResult) => void) | undefined;
    mocks.install.mockImplementationOnce((onProgress: (progress: UpdateProgress) => void) => {
      onProgress({ phase: 'downloading', downloaded: 5, total: 10 });
      return new Promise((resolve) => {
        finishInstall = resolve;
      });
    });
    useUpdateReminderStore.setState({ availableVersion: '0.3.1' });
    renderSidebar();

    fireEvent.click(screen.getByTestId('workspace-nav-update'));
    expect(mocks.install).toHaveBeenCalledOnce();
    expect(screen.getByTestId('workspace-nav-update')).toBeDisabled();
    expect(screen.getByTestId('workspace-nav-update')).toHaveAttribute(
      'aria-label',
      'Downloading… 50%',
    );

    finishInstall?.({ status: 'installed', version: '0.3.1' });
    await waitFor(() => expect(screen.queryByTestId('workspace-nav-update')).toBeNull());
  });

  it('keeps the update button available and exposes the failure after an install error', async () => {
    mocks.install.mockResolvedValueOnce({ status: 'error', message: 'signature check failed' });
    useUpdateReminderStore.setState({ availableVersion: '0.3.1' });
    renderSidebar();

    fireEvent.click(screen.getByTestId('workspace-nav-update'));
    await waitFor(() =>
      expect(screen.getByTestId('workspace-nav-update')).toHaveAttribute(
        'title',
        'signature check failed',
      ),
    );
    expect(screen.getByTestId('workspace-nav-update')).toBeEnabled();
    expect(mocks.install).toHaveBeenCalledOnce();
  });
});
