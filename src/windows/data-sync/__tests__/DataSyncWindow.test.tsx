import { describe, expect, it, vi, beforeEach, afterEach } from 'vitest';
import { render, cleanup, fireEvent, screen, waitFor, within } from '@testing-library/react';
import type { ConnectionConfig } from '../../../types';

const { invokeMock, inspectDataSyncMock, getSyncTasksMock, stableT } = vi.hoisted(() => {
  const stableT = (key: string, params?: Record<string, string | number>) =>
    params ? `${key}:${JSON.stringify(params)}` : key;
  return {
    invokeMock: vi.fn(),
    inspectDataSyncMock: vi.fn(),
    getSyncTasksMock: vi.fn(),
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
    getSyncTasks: (...args: unknown[]) => getSyncTasksMock(...args),
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
  const trigger = within(wrap).getByRole('button');
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
    expect(inspectDataSyncMock).not.toHaveBeenCalled();
  });

  it('inspects same-family connections and keeps Apply disabled', async () => {
    inspectDataSyncMock.mockResolvedValue([
      { sourceTable: 'users', targetTable: 'users', status: 'MATCHED' },
      { sourceTable: 'orders', targetTable: '', status: 'UNMAPPED_SOURCE' },
    ]);
    render(<DataSyncWindow />);
    await waitFor(() => expect(invokeMock).toHaveBeenCalledWith('get_connections'));
    await pickSelect('data-sync-source', 'PG Src');
    await pickSelect('data-sync-target', 'PG Tgt');
    fireEvent.click(screen.getByTestId('data-sync-compare'));
    await waitFor(() =>
      expect(inspectDataSyncMock).toHaveBeenCalledWith('live-pg-src', 'live-pg-tgt'),
    );
    const rows = await screen.findAllByTestId('data-sync-mapping-row');
    expect(rows).toHaveLength(2);
    expect(screen.getByText('sync.mappingMatched')).toBeTruthy();
    expect(screen.getByText('sync.mappingUnmappedSource')).toBeTruthy();
    const apply = screen.getByTestId('data-sync-start-disabled');
    expect(apply).toBeDisabled();
    expect(apply).toHaveAttribute('title', 'sync.applyUnavailable');
  });

  it('marks heterogeneous targets as unsupported', async () => {
    render(<DataSyncWindow />);
    await waitFor(() => expect(invokeMock).toHaveBeenCalledWith('get_connections'));
    await pickSelect('data-sync-source', 'PG Src');
    const wrap = screen.getByTestId('data-sync-target');
    fireEvent.click(within(wrap).getByRole('button'));
    const list = await waitFor(() => {
      const el = document.getElementById('dz-select-listbox');
      if (!el) throw new Error('dz-select-listbox not open');
      return el;
    });
    const mysql = Array.from(list.children).find((el) => (el.textContent || '').includes('My Tgt'));
    expect(mysql?.textContent).toContain('sync.unsupportedHint');
  });

  it('shows inspect errors and connect failures', async () => {
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
    fireEvent.click(screen.getByTestId('data-sync-compare'));
    expect(await screen.findByTestId('data-sync-error')).toHaveTextContent('sync.connectFailed');
    fireEvent.click(screen.getByText('common.ok'));

    invokeMock.mockImplementation(async (cmd: string, args?: { configId?: string }) => {
      if (cmd === 'get_connections') return [pgSrc, pgTgt, mysqlTgt];
      if (cmd === 'connect') return `live-${args?.configId}`;
      return null;
    });
    inspectDataSyncMock.mockRejectedValue(new Error('gate failed'));
    fireEvent.click(screen.getByTestId('data-sync-compare'));
    expect(await screen.findByTestId('data-sync-error')).toHaveTextContent('gate failed');
  });

  it('toggles MATCHED rows and select-all / deselect-all', async () => {
    inspectDataSyncMock.mockResolvedValue([
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
