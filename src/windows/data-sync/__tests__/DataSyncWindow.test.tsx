import { describe, expect, it, vi, beforeEach, afterEach } from 'vitest';
import { act, render, cleanup, fireEvent, screen, waitFor, within } from '@testing-library/react';
import type { ConnectionConfig } from '../../../types';
import type { DataSyncRowChange } from '../../../commands/sync';

const {
  invokeMock,
  inspectDataSyncMock,
  compareDataSyncMock,
  getDataSyncComparisonPageMock,
  applyDataSyncMock,
  executeDataSyncMock,
  generateDataSyncSqlMock,
  cancelDataSyncMock,
  getSyncProfilesMock,
  saveSyncProfileMock,
  deleteSyncProfileMock,
  getDatabasesMock,
  listTablesMock,
  aiChatMock,
  stableT,
  aiConfiguredRef,
  urlParamMock,
} = vi.hoisted(() => {
  const stableT = (key: string, params?: Record<string, string | number>) =>
    params ? `${key}:${JSON.stringify(params)}` : key;
  const aiConfiguredRef = { value: false };
  return {
    invokeMock: vi.fn(),
    inspectDataSyncMock: vi.fn(),
    compareDataSyncMock: vi.fn(),
    getDataSyncComparisonPageMock: vi.fn(),
    applyDataSyncMock: vi.fn(),
    executeDataSyncMock: vi.fn(),
    generateDataSyncSqlMock: vi.fn(),
    cancelDataSyncMock: vi.fn().mockResolvedValue(true),
    getSyncProfilesMock: vi.fn(),
    saveSyncProfileMock: vi.fn(),
    deleteSyncProfileMock: vi.fn(),
    getDatabasesMock: vi.fn(),
    listTablesMock: vi.fn(),
    aiChatMock: vi.fn(),
    stableT,
    aiConfiguredRef,
    urlParamMock: vi.fn<(name: string) => string | null>(),
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

vi.mock('../../../hooks/useLocaleDomains', () => ({
  useLocaleDomains: () => true,
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
    getDataSyncComparisonPage: (...args: unknown[]) => getDataSyncComparisonPageMock(...args),
    applyDataSync: (...args: unknown[]) => applyDataSyncMock(...args),
    executeDataSync: (...args: unknown[]) => executeDataSyncMock(...args),
    generateDataSyncSql: (...args: unknown[]) => generateDataSyncSqlMock(...args),
    cancelDataSync: (...args: unknown[]) => cancelDataSyncMock(...args),
    getSyncProfiles: () => getSyncProfilesMock(),
    saveSyncProfile: (...args: unknown[]) => saveSyncProfileMock(...args),
    deleteSyncProfile: (...args: unknown[]) => deleteSyncProfileMock(...args),
  },
  DEFAULT_SYNC_OPTIONS: { insert: true, update: true, delete: false },
}));

vi.mock('../../../commands/database', () => ({
  databaseCommands: {
    getDatabases: (...args: unknown[]) => getDatabasesMock(...args),
    listTables: (...args: unknown[]) => listTablesMock(...args),
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

vi.mock('../../../lib/windowKind', () => ({
  getUrlParam: (name: string) => urlParamMock(name),
}));

vi.mock('../../../components/TitleBar', () => ({
  TitleBar: ({ title, rightContent }: { title?: unknown; rightContent?: unknown }) => (
    <div data-testid="title-bar">
      {String(title ?? '')}
      {rightContent as never}
    </div>
  ),
}));

vi.mock('../../../components/StatusBar', () => ({
  StatusBar: ({ left }: { left?: unknown }) => <div data-testid="status-bar">{left as never}</div>,
}));

import { DataSyncWindow } from '../DataSyncWindow';
import { clearSyncPairingCache } from '../../../lib/syncPairing';

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

const unknownHistoryRun = {
  id: 'run-unknown',
  operation: 'dataSync' as const,
  status: 'failed' as const,
  outcome: 'unknown',
  phase: 'finished',
  profileId: 'sync-profile-unknown',
  profileRevision: '2026-09-23T00:00:00.000Z',
  sourceConnectionId: 'pg-src',
  targetConnectionId: 'pg-tgt',
  startedAt: '2026-09-23T00:00:00.000Z',
  finishedAt: '2026-09-23T00:01:00.000Z',
  selectedCount: 1,
  committedCount: 0,
  failedCount: 0,
  conflictCount: 0,
  cancelled: false,
  rollbackOutcome: 'unknown',
  errorSummary: 'Commit or rollback could not be confirmed.',
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

function deleteRow(): DataSyncRowChange {
  return {
    operation: 'DELETE',
    key: [2],
    sourceRow: null,
    targetRow: [[2, 'bob']],
    changedColumns: [],
    selected: true,
  };
}

function historyRecoveryProfile(updatedAt = unknownHistoryRun.profileRevision) {
  return {
    version: 1,
    id: unknownHistoryRun.profileId,
    name: 'Recovery scope',
    sourceConnectionId: 'pg-src',
    targetConnectionId: 'pg-tgt',
    sourceDatabase: 'src',
    targetDatabase: 'tgt',
    sourceSchema: null,
    targetSchema: null,
    tables: [{ sourceTable: 'users', targetTable: 'users', enabled: true }],
    options: { insert: true, update: true, delete: false },
    createdAt: '2026-09-22T00:00:00.000Z',
    updatedAt,
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
    return screen.getByRole('listbox');
  });
  const option = Array.from(list.children).find((el) =>
    (el.textContent || '').includes(optionLabel),
  );
  expect(option, `option ${optionLabel}`).toBeTruthy();
  fireEvent.mouseDown(option!);
}

async function selectEndpoints(targetLabel = 'PG Tgt') {
  await waitFor(() => expect(invokeMock).toHaveBeenCalledWith('get_connections'));
  await pickSelect('data-sync-source', 'PG Src');
  await pickSelect('data-sync-target', targetLabel);
  await waitFor(() =>
    expect(screen.getByTestId('data-sync-source-database')).toHaveTextContent('src'),
  );
  await waitFor(() => expect(screen.getByTestId('data-sync-next')).not.toBeDisabled());
}

async function advanceToObjects(targetLabel = 'PG Tgt') {
  await selectEndpoints(targetLabel);
  fireEvent.click(screen.getByTestId('data-sync-next'));
  await waitFor(() => expect(screen.getByTestId('data-sync-step-setup')).toBeTruthy());
  fireEvent.click(screen.getByTestId('data-sync-next'));
  await waitFor(() => expect(screen.getByTestId('data-sync-objects-step')).toBeTruthy());
}

async function advanceToCompare(targetLabel = 'PG Tgt') {
  await advanceToObjects(targetLabel);
  await waitFor(() => expect(screen.getByTestId('data-sync-next')).not.toBeDisabled());
  fireEvent.click(screen.getByTestId('data-sync-next'));
  await waitFor(() => expect(screen.getByTestId('data-sync-summary')).toBeTruthy());
}

async function advanceToPreview(targetLabel = 'PG Tgt') {
  await advanceToCompare(targetLabel);
  fireEvent.click(screen.getByTestId('data-sync-next'));
  await waitFor(() => expect(screen.getByTestId('data-sync-preview')).toBeTruthy());
}

describe('DataSyncWindow wizard', () => {
  beforeEach(() => {
    // 端点配对是模块级缓存。不清会让用例继承更早用例的缓存，既掩盖尚未落定的
    // 异步 effect，也使本文件的通过与否依赖执行顺序。
    clearSyncPairingCache();
    aiConfiguredRef.value = false;
    urlParamMock.mockReset();
    urlParamMock.mockReturnValue(null);
    invokeMock.mockReset();
    inspectDataSyncMock.mockReset();
    compareDataSyncMock.mockReset();
    getDataSyncComparisonPageMock.mockReset();
    applyDataSyncMock.mockReset();
    executeDataSyncMock.mockReset();
    cancelDataSyncMock.mockReset();
    cancelDataSyncMock.mockResolvedValue(true);
    getSyncProfilesMock.mockReset();
    getSyncProfilesMock.mockResolvedValue([]);
    saveSyncProfileMock.mockReset();
    saveSyncProfileMock.mockResolvedValue(undefined);
    deleteSyncProfileMock.mockReset();
    deleteSyncProfileMock.mockResolvedValue(undefined);
    generateDataSyncSqlMock.mockReset();
    generateDataSyncSqlMock.mockImplementation(async (_source, _target, tables, options) =>
      tables.flatMap((table: { targetTable: string; rows?: DataSyncRowChange[] }) =>
        (table.rows ?? [])
          .filter((row) => row.selected && options[row.operation.toLowerCase()])
          .map((row) => ({
            table: table.targetTable,
            operation: row.operation,
            sql: 'PARAMETERIZED DML',
            previewSql: 'REVIEWED DML',
            parameters: row.sourceRow ?? [],
            rowKey: row.key,
          })),
      ),
    );
    executeDataSyncMock.mockResolvedValue({ applied: 1, rolledBack: false });
    aiChatMock.mockReset();
    aiChatMock.mockResolvedValue('AI summary of diffs');
    getDatabasesMock.mockReset();
    getDatabasesMock.mockImplementation(async (connId: string) =>
      connId.includes('pg-src') || connId.includes('my') ? ['src', 'other'] : ['tgt'],
    );
    listTablesMock.mockReset();
    listTablesMock.mockResolvedValue([{ name: 'users', tableType: 'table' }]);
    invokeMock.mockImplementation(
      async (
        cmd: string,
        args?: {
          connectionId?: string;
          database?: string | null;
          dbSessionId?: string;
          request?: { command?: string; dbSessionId?: string };
          sourceDatabaseType?: string;
          targetDatabaseType?: string;
        },
      ) => {
        if (cmd === 'get_connections') return [pgSrc, pgTgt, mysqlTgt];
        if (cmd === 'list_migration_runs') {
          return { items: [unknownHistoryRun], total: 1, offset: 0, limit: 25 };
        }
        if (cmd === 'connect_dedicated') {
          const conn = args?.connectionId ?? 'unknown';
          const db = args?.database ?? 'default';
          return `dedicated-${conn}-${db}`;
        }
        if (cmd === 'execute_driver_command' && args?.request?.command === 'list_databases') {
          return {
            data: {
              databases: args.request.dbSessionId?.includes('pg-src') ? ['src', 'other'] : ['tgt'],
            },
          };
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

  function chooseProfile(profileName: string) {
    fireEvent.click(screen.getByTestId('data-sync-profile-select'));
    fireEvent.mouseDown(screen.getByRole('option', { name: profileName }));
  }

  it('shows the migration-style wizard shell and endpoint controls', async () => {
    render(<DataSyncWindow />);
    await waitFor(() => expect(invokeMock).toHaveBeenCalledWith('get_connections'));
    expect(screen.getByTestId('data-sync-step-endpoints')).toBeTruthy();
    expect(screen.getByTestId('data-sync-step-setup')).toBeTruthy();
    expect(screen.getByTestId('data-sync-step-objects')).toBeTruthy();
    expect(screen.getByTestId('data-sync-step-compare')).toBeTruthy();
    expect(screen.getByTestId('data-sync-step-preview')).toBeTruthy();
    expect(screen.getByTestId('data-sync-step-result')).toBeTruthy();
    expect(screen.getByTestId('data-sync-next')).toBeTruthy();
    expect(screen.getByTestId('data-sync-source')).toBeTruthy();
    expect(screen.getByTestId('data-sync-target')).toBeTruthy();
    expect(screen.queryByTestId('data-sync-start-disabled')).toBeNull();
  });

  it('keeps Next disabled until both endpoints are selected', async () => {
    render(<DataSyncWindow />);
    await waitFor(() => expect(invokeMock).toHaveBeenCalledWith('get_connections'));
    expect(screen.getByTestId('data-sync-next')).toBeDisabled();
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
    await advanceToCompare();
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
    expect(screen.getByTestId('data-sync-summary')).toBeTruthy();
    expect(screen.getByTestId('data-sync-row-diff')).toBeTruthy();
    expect(screen.getAllByText(/sync.rowDiffs/).length).toBeGreaterThan(0);
    fireEvent.click(screen.getByTestId('data-sync-back'));
    await waitFor(() => expect(screen.getByTestId('data-sync-objects-step')).toBeTruthy());
    expect(await screen.findAllByTestId('data-sync-mapping-row')).toHaveLength(2);
    expect(screen.getByText('sync.mappingUnmappedSource')).toBeTruthy();
    fireEvent.click(screen.getByTestId('data-sync-next'));
    await waitFor(() => expect(screen.getByTestId('data-sync-summary')).toBeTruthy());
    fireEvent.click(screen.getByTestId('data-sync-next'));
    await waitFor(() => expect(screen.getByTestId('data-sync-preview')).toBeTruthy());
    const execute = screen.getByTestId('data-sync-start');
    expect(execute).not.toBeDisabled();
    fireEvent.click(execute);
    await waitFor(() =>
      expect(executeDataSyncMock).toHaveBeenCalledWith(
        'dedicated-pg-tgt-tgt',
        expect.arrayContaining([expect.objectContaining({ table: 'users', rowKey: [1] })]),
        expect.any(String),
        'tgt',
      ),
    );
    expect(applyDataSyncMock).not.toHaveBeenCalled();
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

  it('[tester] reviews a paged comparison and preserves cross-page selection changes', async () => {
    inspectDataSyncMock.mockResolvedValue([
      { sourceTable: 'users', targetTable: 'users', status: 'MATCHED' },
    ]);
    compareDataSyncMock.mockResolvedValue({
      contractVersion: 1,
      planId: 'paged-plan',
      selectionRevision: 1,
      pageSize: 1,
      tables: [
        {
          sourceTable: 'users',
          targetTable: 'users',
          status: 'MATCHED',
          columns: ['id', 'name'],
          insertCount: 2,
          updateCount: 0,
          deleteCount: 0,
          unchangedCount: 0,
          rowCount: 2,
          pageSize: 1,
          firstCursor: 'cursor-0',
          hasMore: true,
        },
      ],
    });
    getDataSyncComparisonPageMock.mockImplementation(
      async (cursor: string | null, sourceTable: string, targetTable: string) => {
        expect(sourceTable).toBe('users');
        expect(targetTable).toBe('users');
        if (cursor === 'cursor-0') {
          return {
            contractVersion: 1,
            planId: 'paged-plan',
            sourceTable,
            targetTable,
            cursor,
            nextCursor: 'cursor-1',
            hasMore: true,
            pageSize: 1,
            rows: [{ ...insertRow(), key: [1], sourceRow: [[1, 'alice']] }],
          };
        }
        expect(cursor).toBe('cursor-1');
        return {
          contractVersion: 1,
          planId: 'paged-plan',
          sourceTable,
          targetTable,
          cursor,
          nextCursor: null,
          hasMore: false,
          pageSize: 1,
          rows: [{ ...insertRow(), key: [2], sourceRow: [[2, 'bob']] }],
        };
      },
    );

    render(<DataSyncWindow />);
    await advanceToCompare();
    const review = await screen.findByTestId('data-sync-row-diff');
    await waitFor(() =>
      expect(getDataSyncComparisonPageMock).toHaveBeenCalledWith('cursor-0', 'users', 'users', 1),
    );

    // Cancel the first row, advance to the next page, then cancel its row too.
    let checkbox = within(review).getByRole('checkbox');
    expect(checkbox).toBeChecked();
    fireEvent.click(checkbox);
    fireEvent.click(screen.getByText('sync.pageNext'));
    await waitFor(() => expect(within(review).getByRole('checkbox')).toBeChecked());
    fireEvent.click(within(review).getByRole('checkbox'));

    // Going back and forward must keep both exclusions instead of restoring
    // the page's server default selection.
    fireEvent.click(screen.getByText('sync.pagePrev'));
    await waitFor(() => expect(within(review).getByRole('checkbox')).not.toBeChecked());
    fireEvent.click(screen.getByText('sync.pageNext'));
    await waitFor(() => expect(within(review).getByRole('checkbox')).not.toBeChecked());

    // Restore the second row and verify only that key is submitted for SQL.
    fireEvent.click(within(review).getByRole('checkbox'));
    fireEvent.click(screen.getByTestId('data-sync-next'));
    await screen.findByTestId('data-sync-preview');
    await waitFor(() => expect(generateDataSyncSqlMock).toHaveBeenCalled());
    expect(generateDataSyncSqlMock.mock.calls.at(-1)?.[8]).toEqual([
      { sourceTable: 'users', targetTable: 'users', operation: 'INSERT', key: [2] },
    ]);
  });

  it('[tester] restores default selection across every page after clearing a table scope', async () => {
    inspectDataSyncMock.mockResolvedValue([
      { sourceTable: 'users', targetTable: 'users', status: 'MATCHED' },
    ]);
    compareDataSyncMock.mockResolvedValue({
      contractVersion: 1,
      planId: 'paged-clear-plan',
      selectionRevision: 1,
      pageSize: 1,
      tables: [
        {
          sourceTable: 'users',
          targetTable: 'users',
          status: 'MATCHED',
          columns: ['id', 'name'],
          insertCount: 2,
          updateCount: 0,
          deleteCount: 0,
          unchangedCount: 0,
          rowCount: 2,
          pageSize: 1,
          firstCursor: 'clear-cursor-0',
          hasMore: true,
        },
      ],
    });
    getDataSyncComparisonPageMock.mockImplementation(
      async (cursor: string | null, sourceTable: string, targetTable: string) => ({
        contractVersion: 1,
        planId: 'paged-clear-plan',
        sourceTable,
        targetTable,
        cursor,
        nextCursor: cursor === 'clear-cursor-0' ? 'clear-cursor-1' : null,
        hasMore: cursor === 'clear-cursor-0',
        pageSize: 1,
        rows: [
          {
            ...insertRow(),
            key: [cursor === 'clear-cursor-0' ? 1 : 2],
            sourceRow: [[cursor === 'clear-cursor-0' ? 1 : 2, 'user']],
          },
        ],
      }),
    );

    render(<DataSyncWindow />);
    await advanceToCompare();
    const review = await screen.findByTestId('data-sync-row-diff');
    await waitFor(() =>
      expect(getDataSyncComparisonPageMock).toHaveBeenCalledWith(
        'clear-cursor-0',
        'users',
        'users',
        1,
      ),
    );

    fireEvent.click(screen.getByTestId('data-sync-select-all-INSERT'));
    fireEvent.click(screen.getByText('sync.pageNext'));
    await waitFor(() => expect(within(review).getByRole('checkbox')).toBeChecked());
    fireEvent.click(screen.getByTestId('data-sync-clear-all-INSERT'));

    fireEvent.click(screen.getByText('sync.pagePrev'));
    await waitFor(() => expect(within(review).getByRole('checkbox')).toBeChecked());
    fireEvent.click(screen.getByTestId('data-sync-next'));
    await screen.findByTestId('data-sync-preview');
    await waitFor(() => expect(generateDataSyncSqlMock).toHaveBeenCalled());
    expect(generateDataSyncSqlMock.mock.calls.at(-1)?.[8]).toEqual([]);
    expect(generateDataSyncSqlMock.mock.calls.at(-1)?.[9]).toEqual([
      {
        sourceTable: 'users',
        targetTable: 'users',
        selectionMode: 'defaults',
        operations: ['INSERT'],
        excludedRows: [],
      },
    ]);
  });

  it('shows schema pickers for PostgreSQL when get_tables returns schemas', async () => {
    listTablesMock.mockResolvedValue([
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

  it('discovers schemas for any SQL driver that supports table metadata', async () => {
    listTablesMock.mockResolvedValue([
      { name: 'users', schema: 'public', tableType: 'table' },
      { name: 'users', schema: 'app', tableType: 'table' },
    ]);
    render(<DataSyncWindow />);
    await waitFor(() => expect(invokeMock).toHaveBeenCalledWith('get_connections'));
    await pickSelect('data-sync-source', 'My Tgt');
    await waitFor(() => expect(screen.getByTestId('data-sync-source-schema')).toBeTruthy());
    expect(screen.getByTestId('data-sync-source-schema')).toHaveTextContent('public');
    expect(listTablesMock).toHaveBeenCalledWith('dedicated-my-tgt-src', 'src');
  });

  it('marks heterogeneous targets as unsupported in the picker', async () => {
    render(<DataSyncWindow />);
    await waitFor(() => expect(invokeMock).toHaveBeenCalledWith('get_connections'));
    await pickSelect('data-sync-source', 'PG Src');
    const wrap = screen.getByTestId('data-sync-target');
    fireEvent.click(within(wrap).getAllByRole('button')[0]);
    const list = await waitFor(() => {
      return screen.getByRole('listbox');
    });
    const mysql = Array.from(list.children).find((el) => (el.textContent || '').includes('My Tgt'));
    expect(mysql?.textContent).toContain('common.unsupportedPair');
  });

  it('surfaces error when database list cannot be loaded', async () => {
    invokeMock.mockImplementation(
      async (
        cmd: string,
        args?: { connectionId?: string; sourceDatabaseType?: string; targetDatabaseType?: string },
      ) => {
        if (cmd === 'get_connections') return [pgSrc, pgTgt, mysqlTgt];
        if (cmd === 'connect_dedicated') {
          if (args?.connectionId === 'pg-tgt') throw new Error('refused');
          return `dedicated-${args?.connectionId}-db`;
        }
        if (cmd === 'release_connection') return false;
        if (cmd === 'connect') return `live-${args?.connectionId}`;
        if (cmd === 'classify_data_sync_pair') return mockClassifyDataSyncPair(args);
        return null;
      },
    );
    render(<DataSyncWindow />);
    await waitFor(() => expect(invokeMock).toHaveBeenCalledWith('get_connections'));
    await pickSelect('data-sync-source', 'PG Src');
    await pickSelect('data-sync-target', 'PG Tgt');
    expect(await screen.findByTestId('data-sync-error')).toHaveTextContent(
      'sync.loadDatabasesFailedrefused',
    );
    expect(inspectDataSyncMock).not.toHaveBeenCalled();
  });

  it('surfaces data-sync inspection errors', async () => {
    render(<DataSyncWindow />);
    await selectEndpoints();
    fireEvent.click(screen.getByTestId('data-sync-next'));
    await waitFor(() => expect(screen.getByTestId('data-sync-step-setup')).toBeTruthy());
    inspectDataSyncMock.mockRejectedValue('gate failed');
    fireEvent.click(screen.getByTestId('data-sync-next'));
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
    await selectEndpoints();
    expect(screen.getByTestId('data-sync-path')).toHaveTextContent('sync.pathDirect');
    fireEvent.click(screen.getByTestId('data-sync-next'));
    await waitFor(() => expect(screen.getByTestId('data-sync-step-setup')).toBeTruthy());
    fireEvent.click(screen.getByTestId('data-sync-next'));
    await waitFor(() => expect(screen.getByTestId('data-sync-objects-step')).toBeTruthy());
    await screen.findAllByTestId('data-sync-mapping-row');
    expect(screen.getByText('pk mismatch')).toBeTruthy();

    const matchedRow = screen.getAllByTestId('data-sync-mapping-row')[0];
    fireEvent.click(within(matchedRow).getByRole('checkbox'));
    expect(within(matchedRow).getByRole('checkbox')).not.toBeChecked();
  });

  it('cancel during compare resets sync state and re-enables Compare', async () => {
    inspectDataSyncMock.mockResolvedValue([
      { sourceTable: 'users', targetTable: 'users', status: 'MATCHED' },
    ]);
    let resolveCompare: ((value: unknown) => void) | undefined;
    compareDataSyncMock.mockImplementation(
      () =>
        new Promise((resolve) => {
          resolveCompare = resolve;
        }),
    );
    render(<DataSyncWindow />);
    await advanceToObjects();
    fireEvent.click(screen.getByTestId('data-sync-next'));
    await waitFor(() => expect(screen.getByTestId('data-sync-cancel')).toBeTruthy());
    expect(screen.getByTestId('data-sync-window')).toHaveAttribute('data-sync-state', 'comparing');
    fireEvent.click(screen.getByTestId('data-sync-cancel'));
    await waitFor(() =>
      expect(screen.getByTestId('data-sync-window')).toHaveAttribute('data-sync-state', 'compared'),
    );
    expect(cancelDataSyncMock).toHaveBeenCalled();
    expect(screen.getByTestId('data-sync-next')).not.toBeDisabled();
    resolveCompare?.([]);
  });

  it('keeps a completed no-change comparison reviewable without selecting a phantom row', async () => {
    inspectDataSyncMock.mockResolvedValue([
      { sourceTable: 'users', targetTable: 'users', status: 'MATCHED' },
    ]);
    compareDataSyncMock.mockResolvedValue([
      { sourceTable: 'users', targetTable: 'users', status: 'MATCHED', rows: [] },
    ]);
    render(<DataSyncWindow />);
    await advanceToCompare();
    expect(screen.getByTestId('data-sync-window')).toHaveAttribute('data-sync-state', 'compared');
    expect(screen.queryByTestId('data-sync-row-diff')).toBeNull();
    fireEvent.click(screen.getByTestId('data-sync-next'));
    await screen.findByTestId('data-sync-preview');
    expect(screen.getByTestId('data-sync-start-disabled')).toBeTruthy();
  });

  it('disables Execute and shows targetReadOnly for read-only target', async () => {
    invokeMock.mockImplementation(
      async (
        cmd: string,
        args?: {
          connectionId?: string;
          database?: string | null;
          sourceDatabaseType?: string;
          targetDatabaseType?: string;
        },
      ) => {
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
    await advanceToPreview('PG Tgt RO');
    expect(screen.getByTestId('data-sync-start-disabled')).toBeTruthy();
    expect(screen.getByText('sync.targetReadOnly')).toBeTruthy();
    expect(applyDataSyncMock).not.toHaveBeenCalled();
  });

  it('clears mapping when source connection changes after compare', async () => {
    invokeMock.mockImplementation(
      async (
        cmd: string,
        args?: {
          connectionId?: string;
          database?: string | null;
          sourceDatabaseType?: string;
          targetDatabaseType?: string;
        },
      ) => {
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
    await advanceToCompare();
    expect(screen.getByTestId('data-sync-window')).toHaveAttribute('data-sync-state', 'compared');
    fireEvent.click(screen.getByTestId('data-sync-back'));
    fireEvent.click(screen.getByTestId('data-sync-back'));
    fireEvent.click(screen.getByTestId('data-sync-back'));
    await waitFor(() => expect(screen.getByTestId('data-sync-source')).toBeTruthy());
    await pickSelect('data-sync-source', 'PG Src Alt');
    await waitFor(() =>
      expect(screen.getByTestId('data-sync-window')).toHaveAttribute('data-sync-state', 'idle'),
    );
    expect(screen.queryAllByTestId('data-sync-mapping-row')).toHaveLength(0);
    expect(screen.getByTestId('data-sync-step-endpoints')).toBeTruthy();
  });

  it('[tester] opens execute confirm dialog when delete rows are selected', async () => {
    inspectDataSyncMock.mockResolvedValue([
      { sourceTable: 'users', targetTable: 'users', status: 'MATCHED' },
    ]);
    compareDataSyncMock.mockResolvedValue([
      {
        sourceTable: 'users',
        targetTable: 'users',
        status: 'MATCHED',
        rows: [deleteRow()],
      },
    ]);
    applyDataSyncMock.mockResolvedValue({ applied: 1, rolledBack: false });
    render(<DataSyncWindow />);
    await advanceToCompare();
    fireEvent.click(screen.getByTestId('data-sync-back'));
    fireEvent.click(screen.getByTestId('data-sync-back'));
    await waitFor(() => expect(screen.getByTestId('data-sync-step-setup')).toBeTruthy());
    fireEvent.click(screen.getByTestId('data-sync-option-delete'));
    expect(await screen.findByText('sync.deleteConfirmTitle')).toBeTruthy();
    fireEvent.click(screen.getByText('sync.enableDelete'));
    fireEvent.click(screen.getByTestId('data-sync-next'));
    await waitFor(() => expect(screen.getByTestId('data-sync-objects-step')).toBeTruthy());
    fireEvent.click(screen.getByTestId('data-sync-next'));
    await waitFor(() => expect(screen.getByTestId('data-sync-summary')).toBeTruthy());
    fireEvent.click(screen.getByTestId('data-sync-next'));
    await waitFor(() => expect(screen.getByTestId('data-sync-preview')).toBeTruthy());
    fireEvent.click(screen.getByTestId('data-sync-start'));
    expect(await screen.findByText('sync.executeDeleteTitle')).toBeTruthy();
    const confirmButtons = screen.getAllByText('sync.execute');
    fireEvent.click(confirmButtons[confirmButtons.length - 1]!);
    await waitFor(() => expect(executeDataSyncMock).toHaveBeenCalled());
    expect(applyDataSyncMock).not.toHaveBeenCalled();
  });

  it('[tester] preserves syncOptions when source endpoint changes after setup', async () => {
    invokeMock.mockImplementation(
      async (
        cmd: string,
        args?: {
          connectionId?: string;
          database?: string | null;
          sourceDatabaseType?: string;
          targetDatabaseType?: string;
        },
      ) => {
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
    render(<DataSyncWindow />);
    await selectEndpoints();
    fireEvent.click(screen.getByTestId('data-sync-next'));
    await waitFor(() => expect(screen.getByTestId('data-sync-step-setup')).toBeTruthy());

    const insertCheckbox = screen.getByTestId('data-sync-option-insert') as HTMLInputElement;
    expect(insertCheckbox.checked).toBe(true);
    fireEvent.click(insertCheckbox);
    expect(insertCheckbox.checked).toBe(false);

    fireEvent.click(screen.getByTestId('data-sync-back'));
    await waitFor(() => expect(screen.getByTestId('data-sync-source')).toBeTruthy());
    await pickSelect('data-sync-source', 'PG Src Alt');

    fireEvent.click(screen.getByTestId('data-sync-next'));
    await waitFor(() => expect(screen.getByTestId('data-sync-step-setup')).toBeTruthy());
    expect((screen.getByTestId('data-sync-option-insert') as HTMLInputElement).checked).toBe(false);
  });

  it('prefills connection endpoints from URL params', async () => {
    urlParamMock.mockImplementation((name) => {
      const params: Record<string, string> = {
        sourceId: 'pg-src',
        targetId: 'pg-tgt',
      };
      return params[name] ?? null;
    });
    render(<DataSyncWindow />);
    await waitFor(() => expect(invokeMock).toHaveBeenCalledWith('get_connections'));
    await waitFor(() => expect(screen.getByTestId('data-sync-source')).toHaveTextContent('PG Src'));
    await waitFor(() => expect(screen.getByTestId('data-sync-target')).toHaveTextContent('PG Tgt'));
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
    await advanceToCompare();
    fireEvent.click(screen.getByTestId('data-sync-explain-diff'));
    await waitFor(() => expect(aiChatMock).toHaveBeenCalled());
    expect(await screen.findByTestId('data-sync-explain-result')).toHaveTextContent(
      'AI summary of diffs',
    );
  });
  it.each(['generate', 'execute'] as const)(
    'preserves deselection and never falls back after %s failure',
    async (phase) => {
      inspectDataSyncMock.mockResolvedValue([
        { sourceTable: 'users', targetTable: 'clients', status: 'MATCHED' },
      ]);
      compareDataSyncMock.mockResolvedValue([
        {
          sourceTable: 'users',
          targetTable: 'clients',
          status: 'MATCHED',
          columns: ['id', 'name'],
          rows: [insertRow(), { ...insertRow(), key: [9] }],
        },
      ]);
      render(<DataSyncWindow />);
      await advanceToCompare();
      const review = screen.getByTestId('data-sync-row-diff');
      const choices = within(review).getAllByRole('checkbox');
      fireEvent.click(choices[1]);
      expect(choices[1]).not.toBeChecked();
      fireEvent.click(screen.getByTestId('data-sync-next'));
      await screen.findByTestId('data-sync-preview');
      if (phase === 'generate') generateDataSyncSqlMock.mockRejectedValue('generation failed');
      else executeDataSyncMock.mockRejectedValue(new Error('commit response lost'));
      fireEvent.click(screen.getByTestId('data-sync-start'));
      await screen.findByTestId('data-sync-error');
      expect(applyDataSyncMock).not.toHaveBeenCalled();
      if (phase === 'generate') expect(executeDataSyncMock).not.toHaveBeenCalled();
      else {
        expect(executeDataSyncMock.mock.calls[0][1]).toHaveLength(1);
        expect(screen.getByTestId('data-sync-window')).toHaveAttribute(
          'data-sync-state',
          'unknown',
        );
        expect(screen.getByTestId('data-sync-start-disabled')).toBeTruthy();
      }
      const submitted = generateDataSyncSqlMock.mock.calls.at(-1)?.[2][0];
      expect(submitted.targetTable).toBe('clients');
      expect(submitted.rows.map((row: DataSyncRowChange) => row.selected)).toEqual([true, false]);
      expect(compareDataSyncMock).toHaveBeenCalledTimes(1);
    },
  );

  it('empty generated selection never triggers any execution', async () => {
    inspectDataSyncMock.mockResolvedValue([
      { sourceTable: 'users', targetTable: 'users', status: 'MATCHED' },
    ]);
    compareDataSyncMock.mockResolvedValue([
      { sourceTable: 'users', targetTable: 'users', status: 'MATCHED', rows: [insertRow()] },
    ]);
    generateDataSyncSqlMock.mockResolvedValue([]);
    render(<DataSyncWindow />);
    await advanceToPreview();
    fireEvent.click(screen.getByTestId('data-sync-start'));
    await waitFor(() =>
      expect(screen.getByTestId('data-sync-window')).toHaveAttribute('data-sync-state', 'compared'),
    );
    expect(executeDataSyncMock).not.toHaveBeenCalled();
    expect(applyDataSyncMock).not.toHaveBeenCalled();
  });

  it('[tester] cancelling a fresh comparison must not unlock an unknown write outcome', async () => {
    inspectDataSyncMock.mockResolvedValue([
      { sourceTable: 'users', targetTable: 'users', status: 'MATCHED' },
    ]);
    compareDataSyncMock.mockResolvedValue([
      { sourceTable: 'users', targetTable: 'users', status: 'MATCHED', rows: [insertRow()] },
    ]);
    render(<DataSyncWindow />);
    await advanceToPreview();
    executeDataSyncMock.mockRejectedValue(new Error('commit response lost'));
    fireEvent.click(screen.getByTestId('data-sync-start'));
    await screen.findByTestId('data-sync-error');
    fireEvent.click(screen.getByText('common.ok'));
    fireEvent.click(screen.getByTestId('data-sync-back'));
    fireEvent.click(screen.getByTestId('data-sync-back'));
    compareDataSyncMock.mockImplementation(() => new Promise(() => {}));
    fireEvent.click(screen.getByTestId('data-sync-next'));
    fireEvent.click(await screen.findByTestId('data-sync-cancel'));
    await waitFor(() => expect(screen.queryByTestId('data-sync-cancel')).toBeNull());
    fireEvent.click(screen.getByTestId('data-sync-next'));
    await screen.findByTestId('data-sync-preview');
    expect(screen.queryByTestId('data-sync-start')).toBeNull();
    expect(screen.getByTestId('data-sync-window')).toHaveAttribute(
      'data-write-outcome-uncertain',
      'true',
    );
    expect(executeDataSyncMock).toHaveBeenCalledTimes(1);

    fireEvent.click(screen.getByTestId('data-sync-back'));
    fireEvent.click(screen.getByTestId('data-sync-back'));
    compareDataSyncMock.mockReset();
    compareDataSyncMock.mockResolvedValue([
      { sourceTable: 'users', targetTable: 'users', status: 'MATCHED', rows: [insertRow()] },
    ]);
    fireEvent.click(screen.getByTestId('data-sync-next'));
    await screen.findByTestId('data-sync-summary');
    expect(screen.getByTestId('data-sync-window')).toHaveAttribute(
      'data-write-outcome-uncertain',
      'false',
    );
    fireEvent.click(screen.getByTestId('data-sync-next'));
    await screen.findByTestId('data-sync-start');
  });

  it('[tester] invalidates a comparison before awaiting the cancel response', async () => {
    inspectDataSyncMock.mockResolvedValue([
      { sourceTable: 'users', targetTable: 'users', status: 'MATCHED' },
    ]);
    compareDataSyncMock.mockResolvedValue([
      { sourceTable: 'users', targetTable: 'users', status: 'MATCHED', rows: [insertRow()] },
    ]);
    render(<DataSyncWindow />);
    await advanceToPreview();
    executeDataSyncMock.mockRejectedValue(new Error('commit response lost'));
    fireEvent.click(screen.getByTestId('data-sync-start'));
    await screen.findByTestId('data-sync-error');
    fireEvent.click(screen.getByText('common.ok'));

    fireEvent.click(screen.getByTestId('data-sync-back'));
    fireEvent.click(screen.getByTestId('data-sync-back'));
    let resolveCompare!: (rows: Array<Record<string, unknown>>) => void;
    compareDataSyncMock.mockImplementationOnce(
      () =>
        new Promise((resolve) => {
          resolveCompare = resolve;
        }),
    );
    let resolveCancel!: (cancelled: boolean) => void;
    cancelDataSyncMock.mockImplementationOnce(
      () =>
        new Promise((resolve) => {
          resolveCancel = resolve;
        }),
    );
    fireEvent.click(screen.getByTestId('data-sync-next'));
    fireEvent.click(await screen.findByTestId('data-sync-cancel'));
    await waitFor(() => expect(cancelDataSyncMock).toHaveBeenCalledTimes(1));

    resolveCompare([
      { sourceTable: 'users', targetTable: 'users', status: 'MATCHED', rows: [insertRow()] },
    ]);
    await waitFor(() => expect(screen.queryByTestId('data-sync-cancel')).toBeNull());
    resolveCancel(true);
    await waitFor(() =>
      expect(screen.getByTestId('data-sync-window')).toHaveAttribute('data-sync-state', 'unknown'),
    );
    expect(screen.getByTestId('data-sync-window')).toHaveAttribute(
      'data-write-outcome-uncertain',
      'true',
    );
  });

  it('[tester] lets a fresh comparison win while an older cancel response is delayed', async () => {
    inspectDataSyncMock.mockResolvedValue([
      { sourceTable: 'users', targetTable: 'users', status: 'MATCHED' },
    ]);
    compareDataSyncMock.mockResolvedValue([
      { sourceTable: 'users', targetTable: 'users', status: 'MATCHED', rows: [insertRow()] },
    ]);
    render(<DataSyncWindow />);
    await advanceToPreview();
    executeDataSyncMock.mockRejectedValueOnce(new Error('commit response lost'));
    fireEvent.click(screen.getByTestId('data-sync-start'));
    await screen.findByTestId('data-sync-error');
    fireEvent.click(screen.getByText('common.ok'));

    fireEvent.click(screen.getByTestId('data-sync-back'));
    fireEvent.click(screen.getByTestId('data-sync-back'));
    let resolveOldCompare!: (rows: Array<Record<string, unknown>>) => void;
    compareDataSyncMock.mockImplementationOnce(
      () =>
        new Promise((resolve) => {
          resolveOldCompare = resolve;
        }),
    );
    compareDataSyncMock.mockResolvedValue([
      {
        sourceTable: 'users',
        targetTable: 'users',
        status: 'MATCHED',
        rows: [{ ...insertRow(), key: [9], sourceRow: [[9, 'fresh']] }],
      },
    ]);
    let resolveOldCancel!: (cancelled: boolean) => void;
    cancelDataSyncMock.mockImplementationOnce(
      () =>
        new Promise((resolve) => {
          resolveOldCancel = resolve;
        }),
    );
    fireEvent.click(screen.getByTestId('data-sync-next'));
    fireEvent.click(await screen.findByTestId('data-sync-cancel'));
    await waitFor(() => expect(cancelDataSyncMock).toHaveBeenCalledTimes(1));

    const oldCompareJob = compareDataSyncMock.mock.calls[1][3];
    expect(cancelDataSyncMock.mock.calls[0][0]).toBe(oldCompareJob);
    resolveOldCompare([
      { sourceTable: 'users', targetTable: 'users', status: 'MATCHED', rows: [insertRow()] },
    ]);
    await screen.findByTestId('data-sync-objects-step');
    fireEvent.click(screen.getByTestId('data-sync-next'));
    await screen.findByTestId('data-sync-summary');
    const freshCompareJob = compareDataSyncMock.mock.calls[2][3];
    expect(freshCompareJob).not.toBe(oldCompareJob);
    expect(screen.getByTestId('data-sync-window')).toHaveAttribute(
      'data-write-outcome-uncertain',
      'false',
    );

    resolveOldCancel(true);
    await waitFor(() =>
      expect(screen.getByTestId('data-sync-window')).toHaveAttribute('data-sync-state', 'compared'),
    );
    expect(screen.getByTestId('data-sync-window')).toHaveAttribute(
      'data-write-outcome-uncertain',
      'false',
    );
    fireEvent.click(screen.getByTestId('data-sync-next'));
    const generateCallsBeforeExecute = generateDataSyncSqlMock.mock.calls.length;
    fireEvent.click(await screen.findByTestId('data-sync-start'));
    await waitFor(() =>
      expect(generateDataSyncSqlMock).toHaveBeenCalledTimes(generateCallsBeforeExecute + 1),
    );
    expect(generateDataSyncSqlMock.mock.calls.at(-1)?.[2][0].rows[0].key).toEqual([9]);
  });

  it('[tester] a stale cancel response never clears the newer comparison job id', async () => {
    inspectDataSyncMock.mockResolvedValue([
      { sourceTable: 'users', targetTable: 'users', status: 'MATCHED' },
    ]);
    let resolveOldCompare!: (rows: Array<Record<string, unknown>>) => void;
    let resolveFreshCompare!: (rows: Array<Record<string, unknown>>) => void;
    compareDataSyncMock
      .mockImplementationOnce(
        () =>
          new Promise((resolve) => {
            resolveOldCompare = resolve;
          }),
      )
      .mockImplementationOnce(
        () =>
          new Promise((resolve) => {
            resolveFreshCompare = resolve;
          }),
      );
    let resolveOldCancel!: (cancelled: boolean) => void;
    cancelDataSyncMock.mockImplementationOnce(
      () =>
        new Promise((resolve) => {
          resolveOldCancel = resolve;
        }),
    );
    render(<DataSyncWindow />);
    await advanceToObjects();
    fireEvent.click(screen.getByTestId('data-sync-next'));
    fireEvent.click(await screen.findByTestId('data-sync-cancel'));
    await waitFor(() => expect(cancelDataSyncMock).toHaveBeenCalledTimes(1));
    const oldJob = compareDataSyncMock.mock.calls[0][3];
    expect(cancelDataSyncMock.mock.calls[0][0]).toBe(oldJob);

    resolveOldCompare([]);
    await screen.findByTestId('data-sync-objects-step');
    fireEvent.click(screen.getByTestId('data-sync-next'));
    await screen.findByTestId('data-sync-cancel');
    const freshJob = compareDataSyncMock.mock.calls[1][3];
    expect(freshJob).not.toBe(oldJob);

    resolveOldCancel(true);
    await waitFor(() => expect(screen.getByTestId('data-sync-cancel')).toBeTruthy());
    expect(screen.getByTestId('data-sync-window')).toHaveAttribute('data-sync-state', 'comparing');
    fireEvent.click(screen.getByTestId('data-sync-cancel'));
    await waitFor(() => expect(cancelDataSyncMock).toHaveBeenCalledTimes(2));
    expect(cancelDataSyncMock.mock.calls[1][0]).toBe(freshJob);
    resolveFreshCompare([]);
  });

  it('keeps an unknown write fenced across failed comparison and inspection attempts', async () => {
    inspectDataSyncMock.mockResolvedValue([
      { sourceTable: 'users', targetTable: 'users', status: 'MATCHED' },
    ]);
    compareDataSyncMock.mockResolvedValue([
      { sourceTable: 'users', targetTable: 'users', status: 'MATCHED', rows: [insertRow()] },
    ]);
    render(<DataSyncWindow />);
    await advanceToPreview();
    executeDataSyncMock.mockRejectedValue(new Error('commit response lost'));
    fireEvent.click(screen.getByTestId('data-sync-start'));
    await screen.findByTestId('data-sync-error');
    fireEvent.click(screen.getByText('common.ok'));

    fireEvent.click(screen.getByTestId('data-sync-back'));
    fireEvent.click(screen.getByTestId('data-sync-back'));
    compareDataSyncMock.mockRejectedValueOnce('fresh compare failed');
    fireEvent.click(screen.getByTestId('data-sync-next'));
    expect(await screen.findByTestId('data-sync-error')).toHaveTextContent('fresh compare failed');
    expect(screen.getByTestId('data-sync-window')).toHaveAttribute(
      'data-write-outcome-uncertain',
      'true',
    );

    fireEvent.click(screen.getByText('common.ok'));
    fireEvent.click(screen.getByTestId('data-sync-back'));
    fireEvent.click(screen.getByTestId('data-sync-back'));
    fireEvent.click(screen.getByTestId('data-sync-back'));
    await pickSelect('data-sync-source', 'PG Src');
    inspectDataSyncMock.mockRejectedValueOnce(new Error('fresh inspect failed'));
    fireEvent.click(screen.getByTestId('data-sync-next'));
    fireEvent.click(await screen.findByTestId('data-sync-next'));
    expect(await screen.findByTestId('data-sync-error')).toHaveTextContent('fresh inspect failed');
    expect(screen.getByTestId('data-sync-window')).toHaveAttribute(
      'data-write-outcome-uncertain',
      'true',
    );
  });

  it('keeps cancellation fenced while a database write still has no outcome', async () => {
    inspectDataSyncMock.mockResolvedValue([
      { sourceTable: 'users', targetTable: 'users', status: 'MATCHED' },
    ]);
    compareDataSyncMock.mockResolvedValue([
      { sourceTable: 'users', targetTable: 'users', status: 'MATCHED', rows: [insertRow()] },
    ]);
    let rejectWrite!: (error: Error) => void;
    executeDataSyncMock.mockImplementationOnce(
      () =>
        new Promise((_resolve, reject) => {
          rejectWrite = reject;
        }),
    );
    render(<DataSyncWindow />);
    await advanceToPreview();
    fireEvent.click(screen.getByTestId('data-sync-start'));
    await waitFor(() => expect(rejectWrite).toBeDefined());
    fireEvent.click(screen.getByTestId('data-sync-cancel'));
    await waitFor(() =>
      expect(screen.getByTestId('status-bar')).toHaveTextContent('sync.cancellingExecution'),
    );
    expect(screen.getByTestId('data-sync-window')).toHaveAttribute('data-sync-state', 'executing');
    rejectWrite(new Error('commit response lost'));
    await screen.findByTestId('data-sync-error');
    expect(screen.getByTestId('data-sync-window')).toHaveAttribute(
      'data-write-outcome-uncertain',
      'true',
    );
    expect(executeDataSyncMock).toHaveBeenCalledTimes(1);
  });

  it('[tester] a delayed execution-cancel response cannot overwrite a successful result', async () => {
    inspectDataSyncMock.mockResolvedValue([
      { sourceTable: 'users', targetTable: 'users', status: 'MATCHED' },
    ]);
    compareDataSyncMock.mockResolvedValue([
      { sourceTable: 'users', targetTable: 'users', status: 'MATCHED', rows: [insertRow()] },
    ]);
    let resolveWrite!: (result: { applied: number; rolledBack: boolean }) => void;
    executeDataSyncMock.mockImplementationOnce(
      () =>
        new Promise((resolve) => {
          resolveWrite = resolve;
        }),
    );
    let resolveCancel!: (cancelled: boolean) => void;
    cancelDataSyncMock.mockImplementationOnce(
      () =>
        new Promise((resolve) => {
          resolveCancel = resolve;
        }),
    );
    render(<DataSyncWindow />);
    await advanceToPreview();
    fireEvent.click(screen.getByTestId('data-sync-start'));
    await waitFor(() => expect(resolveWrite).toBeDefined());
    fireEvent.click(screen.getByTestId('data-sync-cancel'));
    await waitFor(() => expect(cancelDataSyncMock).toHaveBeenCalledTimes(1));

    resolveWrite({ applied: 1, rolledBack: false });
    await screen.findByTestId('data-sync-result');
    expect(screen.getByTestId('data-sync-window')).toHaveAttribute('data-sync-state', 'done');
    await act(async () => {
      resolveCancel(true);
    });
    expect(screen.getByTestId('data-sync-window')).toHaveAttribute('data-sync-state', 'done');
    expect(screen.getByTestId('status-bar')).not.toHaveTextContent('sync.compareCancelled');
  });

  it.each([
    {
      outcome: 'rollback',
      expectedState: 'done',
    },
    {
      outcome: 'unknown',
      expectedState: 'unknown',
      expectedError: 'sync.executionUnknown',
    },
  ] as const)(
    '[tester] a delayed execution-cancel response cannot leave $outcome in a cancelling phase',
    async ({ outcome, expectedState, expectedError }) => {
      inspectDataSyncMock.mockResolvedValue([
        { sourceTable: 'users', targetTable: 'users', status: 'MATCHED' },
      ]);
      compareDataSyncMock.mockResolvedValue([
        { sourceTable: 'users', targetTable: 'users', status: 'MATCHED', rows: [insertRow()] },
      ]);
      let resolveWrite!: (result: {
        applied: number;
        rolledBack: boolean;
        rollbackReason?: string;
      }) => void;
      let rejectWrite!: (error: Error) => void;
      executeDataSyncMock.mockImplementationOnce(
        () =>
          new Promise((resolve, reject) => {
            resolveWrite = resolve;
            rejectWrite = reject;
          }),
      );
      let resolveCancel!: (cancelled: boolean) => void;
      cancelDataSyncMock.mockImplementationOnce(
        () =>
          new Promise((resolve) => {
            resolveCancel = resolve;
          }),
      );
      render(<DataSyncWindow />);
      await advanceToPreview();
      fireEvent.click(screen.getByTestId('data-sync-start'));
      await waitFor(() => expect(resolveWrite).toBeDefined());
      fireEvent.click(screen.getByTestId('data-sync-cancel'));
      await waitFor(() => expect(cancelDataSyncMock).toHaveBeenCalledTimes(1));

      if (outcome === 'rollback') {
        resolveWrite({
          applied: 0,
          rolledBack: true,
          rollbackReason: 'optimistic sync conflict; all changes were rolled back',
        });
      } else rejectWrite(new Error('commit response lost'));

      if (outcome === 'rollback') {
        expect(await screen.findByTestId('data-sync-result')).toBeTruthy();
        expect(screen.getByTestId('data-sync-execute-done')).toHaveTextContent(
          'optimistic sync conflict; all changes were rolled back',
        );
        expect(screen.getByTestId('data-sync-re-compare')).toBeTruthy();
      } else {
        expect(await screen.findByTestId('data-sync-error')).toHaveTextContent(expectedError!);
      }
      expect(screen.getByTestId('data-sync-window')).toHaveAttribute(
        'data-sync-state',
        expectedState,
      );

      await act(async () => {
        resolveCancel(true);
      });
      expect(screen.getByTestId('data-sync-window')).toHaveAttribute(
        'data-sync-state',
        expectedState,
      );
      if (outcome === 'rollback') {
        expect(screen.queryByTestId('data-sync-error')).toBeNull();
      } else {
        expect(screen.getByTestId('data-sync-error')).toHaveTextContent(expectedError!);
      }
      expect(screen.getByTestId('status-bar')).not.toHaveTextContent('sync.cancellingExecution');
      expect(screen.getByTestId('status-bar')).not.toHaveTextContent('sync.compareCancelled');
    },
  );

  it('[tester] an explicit unknown execution response never renders a successful completion label', async () => {
    inspectDataSyncMock.mockResolvedValue([
      { sourceTable: 'users', targetTable: 'users', status: 'MATCHED' },
    ]);
    compareDataSyncMock.mockResolvedValue([
      { sourceTable: 'users', targetTable: 'users', status: 'MATCHED', rows: [insertRow()] },
    ]);
    executeDataSyncMock.mockResolvedValue({
      applied: 0,
      rolledBack: false,
      outcome: 'unknown',
      error: 'sync.executionUnknown',
    });
    render(<DataSyncWindow />);
    await advanceToPreview();
    fireEvent.click(screen.getByTestId('data-sync-start'));

    expect(await screen.findByTestId('data-sync-error')).toHaveTextContent('sync.executionUnknown');
    await screen.findByTestId('data-sync-result');
    const completionLabel = screen.getByTestId('data-sync-execute-done');
    expect(completionLabel).toHaveTextContent('sync.executionUnknown');
    expect(completionLabel).not.toHaveTextContent('sync.executeDone');
    expect(completionLabel).not.toHaveClass('text-green-700');
    expect(completionLabel).toHaveClass('text-amber-700');
    expect(completionLabel).toHaveAttribute('role', 'status');
    expect(screen.getByTestId('data-sync-window')).toHaveAttribute('data-sync-state', 'unknown');
    expect(screen.getByTestId('data-sync-window')).toHaveAttribute(
      'data-write-outcome-uncertain',
      'true',
    );
  });

  it('a confirmed rollback creates a fresh comparison plan before allowing another execute', async () => {
    inspectDataSyncMock.mockResolvedValue([
      { sourceTable: 'users', targetTable: 'users', status: 'MATCHED' },
    ]);
    compareDataSyncMock.mockResolvedValue([
      { sourceTable: 'users', targetTable: 'users', status: 'MATCHED', rows: [insertRow()] },
    ]);
    render(<DataSyncWindow />);
    await advanceToPreview();
    executeDataSyncMock.mockResolvedValueOnce({
      applied: 0,
      rolledBack: true,
      rollbackReason: 'target row changed; all changes were rolled back',
    });
    fireEvent.click(screen.getByTestId('data-sync-start'));
    await screen.findByTestId('data-sync-result');
    expect(compareDataSyncMock).toHaveBeenCalledTimes(2);
    expect(screen.getByTestId('data-sync-window')).toHaveAttribute('data-sync-state', 'done');
    expect(screen.getByTestId('data-sync-window')).toHaveAttribute(
      'data-write-outcome-uncertain',
      'false',
    );
    expect(executeDataSyncMock).toHaveBeenCalledTimes(1);
    expect(applyDataSyncMock).not.toHaveBeenCalled();
  });

  it('keeps a known write outcome when the follow-up comparison fails', async () => {
    inspectDataSyncMock.mockResolvedValue([
      { sourceTable: 'users', targetTable: 'users', status: 'MATCHED' },
    ]);
    compareDataSyncMock
      .mockResolvedValueOnce([
        { sourceTable: 'users', targetTable: 'users', status: 'MATCHED', rows: [insertRow()] },
      ])
      .mockRejectedValueOnce(new Error('fresh compare failed'));
    executeDataSyncMock.mockResolvedValueOnce({ applied: 1, rolledBack: false });

    render(<DataSyncWindow />);
    await advanceToPreview();
    fireEvent.click(screen.getByTestId('data-sync-start'));

    expect(await screen.findByTestId('data-sync-error')).toHaveTextContent('fresh compare failed');
    expect(screen.getByTestId('data-sync-error')).not.toHaveTextContent('sync.executionUnknown');
    expect(screen.getByTestId('data-sync-window')).toHaveAttribute('data-sync-state', 'done');
    expect(screen.getByTestId('data-sync-window')).toHaveAttribute(
      'data-write-outcome-uncertain',
      'false',
    );
    expect(screen.getByTestId('data-sync-re-compare')).toBeTruthy();
  });

  it('[tester] saves the reviewed endpoint and option configuration as a profile', async () => {
    render(<DataSyncWindow />);
    await selectEndpoints();

    fireEvent.change(screen.getByTestId('data-sync-profile-name'), {
      target: { value: 'Nightly users' },
    });
    fireEvent.click(screen.getByTestId('data-sync-profile-save'));

    await waitFor(() => expect(saveSyncProfileMock).toHaveBeenCalledTimes(1));
    const [profile] = saveSyncProfileMock.mock.calls[0] as [
      {
        version: number;
        name: string;
        sourceConnectionId: string;
        targetConnectionId: string;
        tables: unknown[];
        options: { insert: boolean; update: boolean; delete: boolean };
      },
    ];
    expect(profile).toMatchObject({
      version: 1,
      name: 'Nightly users',
      sourceConnectionId: 'pg-src',
      targetConnectionId: 'pg-tgt',
      tables: [],
      options: { insert: true, update: true, delete: false },
    });
    expect(profile).not.toHaveProperty('sourceDbSessionId');
    expect(profile).not.toHaveProperty('targetDbSessionId');
    expect(profile).not.toHaveProperty('planId');
  });

  it('[tester] loads a profile and passes mappings with recordsets to a fresh inspect', async () => {
    const sourceFilter = {
      filters: [{ column: 'id', operator: 'gte', value: '10' }],
      logic: 'and' as const,
      recordset: {
        orderBy: 'id',
        start: { value: '10', inclusive: true },
        end: { value: '20', inclusive: false },
        limit: 25,
      },
    };
    const profile = {
      version: 1,
      id: 'profile-1',
      name: 'Scoped users',
      sourceConnectionId: 'pg-src',
      targetConnectionId: 'pg-tgt',
      sourceDatabase: 'src',
      targetDatabase: 'tgt',
      sourceSchema: null,
      targetSchema: null,
      tables: [{ sourceTable: 'users', targetTable: 'users', enabled: false, sourceFilter }],
      options: {
        insert: true,
        update: false,
        delete: true,
        matchingStrategy: 'primaryKey' as const,
        batchSize: 25,
        largeValueMode: 'hash' as const,
        conflictPolicy: 'skip' as const,
      },
      createdAt: '2026-09-21T00:00:00.000Z',
      updatedAt: '2026-09-21T00:00:00.000Z',
    };
    getSyncProfilesMock.mockResolvedValue([profile]);
    inspectDataSyncMock.mockResolvedValue([
      { sourceTable: 'users', targetTable: 'users', status: 'MATCHED', sourceFilter },
    ]);

    render(<DataSyncWindow />);
    await selectEndpoints();
    await waitFor(() => expect(screen.getByTestId('data-sync-profile-select')).toBeTruthy());
    chooseProfile(profile.name);
    fireEvent.click(screen.getByTestId('data-sync-profile-load'));
    expect(await screen.findByText('sync.profile.loaded')).toBeTruthy();

    await waitFor(() => expect(screen.getByTestId('data-sync-next')).not.toBeDisabled());
    fireEvent.click(screen.getByTestId('data-sync-next'));
    await waitFor(() => expect(screen.getByTestId('data-sync-step-setup')).toBeTruthy());
    fireEvent.click(screen.getByTestId('data-sync-next'));
    await waitFor(() => expect(inspectDataSyncMock).toHaveBeenCalledTimes(1));

    expect(inspectDataSyncMock.mock.calls[0]).toEqual([
      expect.stringContaining('dedicated-pg-src'),
      expect.stringContaining('dedicated-pg-tgt'),
      'src',
      'tgt',
      undefined,
      undefined,
      profile.tables,
    ]);
    expect(screen.getByTestId('data-sync-window')).toHaveAttribute('data-sync-state', 'idle');
    expect(screen.getByTestId('data-sync-mapping-row')).toHaveClass('opacity-60');
  });

  it('[tester] reports a missing connection when loading a stale profile', async () => {
    getSyncProfilesMock.mockResolvedValue([
      {
        version: 1,
        id: 'stale-profile',
        name: 'Stale',
        sourceConnectionId: 'deleted-source',
        targetConnectionId: 'pg-tgt',
        tables: [],
        options: { insert: true, update: true, delete: false },
        createdAt: '2026-09-21T00:00:00.000Z',
        updatedAt: '2026-09-21T00:00:00.000Z',
      },
    ]);
    render(<DataSyncWindow />);
    await waitFor(() => expect(screen.getByTestId('data-sync-profile-select')).toBeTruthy());
    chooseProfile('Stale');
    fireEvent.click(screen.getByTestId('data-sync-profile-load'));
    expect(await screen.findByTestId('data-sync-error')).toHaveTextContent(
      'sync.profile.missingConnection',
    );
    expect(inspectDataSyncMock).not.toHaveBeenCalled();
  });

  it('[tester] deletes the selected profile and refreshes the profile list', async () => {
    const profile = {
      version: 1,
      id: 'profile-delete',
      name: 'Delete me',
      sourceConnectionId: 'pg-src',
      targetConnectionId: 'pg-tgt',
      tables: [],
      options: { insert: true, update: true, delete: false },
      createdAt: '2026-09-21T00:00:00.000Z',
      updatedAt: '2026-09-21T00:00:00.000Z',
    };
    getSyncProfilesMock.mockResolvedValue([profile]);
    render(<DataSyncWindow />);
    await waitFor(() => expect(screen.getByTestId('data-sync-profile-select')).toBeTruthy());
    chooseProfile(profile.name);
    fireEvent.click(screen.getByTestId('data-sync-profile-delete'));
    await waitFor(() => expect(deleteSyncProfileMock).toHaveBeenCalledWith(profile.id));
    expect(await screen.findByText('sync.profile.deleted')).toBeTruthy();
  });

  it('reopens an unknown history run through a fresh profile-backed compare', async () => {
    const profile = historyRecoveryProfile();
    getSyncProfilesMock.mockResolvedValue([profile]);
    inspectDataSyncMock.mockResolvedValue([
      { sourceTable: 'users', targetTable: 'users', status: 'MATCHED' },
    ]);
    compareDataSyncMock.mockResolvedValue([
      { sourceTable: 'users', targetTable: 'users', status: 'MATCHED', rows: [insertRow()] },
    ]);
    render(<DataSyncWindow />);

    fireEvent.click(screen.getByText('migrationHistory.open'));
    fireEvent.click(await screen.findByText(/failed/));
    fireEvent.click(await screen.findByTestId('migration-run-reconcile'));
    await waitFor(() => expect(screen.getByTestId('data-sync-step-endpoints')).toBeTruthy());
    await waitFor(() =>
      expect(screen.getByTestId('data-sync-target-database')).toHaveTextContent('tgt'),
    );
    await waitFor(() => expect(screen.getByTestId('data-sync-next')).not.toBeDisabled());

    fireEvent.click(screen.getByTestId('data-sync-next'));
    await waitFor(() => expect(screen.getByTestId('data-sync-step-setup')).toBeTruthy());
    await waitFor(() => expect(screen.getByTestId('data-sync-next')).not.toBeDisabled());
    fireEvent.click(screen.getByTestId('data-sync-next'));
    await waitFor(() => expect(inspectDataSyncMock).toHaveBeenCalledTimes(1));
    await waitFor(() => expect(screen.getByTestId('data-sync-next')).not.toBeDisabled());
    fireEvent.click(screen.getByTestId('data-sync-next'));
    await screen.findByTestId('data-sync-summary');

    expect(compareDataSyncMock).toHaveBeenCalledTimes(1);
    expect(screen.getByTestId('data-sync-window')).toHaveAttribute('data-sync-state', 'compared');
    expect(screen.getByTestId('data-sync-window')).toHaveAttribute(
      'data-write-outcome-uncertain',
      'false',
    );
    expect(generateDataSyncSqlMock).not.toHaveBeenCalled();
    expect(executeDataSyncMock).not.toHaveBeenCalled();
  });

  it('requires scope reselection when the history profile revision changed', async () => {
    getSyncProfilesMock.mockResolvedValue([historyRecoveryProfile('2026-09-24T00:00:00.000Z')]);
    render(<DataSyncWindow />);

    fireEvent.click(screen.getByText('migrationHistory.open'));
    fireEvent.click(await screen.findByText(/failed/));
    fireEvent.click(await screen.findByTestId('migration-run-reconcile'));

    expect(await screen.findByText('migrationHistory.syncReconcileProfileChanged')).toBeTruthy();
    expect(screen.getByTestId('data-sync-recovery-scope-review')).toBeInTheDocument();
    expect(screen.getByTestId('data-sync-next')).toBeDisabled();
    expect(screen.getByTestId('data-sync-window')).toHaveAttribute(
      'data-write-outcome-uncertain',
      'true',
    );
    expect(inspectDataSyncMock).not.toHaveBeenCalled();
    expect(compareDataSyncMock).not.toHaveBeenCalled();

    // 「下一步」是否可用还取决于端点配对，而配对由 useSyncPairingState 的异步 effect
    // 写入（activePairing 初值为 null）。pairingCache 冷启动时每次分类还要多经过一次
    // await，effect 可能在本用例的同步断言之后才落定，表现为 next 仍是 disabled。
    // 该用例此前只有在同文件更早的用例预热缓存后才稳定通过，属于顺序依赖。
    // 先把这段异步落定，之后的断言才能确定地反映「重新确认作用域」这一个变量。
    await act(async () => {});

    fireEvent.click(screen.getByTestId('data-sync-reconfirm-scope'));
    expect(screen.queryByTestId('data-sync-recovery-scope-review')).not.toBeInTheDocument();
    // Belt-and-braces on the `await act` above: assert the outcome, not the tick,
    // so this holds regardless of which async settle the button is waiting on.
    await waitFor(() => expect(screen.getByTestId('data-sync-next')).not.toBeDisabled());
    expect(inspectDataSyncMock).not.toHaveBeenCalled();
    expect(compareDataSyncMock).not.toHaveBeenCalled();
  });

  it('keeps the history-run fence after a failed fresh comparison', async () => {
    getSyncProfilesMock.mockResolvedValue([historyRecoveryProfile()]);
    inspectDataSyncMock.mockResolvedValue([
      { sourceTable: 'users', targetTable: 'users', status: 'MATCHED' },
    ]);
    compareDataSyncMock.mockRejectedValueOnce(new Error('fresh history compare failed'));
    render(<DataSyncWindow />);

    fireEvent.click(screen.getByText('migrationHistory.open'));
    fireEvent.click(await screen.findByText(/failed/));
    fireEvent.click(await screen.findByTestId('migration-run-reconcile'));
    await waitFor(() => expect(screen.getByTestId('data-sync-next')).not.toBeDisabled());
    fireEvent.click(screen.getByTestId('data-sync-next'));
    await waitFor(() => expect(screen.getByTestId('data-sync-step-setup')).toBeTruthy());
    await waitFor(() => expect(screen.getByTestId('data-sync-next')).not.toBeDisabled());
    fireEvent.click(screen.getByTestId('data-sync-next'));
    await waitFor(() => expect(inspectDataSyncMock).toHaveBeenCalledTimes(1));
    await waitFor(() => expect(screen.getByTestId('data-sync-next')).not.toBeDisabled());
    fireEvent.click(screen.getByTestId('data-sync-next'));

    expect(await screen.findByTestId('data-sync-error')).toHaveTextContent(
      'fresh history compare failed',
    );
    expect(screen.getByTestId('data-sync-window')).toHaveAttribute('data-sync-state', 'unknown');
    expect(screen.getByTestId('data-sync-window')).toHaveAttribute(
      'data-write-outcome-uncertain',
      'true',
    );
    expect(executeDataSyncMock).not.toHaveBeenCalled();
  });

  it('does not let a cancelled stale compare clear an unknown history-run fence', async () => {
    getSyncProfilesMock.mockResolvedValue([historyRecoveryProfile()]);
    inspectDataSyncMock.mockResolvedValue([
      { sourceTable: 'users', targetTable: 'users', status: 'MATCHED' },
    ]);
    let resolveCompare!: (rows: Array<Record<string, unknown>>) => void;
    compareDataSyncMock.mockImplementationOnce(
      () =>
        new Promise((resolve) => {
          resolveCompare = resolve;
        }),
    );
    render(<DataSyncWindow />);

    fireEvent.click(screen.getByText('migrationHistory.open'));
    fireEvent.click(await screen.findByText(/failed/));
    fireEvent.click(await screen.findByTestId('migration-run-reconcile'));
    await waitFor(() => expect(screen.getByTestId('data-sync-next')).not.toBeDisabled());
    fireEvent.click(screen.getByTestId('data-sync-next'));
    await waitFor(() => expect(screen.getByTestId('data-sync-step-setup')).toBeTruthy());
    await waitFor(() => expect(screen.getByTestId('data-sync-next')).not.toBeDisabled());
    fireEvent.click(screen.getByTestId('data-sync-next'));
    await waitFor(() => expect(inspectDataSyncMock).toHaveBeenCalledTimes(1));
    await waitFor(() => expect(screen.getByTestId('data-sync-next')).not.toBeDisabled());
    fireEvent.click(screen.getByTestId('data-sync-next'));
    await waitFor(() => expect(resolveCompare).toBeDefined());
    fireEvent.click(await screen.findByTestId('data-sync-cancel'));
    await waitFor(() => expect(cancelDataSyncMock).toHaveBeenCalledTimes(1));

    resolveCompare([
      { sourceTable: 'users', targetTable: 'users', status: 'MATCHED', rows: [insertRow()] },
    ]);
    await screen.findByTestId('data-sync-objects-step');
    expect(screen.getByTestId('data-sync-window')).toHaveAttribute('data-sync-state', 'unknown');
    expect(screen.getByTestId('data-sync-window')).toHaveAttribute(
      'data-write-outcome-uncertain',
      'true',
    );
    expect(executeDataSyncMock).not.toHaveBeenCalled();
  });
});
