import { describe, it, expect, vi, afterEach } from 'vitest';
import { render, screen, fireEvent, waitFor, cleanup } from '@testing-library/react';
import { SqlSnippetsCard } from '../SqlSnippetsCard';
import type { AppSettings } from '../../../types';

vi.mock('../../../hooks/useI18n', () => {
  const t = (key: string) => key;
  return {
    useI18n: () => ({ t }),
  };
});

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
  driverSettings: {},
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

  it('opens duplicate dialog with copied prefix when duplicating a built-in snippet', () => {
    render(<SqlSnippetsCard settings={mockSettings} onUpdateSnippets={vi.fn()} />);

    const duplicateButtons = screen.getAllByRole('button', { name: 'query.snippets.duplicate' });
    fireEvent.click(duplicateButtons[0]);

    expect(screen.getByDisplayValue('sel*_copy')).toBeInTheDocument();
  });

  it('calls onUpdateSnippets when saving a new custom snippet', async () => {
    const onUpdateSnippets = vi.fn().mockResolvedValue(undefined);
    render(<SqlSnippetsCard settings={mockSettings} onUpdateSnippets={onUpdateSnippets} />);

    fireEvent.click(screen.getByRole('button', { name: 'query.snippets.add' }));

    const prefixInput = await screen.findByPlaceholderText('query.snippets.prefixPlaceholder');
    fireEvent.change(prefixInput, { target: { value: 'uniq_custom_snip' } });
    fireEvent.change(screen.getByPlaceholderText('query.snippets.descriptionPlaceholder'), {
      target: { value: 'New snippet' },
    });
    fireEvent.change(screen.getByPlaceholderText('query.snippets.templatePlaceholder'), {
      target: { value: 'SELECT 1;${1}' },
    });

    fireEvent.click(screen.getByRole('button', { name: 'common.save' }));

    await waitFor(() => {
      expect(onUpdateSnippets).toHaveBeenCalledTimes(1);
    });

    expect(onUpdateSnippets).toHaveBeenCalledWith([
      ...mockSettings.sqlSnippets!,
      expect.objectContaining({
        prefix: 'uniq_custom_snip',
        descriptionKey: 'New snippet',
        template: 'SELECT 1;${1}',
      }),
    ]);
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
