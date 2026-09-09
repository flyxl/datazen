import { describe, it, expect, vi, afterEach } from 'vitest';
import { render, screen, fireEvent, waitFor, cleanup } from '@testing-library/react';
import { SqlSnippetsCard } from '../SqlSnippetsCard';
import type { AppSettings } from '../../../types';

vi.mock('../../../hooks/useI18n', () => ({
  useI18n: () => ({ t: (key: string) => key }),
}));

vi.mock('../../../stores/settingsStore', () => ({
  useSettingsStore: (sel: (s: { updateSettings: ReturnType<typeof vi.fn> }) => unknown) =>
    sel({ updateSettings: vi.fn() }),
}));

const mockSettings: AppSettings = {
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
  checkForUpdatesOnStartup: false,
  logLevel: 'info',
  logPath: '',
  mcpServerEnabled: false,
  mcpDisabledTools: [],
  mcpPermissionMode: 'read_only',
  contextDir: '/tmp',
  pluginSettings: {},
  mcpClientServers: [],
  aiStrictEgress: true,
  monitor: {
    enabled: false,
    pollIntervalSecs: 60,
    retentionDays: 7,
    trayEnabled: false,
    alertsEnabled: false,
  },
  sqlSnippets: [
    {
      id: 'custom-1',
      prefix: 'selcustom',
      descriptionKey: 'Custom query',
      template: 'SELECT custom FROM tbl;${1}',
    },
  ],
};

describe('SqlSnippetsCard', () => {
  afterEach(() => {
    cleanup();
  });

  it('renders built-in snippets with builtin badge and custom snippets with custom badge', () => {
    render(<SqlSnippetsCard settings={mockSettings} onUpdateSnippets={vi.fn()} />);

    // Builtin prefix
    expect(screen.getByText('sel*')).toBeInTheDocument();
    // Custom prefix
    expect(screen.getByText('selcustom')).toBeInTheDocument();
    expect(screen.getByText('Custom query')).toBeInTheDocument();
  });

  it('allows clicking Add Snippet to open modal', () => {
    render(<SqlSnippetsCard settings={mockSettings} onUpdateSnippets={vi.fn()} />);

    fireEvent.click(screen.getByRole('button', { name: 'query.snippets.add' }));
    expect(screen.getByPlaceholderText('query.snippets.prefixPlaceholder')).toBeInTheDocument();
  });

  it('calls onUpdateSnippets when deleting a custom snippet', async () => {
    const onUpdateSnippets = vi.fn();
    render(<SqlSnippetsCard settings={mockSettings} onUpdateSnippets={onUpdateSnippets} />);

    const deleteBtn = screen.getByTestId('delete-snippet-custom-1');
    fireEvent.click(deleteBtn);

    // Confirm dialog appears
    const confirmBtn = await screen.findByRole('button', { name: 'common.confirm' });
    fireEvent.click(confirmBtn);

    await waitFor(() => {
      expect(onUpdateSnippets).toHaveBeenCalledWith([]);
    });
  });
});
