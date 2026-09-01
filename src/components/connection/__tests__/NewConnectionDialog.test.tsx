import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { render, screen, fireEvent, cleanup, waitFor, act } from '@testing-library/react';
import { NewConnectionDialog, ConnectionEditorDialogHost } from '../NewConnectionDialog';
import {
  closeNewConnectionDialog,
  openNewConnectionDialog,
  useConnectionEditorStore,
} from '../../../lib/connectionEditor';
import type { ConnectionConfig } from '../../../types';

const saveConnectionMock = vi.fn().mockResolvedValue(undefined);
const fetchConnectionsMock = vi.fn();
const fetchGroupsMock = vi.fn();
const loadSettingsMock = vi.fn();

const { mockConnections, mockGroups } = vi.hoisted(() => ({
  mockConnections: [] as ConnectionConfig[],
  mockGroups: ['development'] as string[],
}));

vi.mock('../../../hooks/useI18n', () => ({
  useI18n: () => ({ t: (key: string) => key }),
}));

vi.mock('../../../hooks/useSettings', () => ({
  useSettings: vi.fn(),
}));

vi.mock('../../../stores/settingsStore', () => ({
  useSettingsStore: (sel: (s: { loadSettings: typeof loadSettingsMock }) => unknown) =>
    sel({ loadSettings: loadSettingsMock }),
}));

vi.mock('../../../stores/connectionStore', () => ({
  useConnectionStore: Object.assign(
    vi.fn((selector: (s: Record<string, unknown>) => unknown) =>
      selector({
        fetchConnections: fetchConnectionsMock,
        fetchGroups: fetchGroupsMock,
        connections: mockConnections,
        groups: mockGroups,
        saveConnection: saveConnectionMock,
      }),
    ),
    {
      getState: () => ({
        testConnection: vi.fn(),
        saveConnection: saveConnectionMock,
      }),
    },
  ),
}));

vi.mock('../../../commands/connection', () => ({
  connectionCommands: {
    getAvailableDrivers: vi.fn().mockResolvedValue(['postgresql', 'mysql', 'sqlite']),
  },
}));

vi.mock('../useConnectionClipboardFill', () => ({
  useConnectionClipboardFill: vi.fn(),
}));

describe('NewConnectionDialog', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mockConnections.length = 0;
    mockGroups.length = 0;
    mockGroups.push('development');
    useConnectionEditorStore.setState({ open: false, editId: null, openSeq: 0 });
  });

  afterEach(cleanup);

  it('does not close when clicking the backdrop', () => {
    render(<NewConnectionDialog open />);
    expect(screen.getByTestId('new-connection-dialog')).toBeInTheDocument();
    fireEvent.click(screen.getByTestId('new-connection-dialog-backdrop'));
    expect(screen.getByTestId('new-connection-dialog')).toBeInTheDocument();
  });

  it('resets form when reopened in create mode via ConnectionEditorDialogHost', async () => {
    render(<ConnectionEditorDialogHost />);

    act(() => openNewConnectionDialog());
    const nameInput = screen.getByPlaceholderText('newConn.namePlaceholder');
    fireEvent.change(nameInput, { target: { value: 'My Test Connection' } });
    expect(nameInput).toHaveValue('My Test Connection');

    fireEvent.click(screen.getByText('common.save'));
    await waitFor(() => expect(saveConnectionMock).toHaveBeenCalled());
    await waitFor(() =>
      expect(screen.queryByTestId('new-connection-dialog')).not.toBeInTheDocument(),
    );

    act(() => openNewConnectionDialog());
    const freshNameInput = screen.getByPlaceholderText('newConn.namePlaceholder');
    expect(freshNameInput).toHaveValue('');
    expect(freshNameInput).not.toHaveValue('My Test Connection');
  });

  it('shows default fields on each create open', async () => {
    render(<ConnectionEditorDialogHost />);

    act(() => openNewConnectionDialog());
    fireEvent.change(screen.getByPlaceholderText('newConn.namePlaceholder'), {
      target: { value: 'Filled once' },
    });
    act(() => closeNewConnectionDialog());

    act(() => openNewConnectionDialog());
    expect(screen.getByPlaceholderText('newConn.namePlaceholder')).toHaveValue('');
    expect(screen.getByDisplayValue('127.0.0.1')).toBeInTheDocument();
    expect(screen.getByDisplayValue('5432')).toBeInTheDocument();
    expect(screen.queryByTestId('new-conn-ssl-mode')).not.toBeInTheDocument();
    expect(screen.queryByTestId('new-conn-ssh-tunnel-checkbox')).not.toBeInTheDocument();

    fireEvent.click(screen.getByTestId('new-conn-advanced-toggle'));
    expect(screen.getByTestId('new-conn-ssl-mode')).toBeInTheDocument();
    expect(screen.queryByTestId('new-conn-ssh-tunnel-checkbox')).not.toBeInTheDocument();

    fireEvent.click(screen.getByTestId('new-conn-ssh-toggle'));
    expect(screen.getByTestId('new-conn-ssh-tunnel-checkbox')).toBeInTheDocument();
  });

  it('loads existing connection in edit mode', async () => {
    mockConnections.push({
      id: 'cfg-1',
      name: 'Prod DB',
      databaseType: 'postgresql',
      host: 'db.example.com',
      port: 5432,
      database: 'app',
      username: 'admin',
      sslMode: 'require',
      group: 'production',
    });

    render(<ConnectionEditorDialogHost />);
    act(() => openNewConnectionDialog('cfg-1'));

    await waitFor(() => {
      expect(screen.getByDisplayValue('Prod DB')).toBeInTheDocument();
    });
    expect(screen.getByDisplayValue('db.example.com')).toBeInTheDocument();
    expect(screen.getByDisplayValue('admin')).toBeInTheDocument();
  });
});
