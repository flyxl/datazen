import { describe, expect, it, vi, beforeEach, afterEach } from 'vitest';
import { render, cleanup, fireEvent, screen, waitFor, within } from '@testing-library/react';
import type { ConnectionConfig } from '../../../types';

const {
  invokeMock,
  inspectDataSyncMock,
  compareDataSyncMock,
  applyDataSyncMock,
  cancelDataSyncMock,
  getSyncTasksMock,
  getDatabasesMock,
  stableT,
} = vi.hoisted(() => {
  const stableT = (key: string, params?: Record<string, string | number>) =>
    params ? `${key}:${JSON.stringify(params)}` : key;
  return {
    invokeMock: vi.fn(),
    inspectDataSyncMock: vi.fn(),
    compareDataSyncMock: vi.fn(),
    applyDataSyncMock: vi.fn(),
    cancelDataSyncMock: vi.fn().mockResolvedValue(true),
    getSyncTasksMock: vi.fn(),
    getDatabasesMock: vi.fn(),
    stableT,
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
    inspectDataSync: (...args: unknown[]) => inspectDataSyncMock(...args),
    compareDataSync: (...args: unknown[]) => compareDataSyncMock(...args),
    applyDataSync: (...args: unknown[]) => applyDataSyncMock(...args),
    cancelDataSync: (...args: unknown[]) => cancelDataSyncMock(...args),
    getSyncTasks: (...args: unknown[]) => getSyncTasksMock(...args),
  },
}));

vi.mock('../../../commands/database', () => ({
  databaseCommands: {
    getDatabases: (...args: unknown[]) => getDatabasesMock(...args),
  },
}));

vi.mock('../../../components/TitleBar', () => ({
  TitleBar: ({ title }: { title?: unknown }) => (
    <div data-testid="title-bar">{String(title ?? '')}</div>
  ),
}));

vi.mock('../../../components/StatusBar', () => ({
  StatusBar: ({ left }: { left?: unknown }) => <div data-testid="status-bar">{left as never}</div>,
}));

