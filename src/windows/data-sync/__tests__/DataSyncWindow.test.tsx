import { describe, expect, it, vi, beforeEach, afterEach } from 'vitest';
import { render, cleanup, fireEvent, screen, waitFor, within } from '@testing-library/react';
import type { ConnectionConfig } from '../../../types';
import type { DataSyncRowChange } from '../../../commands/sync';

const {
  invokeMock,
  inspectDataSyncMock,
  compareDataSyncMock,
  applyDataSyncMock,
  executeDataSyncMock,
  generateDataSyncSqlMock,
  cancelDataSyncMock,
  getDatabasesMock,
  getTablesMock,
  aiChatMock,
  stableT,
  aiConfiguredRef,
} = vi.hoisted(() => {
  const stableT = (key: string, params?: Record<string, string | number>) =>
    params ? `${key}:${JSON.stringify(params)}` : key;
  const aiConfiguredRef = { value: false };
  return {
    invokeMock: vi.fn(),
    inspectDataSyncMock: vi.fn(),
    compareDataSyncMock: vi.fn(),
    applyDataSyncMock: vi.fn(),
    executeDataSyncMock: vi.fn(),
    generateDataSyncSqlMock: vi.fn(),
    cancelDataSyncMock: vi.fn().mockResolvedValue(true),
    getDatabasesMock: vi.fn(),
    getTablesMock: vi.fn(),
    aiChatMock: vi.fn(),
    stableT,
    aiConfiguredRef,
  };
});

vi.mock('@tauri-apps/api/core', () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));

vi.mock('@tauri-apps/api/event', () => ({
  listen: vi.fn().mockResolvedValue(() => {}),
}));

vi.mock('../../../hooks/useThemeListener', () => ({
  useThemeListener: vi.fn(),
}));

vi.mock('../../../hooks/useI18n', () => ({
  useI18n: () => ({ t: stableT, language: 'en' }),
}));

vi.mock('../../../stores/settingsStore', () => ({
  useSettingsStore: (sel: (s: { loadSettings: () => Promise<void> }) => unknown) =>
    sel({ loadSettings: vi.fn().mockResolvedValue(undefined) }),
}));

vi.mock('../../../commands/sync', () => ({
  syncCommands: {
    classifyDataSyncPair: (sourceDatabaseType: string, targetDatabaseType: string) =>
      invokeMock('classify_data_sync_pair', { sourceDatabaseType, targetDatabaseType }),
    inspectDataSync: (...args: unknown[]) => inspectDataSyncMock(...args),
    compareDataSync: (...args: unknown[]) => compareDataSyncMock(...args),
    applyDataSync: (...args: unknown[]) => applyDataSyncMock(...args),
    executeDataSync: (...args: unknown[]) => executeDataSyncMock(...args),
    generateDataSyncSql: (...args: unknown[]) => generateDataSyncSqlMock(...args),
    cancelDataSync: (...args: unknown[]) => cancelDataSyncMock(...args),
  },
  DEFAULT_SYNC_OPTIONS: { insert: true, update: true, delete: false },
}));

vi.mock('../../../commands/database', () => ({
  databaseCommands: {
    getDatabases: (...args: unknown[]) => getDatabasesMock(...args),
    getTables: (...args: unknown[]) => getTablesMock(...args),
  },
}));

vi.mock('../../../commands/ai', () => ({
  aiCommands: {
    chat: (...args: unknown[]) => aiChatMock(...args),
  },
}));

vi.mock('../../../stores/aiStore', () => ({
  useAiStore: (sel: (s: { isConfigured: boolean; loadConfig: () => Promise<void> }) => unknown) =>
    sel({
      isConfigured: aiConfiguredRef.value,
      loadConfig: vi.fn().mockResolvedValue(undefined),
    }),
}));

vi.mock('../../../hooks/useSettings', () => ({
  useSettings: vi.fn(),
}));

vi.mock('../../../lib/windowManager', () => ({
  openSchemaDiffWindow: vi.fn(),
  openDataTransferWindow: vi.fn(),
}));

vi.mock('../../../components/TitleBar', () => ({
  TitleBar: ({ title }: { title?: unknown }) => (
    <div data-testid="title-bar">{String(title ?? '')}</div>
  ),
}));

