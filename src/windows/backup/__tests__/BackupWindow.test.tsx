import { describe, expect, it, vi, beforeEach, afterEach } from 'vitest';
import { render, cleanup, waitFor, fireEvent, screen } from '@testing-library/react';
import { BackupWindow } from '../BackupWindow';
import { PRESET_GROUPS } from '../../../lib/connectionGroups';
import type { ConnectionConfig } from '../../../types';

const {
  invokeMock,
  loadSettingsMock,
  urlParamMock,
  confirmDialogFn,
  listenMock,
  backupOptionsRef,
} = vi.hoisted(() => ({
  invokeMock: vi.fn(),
  loadSettingsMock: vi.fn().mockResolvedValue(undefined),
  urlParamMock: vi.fn((name: string) => (name === 'mode' ? null : null)),
  confirmDialogFn: vi.fn().mockResolvedValue(true),
  listenMock: vi.fn(async () => () => {}),
  // Mutable per-test dialect backup options; defaults to the previous empty list.
  backupOptionsRef: { current: [] as Array<{ id: string; label: string }> },
}));

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
  getSqlDialect: () => ({ backupOptions: backupOptionsRef.current }),
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
  backupOptionsRef.current = [];
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
      if (cmd === 'get_tables') return tables;
      if (cmd === 'restore_sql_file') return true;
      return null;
    });
  }

  async function selectRestoreTarget() {
    render(<BackupWindow />);
    await waitFor(() => expect(screen.getByText('common.restoreDatabase')).toBeInTheDocument());
    fireEvent.click(screen.getByText('Local PG'));
    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith('connect', { connectionId: 'pg-1' }),
    );
    await waitFor(() => screen.getByText('app'));
    fireEvent.click(screen.getByText('app'));
    await waitFor(() => expect(screen.getByTestId('backup-start-restore')).not.toBeDisabled());
  }

  it('restore mode shows restore action and waits for connection + database', async () => {
    mockRestoreCommands([]);
    await selectRestoreTarget();
    expect(screen.queryByText('backup.startBackup')).not.toBeInTheDocument();
    fireEvent.click(screen.getByTestId('backup-start-restore'));
    expect(confirmDialogFn).not.toHaveBeenCalled();
    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith('restore_sql_file', {
        dbSessionId: 'live-1',
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
      if (cmd === 'get_tables') return [];
      if (cmd === 'restore_sql_file') {
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
      expect(invokeMock).toHaveBeenCalledWith('restore_sql_file', {
        dbSessionId: 'live-1',
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
    expect(invokeMock).not.toHaveBeenCalledWith('restore_sql_file', expect.anything());
  });
});

describe('BackupWindow backup flow (F3-BUG-002 coverage)', () => {
  // An earlier suite case leaves `mockResolvedValue(false)` behind; restore the
  // default accept so the overwrite confirmation behaves per-test.
  beforeEach(() => {
    confirmDialogFn.mockResolvedValue(true);
  });

  function pgOnlyConnections() {
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

  function setupInvoke(backupDatabaseImpl: () => Promise<boolean>) {
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === 'get_connections') return pgOnlyConnections();
      if (cmd === 'get_groups') return [PRESET_GROUPS.development];
      if (cmd === 'connect') return 'live-1';
      if (cmd === 'get_connection_info') return { serverVersion: '16' };
      if (cmd === 'get_databases') return ['app', 'postgres'];
      if (cmd === 'backup_database') return backupDatabaseImpl();
      return null;
    });
  }

  async function selectBackupTarget() {
    render(<BackupWindow />);
    await waitFor(() => screen.getByText('Local PG'));
    fireEvent.click(screen.getByText('Local PG'));
    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith('connect', { connectionId: 'pg-1' }),
    );
    await waitFor(() => screen.getByText('app'));
    fireEvent.click(screen.getByText('app'));
    await waitFor(() => expect(screen.getByTestId('backup-start-backup')).not.toBeDisabled());
  }

  function backupDatabaseCall(): { command: string; args: Record<string, unknown> } {
    const call = invokeMock.mock.calls.find((c) => c[0] === 'backup_database');
    expect(call).toBeTruthy();
    return { command: String(call![0]), args: call![1] as Record<string, unknown> };
  }

  it('backup success: invokes backup_database without overridePath and reports success', async () => {
    setupInvoke(async () => true);
    await selectBackupTarget();

    fireEvent.click(screen.getByTestId('backup-start-backup'));

    const { command, args } = await waitFor(() => {
      const found = backupDatabaseCall();
      expect(found.args.database).toBe('app');
      return found;
    });
    expect(command).toBe('backup_database');
    expect(args).toEqual({
      dbSessionId: 'live-1',
      database: 'app',
      defaultFileName: 'untitled.sql',
      filterExtension: 'sql',
      options: [],
      compress: false,
    });
    // Decision 3+6 guard: the production dialog flow must never carry override_path.
    expect(Object.keys(args)).not.toContain('overridePath');

    expect(listenMock).toHaveBeenCalledWith('backup-progress', expect.any(Function));
    await waitFor(() =>
      expect(screen.getByTestId('backup-status')).toHaveTextContent('backup.success'),
    );
    expect(screen.getByTestId('backup-progress-log')).toHaveTextContent('backup.success');
  });

  it('backup cancelled (saved=false): clears status and re-enables start without success log', async () => {
    setupInvoke(async () => false);
    await selectBackupTarget();

    fireEvent.click(screen.getByTestId('backup-start-backup'));

    await waitFor(() => expect(backupDatabaseCall()).toBeTruthy());
    await waitFor(() => expect(screen.queryByTestId('backup-status')).not.toBeInTheDocument());
    expect(screen.queryByTestId('backup-progress-log')).not.toBeInTheDocument();
    expect(screen.getByTestId('backup-start-backup')).toHaveTextContent('backup.startBackup');
  });

  it('backup backend error: surfaces message in status and progress log', async () => {
    setupInvoke(async () => {
      throw new Error('pg_dump exited with code 1');
    });
    await selectBackupTarget();

    fireEvent.click(screen.getByTestId('backup-start-backup'));

    await waitFor(() =>
      expect(screen.getByTestId('backup-status')).toHaveTextContent('pg_dump exited with code 1'),
    );
    expect(screen.getByTestId('backup-progress-log')).toHaveTextContent(
      'pg_dump exited with code 1',
    );
    expect(screen.queryByTestId('backup-start-backup')).toHaveTextContent('backup.startBackup');
  });

  it('option dropdown, custom format, gzip and file name drive filterExtension/defaultFileName', async () => {
    backupOptionsRef.current = [
      { id: 'format-custom', label: 'custom-format' },
      { id: 'routines', label: 'routines' },
    ];
    setupInvoke(async () => true);
    await selectBackupTarget();

    // routines is auto-enabled as a default for SQL dialects.
    fireEvent.click(screen.getByText('backup.addOption')); // open dropdown
    const checkboxes = screen.getAllByRole('checkbox');
    fireEvent.click(checkboxes[0]); // format-custom
    fireEvent.click(checkboxes[checkboxes.length - 1]); // compress gzip

    fireEvent.change(screen.getByDisplayValue('untitled'), {
      target: { value: 'nightly' },
    });

    fireEvent.click(screen.getByTestId('backup-start-backup'));

    const { args } = await waitFor(() => {
      const found = backupDatabaseCall();
      expect(found.args.compress).toBe(true);
      return found;
    });
    expect(args).toEqual({
      dbSessionId: 'live-1',
      database: 'app',
      defaultFileName: 'nightly.sql.gz',
      filterExtension: 'gz',
      options: ['routines', 'format-custom'],
      compress: true,
    });
    expect(Object.keys(args)).not.toContain('overridePath');
  });

  it('prefills connection + database from URL params', async () => {
    urlParamMock.mockImplementation((name: string) => {
      if (name === 'connectionId') return 'pg-1';
      if (name === 'database') return 'postgres';
      return null;
    });
    setupInvoke(async () => true);

    render(<BackupWindow />);
    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith('connect', { connectionId: 'pg-1' }),
    );
    await waitFor(() => expect(screen.getByTestId('backup-start-backup')).not.toBeDisabled());
    expect(screen.getByText('postgres').parentElement?.className).toContain('bg-blue-600/20');

    fireEvent.click(screen.getByTestId('backup-start-backup'));
    const { args } = await waitFor(() => {
      const found = backupDatabaseCall();
      expect(found.args.database).toBe('postgres');
      return found;
    });
    expect(args.dbSessionId).toBe('live-1');
  });

  it('restore backend error surfaces in status and progress log', async () => {
    urlParamMock.mockImplementation((name: string) => (name === 'mode' ? 'restore' : null));
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === 'get_connections') return pgOnlyConnections();
      if (cmd === 'get_groups') return [PRESET_GROUPS.development];
      if (cmd === 'connect') return 'live-1';
      if (cmd === 'get_connection_info') return { serverVersion: '16' };
      if (cmd === 'get_databases') return ['app', 'postgres'];
      if (cmd === 'get_tables') return [{ name: 'users', tableType: 'table' }];
      if (cmd === 'restore_sql_file') throw new Error('restore boom');
      return null;
    });

    render(<BackupWindow />);
    await waitFor(() => screen.getByText('Local PG'));
    fireEvent.click(screen.getByText('Local PG'));
    await waitFor(() => expect(screen.getByTestId('backup-start-restore')).not.toBeDisabled());

    fireEvent.click(screen.getByTestId('backup-start-restore'));

    await waitFor(() => expect(confirmDialogFn).toHaveBeenCalled());
    await waitFor(() =>
      expect(screen.getByTestId('backup-status')).toHaveTextContent('restore boom'),
    );
    expect(screen.getByTestId('backup-progress-log')).toHaveTextContent('restore boom');
  });

  it('collapses and expands a group on header click', async () => {
    setupInvoke(async () => true);
    render(<BackupWindow />);
    await waitFor(() => screen.getByText('Local PG'));

    fireEvent.click(screen.getByTestId('backup-group-header'));
    expect(screen.queryByText('Local PG')).not.toBeInTheDocument();

    fireEvent.click(screen.getByTestId('backup-group-header'));
    await waitFor(() => screen.getByText('Local PG'));
  });
});
