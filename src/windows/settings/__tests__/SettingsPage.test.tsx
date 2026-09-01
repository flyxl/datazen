import { describe, expect, it, vi, beforeEach, afterEach } from 'vitest';
import { render, cleanup, fireEvent, screen, waitFor } from '@testing-library/react';
import { SettingsPage } from '../SettingsPage';

const onBackMock = vi.fn();

vi.mock('../../../hooks/useI18n', () => ({
  useI18n: () => ({ t: (key: string) => key }),
}));

vi.mock('../../../hooks/useSettings', () => ({
  useSettings: () => {},
}));

vi.mock('../../../stores/settingsStore', () => ({
  useSettingsStore: (sel: (s: Record<string, unknown>) => unknown) =>
    sel({
      settings: {
        theme: { mode: 'dark', packId: null },
        language: 'en',
        limitSelectResults: true,
        queryResultLimit: 5000,
        editorFontSize: 13,
        editorFontFamily: 'Menlo',
        confirmOnDelete: true,
        autoCommit: true,
        safeMode: true,
        defaultPageSize: 50,
        connectionPoolSize: 10,
        checkForUpdatesOnStartup: true,
        logLevel: 'info',
        logPath: '',
        autoChartOnQuery: false,
      },
      loadSettings: vi.fn().mockResolvedValue(undefined),
      updateSettings: vi.fn().mockResolvedValue(undefined),
    }),
}));

vi.mock('../../../commands/settings', () => ({
  settingsCommands: {
    getLogPath: vi.fn().mockResolvedValue('/tmp/logs'),
    openLogDir: vi.fn(),
  },
}));

vi.mock('../../../components/TitleBar', () => ({
  TitleBar: ({
    title,
    rightContent,
  }: {
    title?: React.ReactNode;
    rightContent?: React.ReactNode;
  }) => (
    <div data-testid="title-bar">
      <span>{title}</span>
      {rightContent}
    </div>
  ),
}));

vi.mock('../../../components/ThemeToggle', () => ({
  ThemeToggle: () => <div data-testid="theme-toggle">theme</div>,
}));

vi.mock('../SettingsContent', () => ({
  SettingsContent: ({
    initialSection,
    onBack,
    onDirtyChange,
  }: {
    initialSection?: string;
    onBack?: () => void;
    onDirtyChange?: (dirty: boolean) => void;
  }) => (
    <div data-testid="settings-content-mock">
      section={initialSection ?? 'general'}
      <button type="button" data-testid="settings-mark-dirty" onClick={() => onDirtyChange?.(true)}>
        mark dirty
      </button>
      {onBack && (
        <button type="button" data-testid="settings-back" onClick={onBack}>
          common.back
        </button>
      )}
    </div>
  ),
}));

beforeEach(() => {
  vi.clearAllMocks();
});

afterEach(() => {
  cleanup();
});

describe('SettingsPage', () => {
  it('renders page shell with back button and settings content', () => {
    render(<SettingsPage initialSection="ai" onBack={onBackMock} />);

    expect(screen.getByTestId('settings-page')).toBeInTheDocument();
    expect(screen.getByTestId('settings-back')).toBeInTheDocument();
    expect(screen.getByText('common.back')).toBeInTheDocument();
    expect(screen.getByTestId('settings-content-mock')).toHaveTextContent('section=ai');
    expect(screen.getByText('win.settings')).toBeInTheDocument();
    expect(
      screen.queryByTestId('title-bar')?.querySelector('[data-testid="settings-back"]'),
    ).toBeNull();
  });

  it('calls onBack when back button is clicked', () => {
    render(<SettingsPage onBack={onBackMock} />);

    fireEvent.click(screen.getByTestId('settings-back'));
    expect(onBackMock).toHaveBeenCalledTimes(1);
  });

  it('confirms before leaving with unsaved changes', async () => {
    render(<SettingsPage onBack={onBackMock} />);

    fireEvent.click(screen.getByTestId('settings-mark-dirty'));
    fireEvent.click(screen.getByTestId('settings-back'));

    expect(screen.getByText('settings.unsavedChangesTitle')).toBeInTheDocument();
    expect(onBackMock).not.toHaveBeenCalled();

    fireEvent.click(screen.getByTestId('confirm-dialog-ok'));
    await waitFor(() => expect(onBackMock).toHaveBeenCalledTimes(1));
  });

  it('defaults initialSection to general when omitted', () => {
    render(<SettingsPage onBack={onBackMock} />);

    expect(screen.getByTestId('settings-content-mock')).toHaveTextContent('section=general');
  });

  it('renders theme toggle in title bar', () => {
    render(<SettingsPage onBack={onBackMock} />);

    expect(screen.getByTestId('theme-toggle')).toBeInTheDocument();
  });
});
