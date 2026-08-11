import { describe, expect, it, vi, beforeEach, afterEach } from 'vitest';
import { render, cleanup, fireEvent, screen } from '@testing-library/react';
import { ConnectionSettingsDialog } from '../ConnectionSettingsDialog';
import type { AppSettings } from '../../../types';

const updateSettingsMock = vi.fn().mockResolvedValue(undefined);

const baseSettings = {
  theme: { mode: 'dark' as const, packId: null },
  language: 'zh-CN',
  limitSelectResults: true,
  queryResultLimit: 5000,
  editorFontSize: 13,
  editorFontFamily: 'Menlo',
  confirmOnDelete: true,
  autoCommit: true,
  defaultPageSize: 50,
    connectionPoolSize: 10,
  checkForUpdatesOnStartup: true,
  logLevel: 'info' as const,
  logPath: '',
  mcpServerEnabled: false,
  mcpDisabledTools: [] as string[],
  mcpPermissionMode: 'read_only' as const,
  contextDir: '',
  pluginSettings: {},
  monitor: {
    enabled: false,
    pollIntervalSecs: 60,
    retentionDays: 7,
    trayEnabled: false,
    alertsEnabled: false,
  },
} as AppSettings;

vi.mock('../../../hooks/useI18n', () => ({
  useI18n: () => ({ t: (key: string) => key }),
}));

vi.mock('../../../stores/settingsStore', () => ({
  useSettingsStore: (
    sel: (s: { settings: AppSettings; updateSettings: typeof updateSettingsMock }) => unknown,
  ) => sel({ settings: baseSettings, updateSettings: updateSettingsMock }),
}));

beforeEach(() => {
  vi.clearAllMocks();
});

afterEach(cleanup);

describe('ConnectionSettingsDialog', () => {
  it('renders nothing when closed', () => {
    const { container } = render(
      <ConnectionSettingsDialog open={false} onClose={vi.fn()} />,
    );
    expect(container.innerHTML).toBe('');
  });

  it('shows settings fields when open and saves on primary click', async () => {
    const onClose = vi.fn();
    render(<ConnectionSettingsDialog open onClose={onClose} />);

    expect(screen.getByText('connSettings.title')).toBeInTheDocument();
    expect(screen.getByText('settings.theme')).toBeInTheDocument();
    expect(screen.getByText('settings.defaultPageSize')).toBeInTheDocument();

    fireEvent.click(screen.getByText('common.save'));
    await Promise.resolve();
    expect(updateSettingsMock).toHaveBeenCalled();
    expect(onClose).toHaveBeenCalled();
  });

  it('toggles limitSelect and updates font size before save', async () => {
    const onClose = vi.fn();
    render(<ConnectionSettingsDialog open onClose={onClose} />);

    const checkboxes = screen.getAllByRole('checkbox');
    // limitSelectResults is first checkbox
    fireEvent.click(checkboxes[0]);
    const range = document.querySelector('input[type="range"]') as HTMLInputElement;
    fireEvent.change(range, { target: { value: '18' } });
    expect(screen.getByText('18px')).toBeInTheDocument();

    const fontInput = document.querySelector('input[type="text"]') as HTMLInputElement;
    fireEvent.change(fontInput, { target: { value: 'Fira Code' } });

    fireEvent.click(checkboxes[1]); // confirmOnDelete
    fireEvent.click(checkboxes[2]); // autoCommit

    fireEvent.click(screen.getByText('common.save'));
    await Promise.resolve();
    expect(updateSettingsMock).toHaveBeenCalled();
    const saved = updateSettingsMock.mock.calls[0][0] as AppSettings;
    expect(saved.editorFontSize).toBe(18);
    expect(saved.editorFontFamily).toBe('Fira Code');
  });
});