vi.mock('../../../components/schema/SchemaDiffPanel', () => ({
  SchemaDiffPanel: () => null,
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

describe('DataSyncWindow (F9 Diff Workspace shell)', () => {
  beforeEach(() => {
    invokeMock.mockReset();
    inspectDataSyncMock.mockReset();
    getSyncTasksMock.mockReset();
    getSyncTasksMock.mockResolvedValue([]);
    getDatabasesMock.mockReset();
    getDatabasesMock.mockImplementation(async (connId: string) =>
      connId.includes('pg-src') || connId.includes('my') ? ['src', 'other'] : ['tgt'],
    );
    invokeMock.mockImplementation(async (cmd: string, args?: { configId?: string }) => {
      if (cmd === 'get_connections') return [pgSrc, pgTgt, mysqlTgt];
      if (cmd === 'connect') return `live-${args?.configId}`;
      return null;
    });
  });

  afterEach(() => {
    cleanup();
  });

  it('shows the overwrite-retired banner and idle prompt', async () => {
    render(<DataSyncWindow />);
    await waitFor(() => expect(invokeMock).toHaveBeenCalledWith('get_connections'));
    const banner = screen.getByTestId('data-sync-overwrite-retired');
    expect(banner).toHaveTextContent('sync.overwriteRetiredBanner');
    expect(screen.getByText('sync.selectPrompt')).toBeTruthy();
    expect(screen.getByTestId('data-sync-compare')).toBeTruthy();
    expect(screen.queryByTestId('data-sync-start-disabled')).toBeNull();
  });

  it('prompts to select both endpoints when Compare is clicked empty', async () => {
    render(<DataSyncWindow />);
    await waitFor(() => expect(invokeMock).toHaveBeenCalledWith('get_connections'));
    fireEvent.click(screen.getByTestId('data-sync-compare'));
    expect(await screen.findByTestId('data-sync-error')).toHaveTextContent('sync.selectBoth');
    expect(compareDataSyncMock).not.toHaveBeenCalled();
  });

  it('compares same-family connections and enables Apply when row diffs exist', async () => {
    compareDataSyncMock.mockResolvedValue([
      {
        sourceTable: 'users',
        targetTable: 'users',
        status: 'MATCHED',
        rows: [{ operation: 'INSERT' }],
      },
      { sourceTable: 'orders', targetTable: '', status: 'UNMAPPED_SOURCE' },
    ]);
    applyDataSyncMock.mockResolvedValue({ applied: 1, rolledBack: false });
    render(<DataSyncWindow />);
    await waitFor(() => expect(invokeMock).toHaveBeenCalledWith('get_connections'));
    await pickSelect('data-sync-source', 'PG Src');
    await pickSelect('data-sync-target', 'PG Tgt');
    // Both connections auto-connect to enumerate databases; default DBs are selected.
    await waitFor(() =>
      expect(screen.getByTestId('data-sync-source-database')).toHaveTextContent('src'),
    );
    fireEvent.click(screen.getByTestId('data-sync-compare'));
    await waitFor(() =>
      expect(compareDataSyncMock).toHaveBeenCalledWith(
        'live-pg-src',
        'live-pg-tgt',
        [],
        expect.any(String),
        'src',
        'tgt',
      ),
    );
    const rows = await screen.findAllByTestId('data-sync-mapping-row');
    expect(rows).toHaveLength(2);
    expect(screen.getByText(/sync.rowDiffs/)).toBeTruthy();
    expect(screen.getByText('sync.mappingUnmappedSource')).toBeTruthy();
    const apply = screen.getByTestId('data-sync-start');
    expect(apply).not.toBeDisabled();
    fireEvent.click(apply);
    await waitFor(() =>
      expect(applyDataSyncMock).toHaveBeenCalledWith(
        'live-pg-src',
        'live-pg-tgt',
        ['users'],
        expect.any(String),
        'src',
        'tgt',
      ),
    );
    // Drain the post-apply re-compare so it does not leak into the next test.
    await waitFor(() =>
      expect(compareDataSyncMock).toHaveBeenLastCalledWith(
        'live-pg-src',
        'live-pg-tgt',
        ['users'],
        expect.any(String),
        'src',
        'tgt',
      ),
    );
  });

  it('marks heterogeneous targets as unsupported', async () => {
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
    expect(mysql?.textContent).toContain('sync.unsupportedHint');
  });

  it('gates compare when a database cannot be enumerated', async () => {
    invokeMock.mockImplementation(async (cmd: string, args?: { configId?: string }) => {
      if (cmd === 'get_connections') return [pgSrc, pgTgt, mysqlTgt];
      if (cmd === 'connect') {
        if (args?.configId === 'pg-tgt') throw new Error('refused');
        return `live-${args?.configId}`;
      }
      return null;
    });
    render(<DataSyncWindow />);
    await waitFor(() => expect(invokeMock).toHaveBeenCalledWith('get_connections'));
    await pickSelect('data-sync-source', 'PG Src');
    await pickSelect('data-sync-target', 'PG Tgt');
    // The target database list failed to enumerate, so compare stays gated.
    fireEvent.click(screen.getByTestId('data-sync-compare'));
    expect(await screen.findByTestId('data-sync-error')).toHaveTextContent('sync.selectDbRequired');
  });

  it('surfaces data-sync comparison errors', async () => {
    invokeMock.mockImplementation(async (cmd: string, args?: { configId?: string }) => {
      if (cmd === 'get_connections') return [pgSrc, pgTgt, mysqlTgt];
      if (cmd === 'connect') return `live-${args?.configId}`;
      return null;
    });
    render(<DataSyncWindow />);
    await waitFor(() => expect(invokeMock).toHaveBeenCalledWith('get_connections'));
    await pickSelect('data-sync-source', 'PG Src');
    await pickSelect('data-sync-target', 'PG Tgt');
    await waitFor(() =>
      expect(screen.getByTestId('data-sync-source-database')).toHaveTextContent('src'),
    );
    compareDataSyncMock.mockRejectedValue(new Error('gate failed'));
    fireEvent.click(screen.getByTestId('data-sync-compare'));
    expect(await screen.findByTestId('data-sync-error')).toHaveTextContent('gate failed');
  });

  it('toggles MATCHED rows and select-all / deselect-all', async () => {
    compareDataSyncMock.mockResolvedValue([
      { sourceTable: 'users', targetTable: 'users', status: 'MATCHED' },
      {
        sourceTable: 'legacy',
        targetTable: 'legacy',
        status: 'INCOMPATIBLE',
        incompatibleReason: 'pk mismatch',
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
    await screen.findAllByTestId('data-sync-mapping-row');
    expect(screen.getByTestId('data-sync-path')).toHaveTextContent('sync.pathDirect');
    expect(screen.getByText('pk mismatch')).toBeTruthy();
    expect(screen.getByTestId('data-sync-selected').textContent).toContain('"selected":1');

    const matchedRow = screen.getAllByTestId('data-sync-mapping-row')[0];
    fireEvent.click(within(matchedRow).getByRole('checkbox'));
    expect(screen.getByTestId('data-sync-selected').textContent).toContain('"selected":0');
    fireEvent.click(screen.getByTestId('data-sync-select-all'));
    expect(screen.getByTestId('data-sync-selected').textContent).toContain('"selected":1');
    fireEvent.click(screen.getByTestId('data-sync-deselect-all'));
    expect(screen.getByTestId('data-sync-selected').textContent).toContain('"selected":0');
  });
});
