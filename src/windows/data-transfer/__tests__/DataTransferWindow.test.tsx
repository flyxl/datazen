import { cleanup, fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import type { ConnectionConfig } from '../../../types';
import type { TransferTableResult } from '../../../commands/transfer';
import { transferCommands } from '../../../commands/transfer';
import { clearTransferLimitationsDismissed } from '../../../lib/transferLimitationsPrefs';

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

vi.mock('@tauri-apps/api/event', () => ({
  listen: vi.fn().mockResolvedValue(() => {}),
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

vi.mock('../../../hooks/useLocaleDomains', () => ({
  useLocaleDomains: () => true,
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
    preview: vi.fn().mockResolvedValue({
      canExecute: true,
      ddl: [],
      writePlans: [
        {
          sourceTable: 'users',
          targetTable: 'users',
          writeMode: 'truncateInsert',
          mappedColumns: [],
          preamble: [],
          estimatedRows: 3,
        },
      ],
      warnings: [],
      pairingPath: 'direct',
      mode: 'data',
      writeMode: 'truncateInsert',
    }),
    execute: vi.fn().mockResolvedValue({ rowsInserted: 3, tables: [] }),
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
    return screen.getByRole('listbox');
  });
  const option = Array.from(list.children).find((el) =>
    (el.textContent || '').includes(optionLabel),
  );
  expect(option, `option ${optionLabel}`).toBeTruthy();
  fireEvent.mouseDown(option!);
}

async function dismissLimitationsDialog() {
  await waitFor(() => {
    expect(screen.getByTestId('data-transfer-limitations')).toBeTruthy();
  });
  fireEvent.click(screen.getByTestId('data-transfer-limitations-close'));
  await waitFor(() => {
    expect(screen.queryByTestId('data-transfer-limitations')).toBeNull();
  });
}

async function advanceToMappingStep() {
  const { DataTransferWindow } = await import('../DataTransferWindow');
  render(<DataTransferWindow />);

  await waitFor(() => expect(invokeMock).toHaveBeenCalledWith('get_connections'));

  await dismissLimitationsDialog();

  await pickSelect('data-transfer-source', 'PG Src (postgresql)');
  await pickSelect('data-transfer-target', 'PG Tgt (postgresql)');

  await waitFor(() => expect(getDatabasesMock).toHaveBeenCalled());

  // endpoints → setup
  fireEvent.click(screen.getByTestId('data-transfer-next'));
  await waitFor(() => expect(screen.getByTestId('data-transfer-mode-data')).toBeTruthy());

  fireEvent.click(screen.getByTestId('data-transfer-mode-data'));

  inspectTransferMock.mockResolvedValue(inspectRows);
  // setup → objects
  fireEvent.click(screen.getByTestId('data-transfer-next'));

  await waitFor(() => expect(inspectTransferMock).toHaveBeenCalled());
  await waitFor(() => expect(screen.getByTestId('data-transfer-table-row')).toBeTruthy());

  // objects → mapping
  fireEvent.click(screen.getByTestId('data-transfer-next'));

  await waitFor(() => expect(screen.getByTestId('data-transfer-mapping-step')).toBeTruthy());
}

async function advanceToPreviewStep(writeMode: 'insert' | 'truncateInsert' = 'truncateInsert') {
  const { DataTransferWindow } = await import('../DataTransferWindow');
  render(<DataTransferWindow />);

  await waitFor(() => expect(invokeMock).toHaveBeenCalledWith('get_connections'));
  await dismissLimitationsDialog();

  await pickSelect('data-transfer-source', 'PG Src (postgresql)');
  await pickSelect('data-transfer-target', 'PG Tgt (postgresql)');
  await waitFor(() => expect(getDatabasesMock).toHaveBeenCalled());

  fireEvent.click(screen.getByTestId('data-transfer-next'));
  await waitFor(() => expect(screen.getByTestId('data-transfer-mode-data')).toBeTruthy());
  fireEvent.click(screen.getByTestId('data-transfer-mode-data'));

  if (writeMode !== 'insert') {
    await waitFor(() => expect(screen.getByTestId('data-transfer-write-mode')).toBeTruthy());
    await pickSelect('data-transfer-write-mode', 'transfer.writeMode.truncateInsert');
    fireEvent.click(screen.getByTestId('data-transfer-destructive-confirm'));
  }

  inspectTransferMock.mockResolvedValue(inspectRows);
  fireEvent.click(screen.getByTestId('data-transfer-next'));
  await waitFor(() => expect(screen.getByTestId('data-transfer-table-row')).toBeTruthy());
  fireEvent.click(screen.getByTestId('data-transfer-next'));
  await waitFor(() => expect(screen.getByTestId('data-transfer-mapping-step')).toBeTruthy());
  fireEvent.click(screen.getByTestId('data-transfer-next'));
  await waitFor(() => expect(screen.getByTestId('data-transfer-preview')).toBeTruthy());
}

describe('DataTransferWindow', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    clearTransferLimitationsDismissed();
    getDatabasesMock.mockResolvedValue(['src', 'tgt']);
    invokeMock.mockImplementation(async (cmd: string, args?: Record<string, unknown>) => {
      if (cmd === 'get_connections') return [pgSrc, pgTgt];
      if (cmd === 'connect_dedicated') {
        const conn = args?.connectionId as string;
        const db = (args?.database as string | null | undefined) ?? 'default';
        return `dedicated-${conn}-${db}`;
      }
      if (cmd === 'release_connection') return false;
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
    expect(screen.getByTestId('data-transfer-step-setup')).toBeTruthy();
    expect(screen.getByTestId('data-transfer-source')).toBeTruthy();
    expect(screen.getByTestId('data-transfer-target')).toBeTruthy();
  });

  it('opens limitations dialog on first visit', async () => {
    const { DataTransferWindow } = await import('../DataTransferWindow');
    render(<DataTransferWindow />);

    await waitFor(() => {
      expect(screen.getByTestId('data-transfer-limitations')).toBeTruthy();
      expect(screen.getByTestId('data-transfer-limitations-close')).toBeTruthy();
    });
  });

  it('does not reopen limitations dialog after dontShowAgain is checked', async () => {
    const { DataTransferWindow } = await import('../DataTransferWindow');
    const { unmount } = render(<DataTransferWindow />);

    await waitFor(() => {
      expect(screen.getByTestId('data-transfer-limitations')).toBeTruthy();
    });

    fireEvent.click(screen.getByTestId('data-transfer-limitations-dismiss'));
    fireEvent.click(screen.getByTestId('data-transfer-limitations-close'));

    await waitFor(() => {
      expect(screen.queryByTestId('data-transfer-limitations')).toBeNull();
    });

    unmount();
    cleanup();

    render(<DataTransferWindow />);
    await waitFor(() => expect(invokeMock).toHaveBeenCalledWith('get_connections'));

    expect(screen.queryByTestId('data-transfer-limitations')).toBeNull();
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
    const list = await waitFor(() => screen.getByRole('listbox'));
    const emailOption = Array.from(list.children).find((el) =>
      (el.textContent || '').includes('email'),
    );
    fireEvent.mouseDown(emailOption!);

    expect(screen.getByTestId('data-transfer-skip-extra')).not.toBeChecked();
    expect(screen.queryByTestId('data-transfer-unmapped-target-warning')).toBeNull();
  });

  it('shows execute confirm dialog for destructive write mode before running', async () => {
    await advanceToPreviewStep('truncateInsert');

    fireEvent.click(screen.getByTestId('data-transfer-execute'));
    await waitFor(() => {
      expect(screen.getByTestId('data-transfer-execute-confirm')).toBeTruthy();
      expect(screen.getByTestId('data-transfer-execute-confirm-table-users')).toBeTruthy();
    });
    expect(transferCommands.execute).not.toHaveBeenCalled();

    fireEvent.click(screen.getByTestId('data-transfer-execute-confirm-proceed'));
    await waitFor(() => expect(transferCommands.execute).toHaveBeenCalled());
  });

  it('runs execute immediately for insert write mode without confirm dialog', async () => {
    await advanceToPreviewStep('insert');

    fireEvent.click(screen.getByTestId('data-transfer-execute'));
    await waitFor(() => expect(transferCommands.execute).toHaveBeenCalled());
    expect(screen.queryByTestId('data-transfer-execute-confirm')).toBeNull();
  });
});
