import { describe, expect, it, vi, beforeEach, afterEach } from 'vitest';
import { render, cleanup, waitFor, fireEvent, screen } from '@testing-library/react';
import { BackupWindow } from '../BackupWindow';
import { PRESET_GROUPS } from '../../../lib/connectionGroups';
import type { ConnectionConfig } from '../../../types';

const { invokeMock, loadSettingsMock, urlParamMock, confirmDialogFn, listenMock } = vi.hoisted(
  () => ({
    invokeMock: vi.fn(),
    loadSettingsMock: vi.fn().mockResolvedValue(undefined),
    urlParamMock: vi.fn((name: string) => (name === 'mode' ? null : null)),
    confirmDialogFn: vi.fn().mockResolvedValue(true),
    listenMock: vi.fn(async () => () => {}),
  }),
);

vi.mock('../../../hooks/useI18n', () => ({
  useI18n: () => ({
    t: (key: string, params?: Record<string, string | number>) =>
      params ? `${key}:${JSON.stringify(params)}` : key,
  }),
}));

vi.mock('../../../hooks/useSettings', () => ({
  useSettings: () => {},
}));

vi.mock('../../../stores/settingsStore', () => ({
  useSettingsStore: (sel: (s: { loadSettings: () => Promise<void> }) => unknown) =>
    sel({ loadSettings: loadSettingsMock }),
}));

vi.mock('../../../lib/databaseTypes', () => ({
  DB_REGISTRY: {
    postgresql: { supportsBackup: true },
    redis: { supportsBackup: false },
  },
  getDbLabel: (type: string) => type,
}));

vi.mock('../../../components/DbTypeBadge', () => ({
  DbTypeBadge: () => <span>badge</span>,
}));

vi.mock('../../../components/TitleBar', () => ({
  TitleBar: ({ title }: { title?: string }) => <div>{title}</div>,
}));

vi.mock('../../../lib/sqlDialects', () => ({
  getSqlDialect: () => ({ backupOptions: [] }),
}));

vi.mock('@tauri-apps/api/core', () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));

vi.mock('@tauri-apps/api/event', () => ({
  listen: (...args: unknown[]) => listenMock(...args),
}));

vi.mock('../../../hooks/useConfirmDialog', () => ({
  useConfirmDialog: () => [confirmDialogFn, null],
}));

vi.mock('../../../lib/windowKind', () => ({
  getUrlParam: (name: string) => urlParamMock(name),
}));

/** jsdom has no layout; render all virtual rows so log content is visible. */
vi.mock('@tanstack/react-virtual', () => ({
  useVirtualizer: ({
    count,
    estimateSize,
  }: {
    count: number;
    estimateSize?: (i: number) => number;
  }) => {
    const sizeOf = (i: number) => estimateSize?.(i) ?? 20;
    let offset = 0;
    const items = Array.from({ length: count }, (_, index) => {
      const size = sizeOf(index);
      const start = offset;
      offset += size;
      return { index, key: index, start, size, end: start + size };
    });
    return {
      getTotalSize: () => offset || count * 20,
      getVirtualItems: () => items,
      scrollToIndex: () => {},
    };
  },
}));

function conn(
  partial: Partial<ConnectionConfig> & Pick<ConnectionConfig, 'id' | 'name' | 'databaseType'>,
): ConnectionConfig {
  return {
    sslMode: 'prefer',
    ...partial,
  };
}

beforeEach(() => {
  urlParamMock.mockImplementation(() => null);
  listenMock.mockImplementation(async () => () => {});
  invokeMock.mockImplementation(async (cmd: string) => {
    if (cmd === 'get_connections') {
      return [
        conn({
          id: 'pg-1',
          name: 'Local PG',
          databaseType: 'postgresql',
          group: PRESET_GROUPS.development,
          host: 'localhost',
          database: 'app',
        }),
        conn({
          id: 'redis-1',
          name: 'Cache',
          databaseType: 'redis',
          group: PRESET_GROUPS.testing,
          host: 'localhost',
          database: '0',
        }),
        conn({
          id: 'orphan-1',
          name: 'Orphan PG',
          databaseType: 'postgresql',
          group: 'custom-team',
          host: 'localhost',
          database: 'other',
        }),
      ];
    }
    if (cmd === 'get_groups') {
      return [PRESET_GROUPS.development, PRESET_GROUPS.testing];
    }
    return null;
  });
});

