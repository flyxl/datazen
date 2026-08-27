import { cleanup, fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import type { ConnectionConfig } from '../../../types';
import type { TransferTableResult } from '../../../commands/transfer';

const { invokeMock, inspectTransferMock, getDatabasesMock, stableT } = vi.hoisted(() => {
  const stableT = (key: string, params?: Record<string, string | number>) =>
    params ? `${key}:${JSON.stringify(params)}` : key;
  return {
    invokeMock: vi.fn(),
    inspectTransferMock: vi.fn(),
    getDatabasesMock: vi.fn(),
    stableT,
  };
});

vi.mock('@tauri-apps/api/core', () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));

vi.mock('../../../hooks/useSettings', () => ({
  useSettings: () => undefined,
}));

vi.mock('../../../stores/settingsStore', () => ({
  useSettingsStore: (sel: (s: { loadSettings: () => void }) => unknown) =>
    sel({ loadSettings: vi.fn() }),
}));

vi.mock('../../../hooks/useI18n', () => ({
  useI18n: () => ({ t: stableT }),
}));

vi.mock('../../../commands/database', () => ({
  databaseCommands: {
    getDatabases: (...args: unknown[]) => getDatabasesMock(...args),
  },
}));

vi.mock('../../../commands/transfer', () => ({
  DEFAULT_TRANSFER_OPTIONS: { batchSize: 500, stopOnError: true, confirmedDestructive: false },
  transferCommands: {
    inspect: (...args: unknown[]) => inspectTransferMock(...args),
    preview: vi.fn().mockResolvedValue({ canExecute: true, ddl: [], writePlans: [], warnings: [] }),
    execute: vi.fn(),
    cancel: vi.fn(),
    classifyPair: vi.fn(),
  },
}));

vi.mock('../../../components/TitleBar', () => ({
  TitleBar: ({ title }: { title?: unknown }) => <div>{String(title ?? '')}</div>,
}));

vi.mock('../../../components/StatusBar', () => ({
  StatusBar: () => <div data-testid="status-bar" />,
}));

vi.mock('../../../components/SqlCodeBlock', () => ({
  SqlCodeBlock: ({ code }: { code: string }) => <pre data-testid="sql-code-block">{code}</pre>,
}));

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

const inspectRows: TransferTableResult[] = [
  {
    sourceTable: 'users',
    targetTable: 'users',
    status: 'MATCHED',
    createNew: false,
    enabled: true,
    sourceColumns: ['id', 'name', 'extra'],
    targetColumns: ['id', 'name', 'email'],
    columnMappings: [
      { sourceColumn: 'id', targetColumn: 'id', skip: false },
      { sourceColumn: 'name', targetColumn: 'name', skip: false },
      { sourceColumn: 'extra', targetColumn: '', skip: true },
    ],
  },
];

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

async function advanceToMappingStep() {
  const { DataTransferWindow } = await import('../DataTransferWindow');
  render(<DataTransferWindow />);

  await waitFor(() => expect(invokeMock).toHaveBeenCalledWith('get_connections'));

  await pickSelect('data-transfer-source', 'PG Src (postgresql)');
  await pickSelect('data-transfer-target', 'PG Tgt (postgresql)');

  await waitFor(() => expect(getDatabasesMock).toHaveBeenCalled());

  fireEvent.click(screen.getByTestId('data-transfer-next'));
  fireEvent.click(screen.getByTestId('data-transfer-mode-data'));

  inspectTransferMock.mockResolvedValue(inspectRows);
  fireEvent.click(screen.getByTestId('data-transfer-next'));

  await waitFor(() => expect(inspectTransferMock).toHaveBeenCalled());
  await waitFor(() => expect(screen.getByTestId('data-transfer-table-row')).toBeTruthy());

  fireEvent.click(screen.getByTestId('data-transfer-next'));

  await waitFor(() => expect(screen.getByTestId('data-transfer-mapping-step')).toBeTruthy());
}

describe('DataTransferWindow', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    getDatabasesMock.mockResolvedValue(['src', 'tgt']);
    invokeMock.mockImplementation(async (cmd: string, args?: Record<string, unknown>) => {
      if (cmd === 'get_connections') return [pgSrc, pgTgt];
      if (cmd === 'connect') return `live-${args?.connectionId as string}`;
      return null;
    });
  });

  afterEach(() => {
    cleanup();
  });

  it('renders wizard shell', async () => {
    const { DataTransferWindow } = await import('../DataTransferWindow');
    render(<DataTransferWindow />);
    expect(screen.getByTestId('data-transfer-window')).toBeTruthy();
    expect(screen.getByTestId('data-transfer-step-endpoints')).toBeTruthy();
    expect(screen.getByTestId('data-transfer-source')).toBeTruthy();
    expect(screen.getByTestId('data-transfer-target')).toBeTruthy();
  });

  it('allows editing column mappings on mapping step', async () => {
    await advanceToMappingStep();

    expect(screen.getByTestId('data-transfer-column-editor')).toBeTruthy();
    expect(screen.getAllByTestId('data-transfer-column-row')).toHaveLength(3);

    fireEvent.click(screen.getByTestId('data-transfer-auto-match'));
    expect(screen.getByTestId('data-transfer-skip-extra')).toBeChecked();
    expect(screen.getByTestId('data-transfer-unmapped-target-warning')).toBeTruthy();

    const emailRow = screen
      .getAllByTestId('data-transfer-column-row')
      .find((row) => within(row).queryByText('extra'));
    expect(emailRow).toBeTruthy();

    const selectWrap = within(emailRow!).getByTestId('data-transfer-target-select-extra');
    const trigger = within(selectWrap).getAllByRole('button')[0];
    fireEvent.click(trigger);
    const list = await waitFor(() => document.getElementById('dz-select-listbox')!);
    const emailOption = Array.from(list.children).find((el) =>
      (el.textContent || '').includes('email'),
    );
    fireEvent.mouseDown(emailOption!);

    expect(screen.getByTestId('data-transfer-skip-extra')).not.toBeChecked();
    expect(screen.queryByTestId('data-transfer-unmapped-target-warning')).toBeNull();
  });
});