vi.mock('../../../components/StatusBar', () => ({
  StatusBar: ({ left }: { left?: unknown }) => <div data-testid="status-bar">{left as never}</div>,
}));

import { DataSyncWindow } from '../DataSyncWindow';

const pgSrc: ConnectionConfig = {
  id: 'pg-src',
  name: 'PG Src',
  databaseType: 'postgresql',
  host: '127.0.0.1',
  port: 5432,
  database: 'src',
  username: 'postgres',
  password: '',
  sslMode: 'disable',
};

const pgSrcAlt: ConnectionConfig = {
  ...pgSrc,
  id: 'pg-src-alt',
  name: 'PG Src Alt',
};

const pgTgt: ConnectionConfig = {
  id: 'pg-tgt',
  name: 'PG Tgt',
  databaseType: 'postgresql',
  host: '127.0.0.1',
  port: 5432,
  database: 'tgt',
  username: 'postgres',
  password: '',
  sslMode: 'disable',
};

const pgTgtReadOnly: ConnectionConfig = {
  ...pgTgt,
  id: 'pg-tgt-ro',
  name: 'PG Tgt RO',
  readOnly: true,
};

const mysqlTgt: ConnectionConfig = {
  id: 'my-tgt',
  name: 'My Tgt',
  databaseType: 'mysql',
  host: '127.0.0.1',
  port: 3306,
  database: 'tgt',
  username: 'root',
  password: '',
  sslMode: 'disable',
};

function insertRow(): DataSyncRowChange {
  return {
    operation: 'INSERT',
    key: [1],
    sourceRow: [[1, 'alice']],
    targetRow: null,
    changedColumns: [],
    selected: true,
  };
}

function mockClassifyDataSyncPair(args?: {
  sourceDatabaseType?: string;
  targetDatabaseType?: string;
}) {
  const src = args?.sourceDatabaseType ?? '';
  const tgt = args?.targetDatabaseType ?? '';
  if (src === tgt && src === 'postgresql') {
    return { path: 'direct', supported: true, family: 'postgresql' };
  }
  if ((src === 'mysql' && tgt === 'mariadb') || (src === 'mariadb' && tgt === 'mysql')) {
    return { path: 'direct', supported: true, family: 'mysql' };
  }
  if (src === 'postgresql' && tgt === 'mysql') {
    return {
      path: 'ir',
      supported: false,
      reason: 'heterogeneous pair postgresql → mysql is Data Transfer, not Data Synchronization',
    };
  }
  return { path: 'unsupported', supported: false };
}

async function pickSelect(testId: string, optionLabel: string) {
  const wrap = screen.getByTestId(testId);
  const trigger = within(wrap).getAllByRole('button')[0];
  fireEvent.click(trigger);
  const list = await waitFor(() => {
    const el = document.getElementById('dz-select-listbox');
    if (!el) throw new Error('dz-select-listbox not open');
    return el;
  });
  const option = Array.from(list.children).find((el) =>
    (el.textContent || '').includes(optionLabel),
  );
  expect(option, `option ${optionLabel}`).toBeTruthy();
  fireEvent.mouseDown(option!);
}