afterEach(() => {
  cleanup();
  vi.clearAllMocks();
});

describe('BackupWindow connection list', () => {
  it('shows localized preset groups and every connection including orphan groups', async () => {
    render(<BackupWindow />);
    await waitFor(() => expect(invokeMock).toHaveBeenCalledWith('get_connections'));

    expect(screen.getByText('newConn.groupDev')).toBeInTheDocument();
    expect(screen.getByText('newConn.groupTest')).toBeInTheDocument();
    expect(screen.getByText('custom-team')).toBeInTheDocument();
    expect(screen.queryByText('preset:development')).not.toBeInTheDocument();

    expect(screen.getByText('Local PG')).toBeInTheDocument();
    expect(screen.getByText('Cache')).toBeInTheDocument();
    expect(screen.getByText('Orphan PG')).toBeInTheDocument();
  });

  it('does not connect when selecting a non-backupable type', async () => {
    render(<BackupWindow />);
    await waitFor(() => screen.getByText('Cache'));
    fireEvent.click(screen.getByText('Cache'));
    await waitFor(() =>
      expect(screen.getByTestId('backup-status')).toHaveTextContent('backup.unsupportedType'),
    );
    expect(invokeMock).not.toHaveBeenCalledWith('connect', expect.anything());
  });

  function mockRestoreCommands(tables: unknown[] = []) {
    urlParamMock.mockImplementation((name: string) => (name === 'mode' ? 'restore' : null));
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === 'get_connections') {
        return [
          conn({
            id: 'pg-1',
            name: 'Local PG',
            databaseType: 'postgresql',
            group: PRESET_GROUPS.development,
            host: 'localhost',
            database: 'app',
          }),
        ];
      }
      if (cmd === 'get_groups') return [PRESET_GROUPS.development];
      if (cmd === 'connect') return 'live-1';
      if (cmd === 'get_connection_info') return { serverVersion: '16' };
      if (cmd === 'get_databases') return ['app', 'postgres'];
      if (cmd === 'use_database') return undefined;
      if (cmd === 'get_tables') return tables;
      if (cmd === 'restore_database_with_dialog') return true;
      return null;
    });
  }

  async function selectRestoreTarget() {
    render(<BackupWindow />);
    await waitFor(() => expect(screen.getByText('backup.restoreTitle')).toBeInTheDocument());
    fireEvent.click(screen.getByText('Local PG'));
    await waitFor(() => expect(invokeMock).toHaveBeenCalledWith('connect', { configId: 'pg-1' }));
    await waitFor(() => screen.getByText('app'));
    fireEvent.click(screen.getByText('app'));
    await waitFor(() => expect(screen.getByTestId('backup-start-restore')).not.toBeDisabled());
  }

  it('restore mode shows restore action and waits for connection + database', async () => {
    mockRestoreCommands([]);
    await selectRestoreTarget();
    expect(screen.queryByText('backup.startBackup')).not.toBeInTheDocument();
    fireEvent.click(screen.getByTestId('backup-start-restore'));
    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith('use_database', {
        connectionId: 'live-1',
        database: 'app',
      }),
    );
    expect(confirmDialogFn).not.toHaveBeenCalled();
    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith('restore_database_with_dialog', {
        connectionId: 'live-1',
        database: 'app',
        options: [],
      }),
    );
    await waitFor(() =>
      expect(screen.getByTestId('backup-status')).toHaveTextContent('backup.restoreSuccess'),
    );
    expect(listenMock).toHaveBeenCalledWith('restore-progress', expect.any(Function));
    expect(screen.getByTestId('backup-progress-log')).toHaveTextContent('backup.restoring');
    expect(screen.getByTestId('backup-progress-log')).toHaveTextContent('backup.restoreSuccess');
  });

  it('restore progress log appends each statement', async () => {
    mockRestoreCommands([]);
    let onProgress: ((event: { payload: unknown }) => void) | undefined;
    listenMock.mockImplementation(
      async (_name: string, cb: (event: { payload: unknown }) => void) => {
        onProgress = cb;
        return () => {};
      },
    );
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === 'get_connections') {
        return [
          conn({
            id: 'pg-1',
            name: 'Local PG',
            databaseType: 'postgresql',
            group: PRESET_GROUPS.development,
            host: 'localhost',
            database: 'app',
          }),
        ];
      }
      if (cmd === 'get_groups') return [PRESET_GROUPS.development];
      if (cmd === 'connect') return 'live-1';
      if (cmd === 'get_connection_info') return { serverVersion: '16' };
      if (cmd === 'get_databases') return ['app', 'postgres'];
      if (cmd === 'use_database') return undefined;
      if (cmd === 'get_tables') return [];
      if (cmd === 'restore_database_with_dialog') {
        onProgress?.({
          payload: { current: 1, total: 2, objectName: 'CREATE TABLE users', phase: 'object' },
        });
        onProgress?.({
          payload: { current: 2, total: 2, objectName: 'INSERT INTO users', phase: 'object' },
        });
        return true;
      }
      return null;
    });

    await selectRestoreTarget();
    fireEvent.click(screen.getByTestId('backup-start-restore'));
    await waitFor(() =>
      expect(screen.getByTestId('backup-progress-log')).toHaveTextContent('CREATE TABLE users'),
    );
    expect(screen.getByTestId('backup-progress-log')).toHaveTextContent('INSERT INTO users');
    expect(screen.getByTestId('backup-progress-log-copy')).toBeInTheDocument();
  });

  it('copy log writes all lines to the clipboard', async () => {
    mockRestoreCommands([]);
    const writeText = vi.fn().mockResolvedValue(undefined);
    Object.assign(navigator, { clipboard: { writeText } });
    await selectRestoreTarget();
    fireEvent.click(screen.getByTestId('backup-start-restore'));
    await waitFor(() =>
      expect(screen.getByTestId('backup-progress-log')).toHaveTextContent('backup.restoreSuccess'),
    );
    fireEvent.click(screen.getByTestId('backup-progress-log-copy'));
    await waitFor(() => expect(writeText).toHaveBeenCalled());
    const copied = String(writeText.mock.calls[0]?.[0] ?? '');
    expect(copied).toContain('backup.restoring');
    expect(copied).toContain('backup.restoreSuccess');
  });

  it('restore asks to overwrite when target already has objects', async () => {
    mockRestoreCommands([{ name: 'users', tableType: 'table' }]);
    confirmDialogFn.mockResolvedValue(true);
    await selectRestoreTarget();
    fireEvent.click(screen.getByTestId('backup-start-restore'));
    await waitFor(() => expect(confirmDialogFn).toHaveBeenCalled());
    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith('restore_database_with_dialog', {
        connectionId: 'live-1',
        database: 'app',
        options: ['overwrite'],
      }),
    );
  });

  it('restore does not start when overwrite is declined', async () => {
    mockRestoreCommands([{ name: 'users', tableType: 'table' }]);
    confirmDialogFn.mockResolvedValue(false);
    await selectRestoreTarget();
    fireEvent.click(screen.getByTestId('backup-start-restore'));
    await waitFor(() => expect(confirmDialogFn).toHaveBeenCalled());
    expect(invokeMock).not.toHaveBeenCalledWith('restore_database_with_dialog', expect.anything());
  });
});