describe('DataSyncWindow (Diff Workspace)', () => {
  beforeEach(() => {
    aiConfiguredRef.value = false;
    invokeMock.mockReset();
    inspectDataSyncMock.mockReset();
    compareDataSyncMock.mockReset();
    applyDataSyncMock.mockReset();
    executeDataSyncMock.mockReset();
    generateDataSyncSqlMock.mockReset();
    generateDataSyncSqlMock.mockRejectedValue(new Error('not registered'));
    aiChatMock.mockReset();
    aiChatMock.mockResolvedValue('AI summary of diffs');
    getDatabasesMock.mockReset();
    getDatabasesMock.mockImplementation(async (connId: string) =>
      connId.includes('pg-src') || connId.includes('my') ? ['src', 'other'] : ['tgt'],
    );
    getTablesMock.mockReset();
    getTablesMock.mockResolvedValue([{ name: 'users', tableType: 'table' }]);
    invokeMock.mockImplementation(
      async (cmd: string, args?: { connectionId?: string; database?: string | null; sourceDatabaseType?: string; targetDatabaseType?: string }) => {
        if (cmd === 'get_connections') return [pgSrc, pgTgt, mysqlTgt];
        if (cmd === 'connect_dedicated') {
          const conn = args?.connectionId ?? 'unknown';
          const db = args?.database ?? 'default';
          return `dedicated-${conn}-${db}`;
        }
        if (cmd === 'release_connection') return false;
        if (cmd === 'connect') return `live-${args?.connectionId}`;
        if (cmd === 'classify_data_sync_pair') return mockClassifyDataSyncPair(args);
        return null;
      },
    );
  });

  afterEach(() => {
    cleanup();
  });

  it('shows idle prompt and primary controls', async () => {
    render(<DataSyncWindow />);
    await waitFor(() => expect(invokeMock).toHaveBeenCalledWith('get_connections'));
    expect(screen.queryByTestId('data-sync-overwrite-retired')).toBeNull();
    expect(screen.getByText('sync.selectPrompt')).toBeTruthy();
    expect(screen.getByTestId('data-sync-compare')).toBeTruthy();
    expect(screen.getByTestId('data-sync-swap')).toBeTruthy();
    expect(screen.getByTestId('data-sync-option-insert')).toBeTruthy();
    expect(screen.queryByTestId('data-sync-start-disabled')).toBeNull();
  });

  it('prompts to select both endpoints when Compare is clicked empty', async () => {
    render(<DataSyncWindow />);
    await waitFor(() => expect(invokeMock).toHaveBeenCalledWith('get_connections'));
    fireEvent.click(screen.getByTestId('data-sync-compare'));
    expect(await screen.findByTestId('data-sync-error')).toHaveTextContent('sync.selectBoth');
    expect(inspectDataSyncMock).not.toHaveBeenCalled();
    expect(compareDataSyncMock).not.toHaveBeenCalled();
  });

  it('inspects then compares and enables Execute when row diffs exist', async () => {
    inspectDataSyncMock.mockResolvedValue([
      { sourceTable: 'users', targetTable: 'users', status: 'MATCHED' },
      { sourceTable: 'orders', targetTable: '', status: 'UNMAPPED_SOURCE' },
    ]);
    compareDataSyncMock.mockResolvedValue([
      {
        sourceTable: 'users',
        targetTable: 'users',
        status: 'MATCHED',
        rows: [insertRow()],
      },
    ]);
    applyDataSyncMock.mockResolvedValue({ applied: 1, rolledBack: false });
    render(<DataSyncWindow />);
    await waitFor(() => expect(invokeMock).toHaveBeenCalledWith('get_connections'));
    await pickSelect('data-sync-source', 'PG Src');
    await pickSelect('data-sync-target', 'PG Tgt');
    await waitFor(() =>
      expect(screen.getByTestId('data-sync-source-database')).toHaveTextContent('src'),
    );
    fireEvent.click(screen.getByTestId('data-sync-compare'));
    await waitFor(() =>
      expect(inspectDataSyncMock).toHaveBeenCalledWith(
        'dedicated-pg-src-src',
        'dedicated-pg-tgt-tgt',
        'src',
        'tgt',
        undefined,
        undefined,
      ),
    );
    await waitFor(() =>
      expect(compareDataSyncMock).toHaveBeenCalledWith(
        'dedicated-pg-src-src',
        'dedicated-pg-tgt-tgt',
        ['users'],
        expect.any(String),
        'src',
        'tgt',
        undefined,
        undefined,
        expect.objectContaining({ insert: true, update: true, delete: false }),
      ),
    );
    const rows = await screen.findAllByTestId('data-sync-mapping-row');
    expect(rows).toHaveLength(2);
    expect(screen.getByTestId('data-sync-summary')).toBeTruthy();
    expect(screen.getByTestId('data-sync-row-diff')).toBeTruthy();
    expect(screen.getAllByText(/sync.rowDiffs/).length).toBeGreaterThan(0);
    expect(screen.getByText('sync.mappingUnmappedSource')).toBeTruthy();
    const execute = screen.getByTestId('data-sync-start');
    expect(execute).not.toBeDisabled();
    fireEvent.click(execute);
    await waitFor(() =>
      expect(applyDataSyncMock).toHaveBeenCalledWith(
        'dedicated-pg-src-src',
        'dedicated-pg-tgt-tgt',
        ['users'],
        expect.any(String),
        'src',
        'tgt',
        undefined,
        undefined,
        expect.objectContaining({ insert: true }),
      ),
    );
    await waitFor(() =>
      expect(compareDataSyncMock).toHaveBeenLastCalledWith(
        'dedicated-pg-src-src',
        'dedicated-pg-tgt-tgt',
        ['users'],
        expect.any(String),
        'src',
        'tgt',
        undefined,
        undefined,
        expect.any(Object),
      ),
    );
  });

  it('shows schema pickers for PostgreSQL when get_tables returns schemas', async () => {
    getTablesMock.mockResolvedValue([
      { name: 'users', schema: 'public', tableType: 'table' },
      { name: 'users', schema: 'app', tableType: 'table' },
    ]);
    render(<DataSyncWindow />);
    await waitFor(() => expect(invokeMock).toHaveBeenCalledWith('get_connections'));
    await pickSelect('data-sync-source', 'PG Src');
    await pickSelect('data-sync-target', 'PG Tgt');
    await waitFor(() => expect(screen.getByTestId('data-sync-source-schema')).toBeTruthy());
    expect(screen.getByTestId('data-sync-target-schema')).toBeTruthy();
    expect(screen.getByTestId('data-sync-source-schema')).toHaveTextContent('public');
  });

  it('marks heterogeneous targets as unsupported in the picker', async () => {
    render(<DataSyncWindow />);
    await waitFor(() => expect(invokeMock).toHaveBeenCalledWith('get_connections'));
    await pickSelect('data-sync-source', 'PG Src');
    const wrap = screen.getByTestId('data-sync-target');
    fireEvent.click(within(wrap).getAllByRole('button')[0]);
    const list = await waitFor(() => {
      const el = document.getElementById('dz-select-listbox');
      if (!el) throw new Error('dz-select-listbox not open');
      return el;
    });
    const mysql = Array.from(list.children).find((el) => (el.textContent || '').includes('My Tgt'));
    expect(mysql?.textContent).toContain('common.unsupportedPair');
  });

  it('surfaces error when database list cannot be loaded', async () => {
    invokeMock.mockImplementation(async (cmd: string, args?: { connectionId?: string; sourceDatabaseType?: string; targetDatabaseType?: string }) => {
      if (cmd === 'get_connections') return [pgSrc, pgTgt, mysqlTgt];
      if (cmd === 'connect_dedicated') {
        if (args?.connectionId === 'pg-tgt') throw new Error('refused');
        return `dedicated-${args?.connectionId}-db`;
      }
      if (cmd === 'release_connection') return false;
      if (cmd === 'connect') return `live-${args?.connectionId}`;
      if (cmd === 'classify_data_sync_pair') return mockClassifyDataSyncPair(args);
      return null;
    });
    render(<DataSyncWindow />);
    await waitFor(() => expect(invokeMock).toHaveBeenCalledWith('get_connections'));
    await pickSelect('data-sync-source', 'PG Src');
    await pickSelect('data-sync-target', 'PG Tgt');
    expect(await screen.findByTestId('data-sync-error')).toHaveTextContent(
      'sync.loadDatabasesFailedrefused',
    );
    expect(inspectDataSyncMock).not.toHaveBeenCalled();
  });

  it('surfaces data-sync comparison errors', async () => {
    render(<DataSyncWindow />);
    await waitFor(() => expect(invokeMock).toHaveBeenCalledWith('get_connections'));
    await pickSelect('data-sync-source', 'PG Src');
    await pickSelect('data-sync-target', 'PG Tgt');
    await waitFor(() =>
      expect(screen.getByTestId('data-sync-source-database')).toHaveTextContent('src'),
    );
    inspectDataSyncMock.mockRejectedValue(new Error('gate failed'));
    fireEvent.click(screen.getByTestId('data-sync-compare'));
    expect(await screen.findByTestId('data-sync-error')).toHaveTextContent('gate failed');
  });

  it('toggles mapping include checkboxes and shows incompatible reason', async () => {
    inspectDataSyncMock.mockResolvedValue([
      { sourceTable: 'users', targetTable: 'users', status: 'MATCHED' },
      {
        sourceTable: 'legacy',
        targetTable: 'legacy',
        status: 'INCOMPATIBLE',
        incompatibleReason: 'pk mismatch',
      },
    ]);
    compareDataSyncMock.mockResolvedValue([
      { sourceTable: 'users', targetTable: 'users', status: 'MATCHED', rows: [] },
    ]);
    render(<DataSyncWindow />);
    await waitFor(() => expect(invokeMock).toHaveBeenCalledWith('get_connections'));
    await pickSelect('data-sync-source', 'PG Src');
    await pickSelect('data-sync-target', 'PG Tgt');
    await waitFor(() =>
      expect(screen.getByTestId('data-sync-source-database')).toHaveTextContent('src'),
    );
    fireEvent.click(screen.getByTestId('data-sync-compare'));
    await screen.findAllByTestId('data-sync-mapping-row');
    expect(screen.getByTestId('data-sync-path')).toHaveTextContent('sync.pathDirect');
    expect(screen.getByText('pk mismatch')).toBeTruthy();

    const matchedRow = screen.getAllByTestId('data-sync-mapping-row')[0];
    fireEvent.click(within(matchedRow).getByRole('checkbox'));
    expect(within(matchedRow).getByRole('checkbox')).not.toBeChecked();
  });

  it('cancel during compare resets sync state and re-enables Compare', async () => {
    let resolveInspect: ((value: unknown) => void) | undefined;
    inspectDataSyncMock.mockImplementation(
      () =>
        new Promise((resolve) => {
          resolveInspect = resolve;
        }),
    );
    render(<DataSyncWindow />);
    await waitFor(() => expect(invokeMock).toHaveBeenCalledWith('get_connections'));
    await pickSelect('data-sync-source', 'PG Src');
    await pickSelect('data-sync-target', 'PG Tgt');
    await waitFor(() =>
      expect(screen.getByTestId('data-sync-source-database')).toHaveTextContent('src'),
    );
    fireEvent.click(screen.getByTestId('data-sync-compare'));
    await waitFor(() => expect(screen.getByTestId('data-sync-cancel')).toBeTruthy());
    expect(screen.getByTestId('data-sync-window')).toHaveAttribute('data-sync-state', 'inspecting');
    fireEvent.click(screen.getByTestId('data-sync-cancel'));
    await waitFor(() =>
      expect(screen.getByTestId('data-sync-window')).toHaveAttribute('data-sync-state', 'idle'),
    );
    expect(cancelDataSyncMock).toHaveBeenCalled();
    expect(screen.getByTestId('data-sync-compare')).not.toBeDisabled();
    resolveInspect?.([{ sourceTable: 'users', targetTable: 'users', status: 'MATCHED' }]);
  });

  it('disables Execute and shows targetReadOnly for read-only target', async () => {
    invokeMock.mockImplementation(
      async (cmd: string, args?: { connectionId?: string; database?: string | null; sourceDatabaseType?: string; targetDatabaseType?: string }) => {
        if (cmd === 'get_connections') return [pgSrc, pgTgtReadOnly, mysqlTgt];
        if (cmd === 'connect_dedicated') {
          const conn = args?.connectionId ?? 'unknown';
          const db = args?.database ?? 'default';
          return `dedicated-${conn}-${db}`;
        }
        if (cmd === 'release_connection') return false;
        if (cmd === 'connect') return `live-${args?.connectionId}`;
        if (cmd === 'classify_data_sync_pair') return mockClassifyDataSyncPair(args);
        return null;
      },
    );
    inspectDataSyncMock.mockResolvedValue([
      { sourceTable: 'users', targetTable: 'users', status: 'MATCHED' },
    ]);
    compareDataSyncMock.mockResolvedValue([
      {
        sourceTable: 'users',
        targetTable: 'users',
        status: 'MATCHED',
        rows: [insertRow()],
      },
    ]);
    render(<DataSyncWindow />);
    await waitFor(() => expect(invokeMock).toHaveBeenCalledWith('get_connections'));
    await pickSelect('data-sync-source', 'PG Src');
    await pickSelect('data-sync-target', 'PG Tgt RO');
    await waitFor(() =>
      expect(screen.getByTestId('data-sync-source-database')).toHaveTextContent('src'),
    );
    fireEvent.click(screen.getByTestId('data-sync-compare'));
    await screen.findByTestId('data-sync-summary');
    expect(screen.getByTestId('data-sync-start-disabled')).toBeTruthy();
    expect(screen.getByText('sync.targetReadOnly')).toBeTruthy();
    expect(applyDataSyncMock).not.toHaveBeenCalled();
  });

  it('clears mapping when source connection changes after compare', async () => {
    invokeMock.mockImplementation(
      async (cmd: string, args?: { connectionId?: string; database?: string | null; sourceDatabaseType?: string; targetDatabaseType?: string }) => {
        if (cmd === 'get_connections') return [pgSrc, pgSrcAlt, pgTgt, mysqlTgt];
        if (cmd === 'connect_dedicated') {
          const conn = args?.connectionId ?? 'unknown';
          const db = args?.database ?? 'default';
          return `dedicated-${conn}-${db}`;
        }
        if (cmd === 'release_connection') return false;
        if (cmd === 'connect') return `live-${args?.connectionId}`;
        if (cmd === 'classify_data_sync_pair') return mockClassifyDataSyncPair(args);
        return null;
      },
    );
    inspectDataSyncMock.mockResolvedValue([
      { sourceTable: 'users', targetTable: 'users', status: 'MATCHED' },
    ]);
    compareDataSyncMock.mockResolvedValue([
      { sourceTable: 'users', targetTable: 'users', status: 'MATCHED', rows: [insertRow()] },
    ]);
    render(<DataSyncWindow />);
    await waitFor(() => expect(invokeMock).toHaveBeenCalledWith('get_connections'));
    await pickSelect('data-sync-source', 'PG Src');
    await pickSelect('data-sync-target', 'PG Tgt');
    await waitFor(() =>
      expect(screen.getByTestId('data-sync-source-database')).toHaveTextContent('src'),
    );
    fireEvent.click(screen.getByTestId('data-sync-compare'));
    await screen.findAllByTestId('data-sync-mapping-row');
    expect(screen.getByTestId('data-sync-window')).toHaveAttribute('data-sync-state', 'compared');
    await pickSelect('data-sync-source', 'PG Src Alt');
    await waitFor(() =>
      expect(screen.getByTestId('data-sync-window')).toHaveAttribute('data-sync-state', 'idle'),
    );
    expect(screen.queryAllByTestId('data-sync-mapping-row')).toHaveLength(0);
    expect(screen.getByText('sync.selectPrompt')).toBeTruthy();
  });

  it('shows AI explain diff result when configured', async () => {
    aiConfiguredRef.value = true;
    inspectDataSyncMock.mockResolvedValue([
      { sourceTable: 'users', targetTable: 'users', status: 'MATCHED' },
    ]);
    compareDataSyncMock.mockResolvedValue([
      {
        sourceTable: 'users',
        targetTable: 'users',
        status: 'MATCHED',
        rows: [insertRow()],
      },
    ]);
    render(<DataSyncWindow />);
    await waitFor(() => expect(invokeMock).toHaveBeenCalledWith('get_connections'));
    await pickSelect('data-sync-source', 'PG Src');
    await pickSelect('data-sync-target', 'PG Tgt');
    await waitFor(() =>
      expect(screen.getByTestId('data-sync-source-database')).toHaveTextContent('src'),
    );
    fireEvent.click(screen.getByTestId('data-sync-compare'));
    await screen.findByTestId('data-sync-summary');
    fireEvent.click(screen.getByTestId('data-sync-explain-diff'));
    await waitFor(() => expect(aiChatMock).toHaveBeenCalled());
    expect(await screen.findByTestId('data-sync-explain-result')).toHaveTextContent(
      'AI summary of diffs',
    );
  });
});
