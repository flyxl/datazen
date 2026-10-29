/**
 * Review audit of the insert-frontend track — the read-only capability gate
 * (04 §5 Step 8, 00-contracts R4) and the wording of the degradation notice.
 *
 * The cases below are deliberately **paired**: the same key press is dispatched
 * on an editable panel and on a read-only one, so the read-only case cannot
 * pass for the wrong reason (a missing handler, a swallowed event, a target
 * that swallows the key).
 */
import { describe, expect, it, vi, afterEach, beforeEach } from 'vitest';
import { render, fireEvent, cleanup, screen, waitFor } from '@testing-library/react';
import { TableView } from '../TableView';
import { useTableDataStore } from '../../../stores/tableDataStore';
import enQuery from '../../../locales/en/query';

const mockDatabaseCommands = vi.hoisted(() => ({
  getTableData: vi.fn(),
  previewPendingChanges: vi.fn(),
  commitPendingChanges: vi.fn(),
}));

vi.mock('../../../commands/database', () => ({ databaseCommands: mockDatabaseCommands }));

vi.mock('../../../hooks/useI18n', () => ({
  useI18n: () => ({ t: (key: string) => key }),
}));

vi.mock('../../../hooks/useConfirmDialog', () => ({
  useConfirmDialog: () => [vi.fn().mockResolvedValue(true), null],
}));

vi.mock('../../../hooks/useVirtualTable', () => ({
  useVirtualTable: ({ rows }: { rows: unknown[][] }) => ({
    virtualRows: rows.map((_, index) => ({ index, key: String(index), start: index * 40 })),
    totalHeight: rows.length * 40,
  }),
}));

vi.mock('../../../components/ai/NlFilterInput', () => ({
  NlFilterInput: () => <div data-testid="mock-nl-filter" />,
}));

const tableController = vi.hoisted(() => ({
  snapshot: { state: 'ready' },
  refresh: vi.fn().mockResolvedValue({ state: 'ready', observedContext: { transactionState: 'none' } }),
  execute: vi.fn().mockResolvedValue({ executionId: 'table-tx' }),
  getExecution: vi.fn().mockResolvedValue({ state: 'succeeded' }),
}));
vi.mock('../../../lib/session/tableSession', () => ({
  tableSessionController: vi.fn().mockResolvedValue(tableController),
  tableManualHandle: vi.fn(() => null),
}));

const settingsState = vi.hoisted(() => ({
  confirmOnDelete: false,
  safeMode: false,
  defaultPageSize: 50,
}));
vi.mock('../../../stores/settingsStore', () => ({
  useSettingsStore: Object.assign(
    (sel: (s: { settings: typeof settingsState }) => unknown) => sel({ settings: settingsState }),
    { getState: () => ({ settings: settingsState }) },
  ),
}));

const connectionsState = vi.hoisted(() => ({
  connections: [] as { id: string; name: string; readOnly?: boolean }[],
}));
vi.mock('../../../stores/connectionStore', async (importOriginal) => {
  const actual = await importOriginal<typeof import('../../../stores/connectionStore')>();
  return {
    ...actual,
    useConnectionStore: Object.assign(
      (selector?: (s: typeof connectionsState) => unknown) =>
        selector ? selector(connectionsState) : connectionsState,
      {
        getState: () => connectionsState,
        setState: (partial: Partial<typeof connectionsState>) =>
          Object.assign(connectionsState, partial),
      },
    ),
  };
});

const PANEL = 'panel-users';

const COLUMNS = [
  { name: 'id', dataType: 'integer', nullable: false, isPrimaryKey: true },
  { name: 'name', dataType: 'text', nullable: true },
];

afterEach(cleanup);

beforeEach(() => {
  vi.clearAllMocks();
  settingsState.confirmOnDelete = false;
  connectionsState.connections = [];
  useTableDataStore.getState().reset();
  mockDatabaseCommands.getTableData.mockResolvedValue({
    columns: COLUMNS,
    rows: [
      [1, 'Alice'],
      [2, 'Bob'],
    ],
    totalRows: 2,
    page: 0,
    pageSize: 50,
  });
  mockDatabaseCommands.previewPendingChanges.mockResolvedValue({
    planId: 'plan-1',
    fingerprint: 'fingerprint-1',
    table: {
      connectionId: 'conn-1',
      dbSessionId: 'conn-1',
      driverType: 'postgres',
      database: 'app',
      schema: null,
      table: 'users',
    },
    updates: [],
    deletes: [],
    inserts: [],
    warnings: [],
  });
  mockDatabaseCommands.commitPendingChanges.mockResolvedValue({
    planId: 'plan-1',
    fingerprint: 'fingerprint-1',
    statements: [{ operation: 'insert', rowIdentity: {}, affectedRows: 1 }],
    affectedRows: 1,
  });
});

function renderTable(readOnly: boolean) {
  return render(
    <TableView
      panelId={PANEL}
      dbSessionId="conn-1"
      database="app"
      tableName="users"
      connectionId="conn-1"
      databaseType="postgres"
      readOnly={readOnly}
    />,
  );
}

async function waitForTable() {
  await waitFor(() => expect(mockDatabaseCommands.getTableData).toHaveBeenCalled());
}

/** A cell of the grid: the key press is dispatched on the grid itself. */
function gridCell(): HTMLElement {
  const cell = document.querySelector('[data-testid="data-table-cell"]');
  if (!cell) throw new Error('the grid did not render a cell');
  return cell as HTMLElement;
}

describe('C · a read-only connection has no insert capability at all', () => {
  it('hides the "+ row" entry', async () => {
    renderTable(true);
    await waitForTable();

    expect(screen.queryByTestId('table-insert-row')).toBeNull();
  });

  it('ignores the Cmd/Ctrl+I shortcut', async () => {
    renderTable(true);
    await waitForTable();

    // The very same dispatch on an editable panel creates a draft row (the
    // paired case below), so this one cannot pass for the wrong reason.
    fireEvent.keyDown(gridCell(), { key: 'i', metaKey: true });
    fireEvent.keyDown(gridCell(), { key: 'i', ctrlKey: true });

    await waitFor(() => expect(screen.queryByTestId('draft-row-0')).toBeNull());
    expect(screen.queryAllByTestId(/^draft-row-/)).toHaveLength(0);
    // Nothing was staged either.
    const panel = useTableDataStore.getState().byPanel.get(PANEL);
    expect(panel?.draftInserts.size ?? 0).toBe(0);
  });

  it('creates a draft row with the same shortcut on an editable panel', async () => {
    renderTable(false);
    await waitForTable();

    expect(screen.getByTestId('table-insert-row')).toBeInTheDocument();
    fireEvent.keyDown(gridCell(), { key: 'i', metaKey: true });

    expect(await screen.findByTestId('draft-row-0')).toBeInTheDocument();
  });
});

describe('D2 · the degradation notice is neutral, word for word', () => {
  it('does not claim that the new row was highlighted or located', () => {
    const message = enQuery['tableData.insertReturningUnavailable'];

    expect(typeof message).toBe('string');
    expect(message.length).toBeGreaterThan(0);
    // The forbidden claims, in any casing.
    expect(message).not.toMatch(/highlight/i);
    expect(message).not.toMatch(/select(ed)?\b/i);
    expect(message).not.toMatch(/has been located/i);
    expect(message).not.toMatch(/successfully located/i);
    expect(message).not.toMatch(/the new row is/i);
    // It says the opposite: the key could not be located and the page was
    // refreshed.
    expect(message).toMatch(/could not be located/i);
    expect(message).toMatch(/refreshed/i);
  });

  it('shows the notice as a status, not as an error', async () => {
    renderTable(false);
    await waitForTable();

    fireEvent.click(screen.getByTestId('table-insert-row'));
    await screen.findByTestId('draft-row-0');
    fireEvent.click(screen.getByTestId('pending-commit'));
    await waitFor(() => expect(mockDatabaseCommands.commitPendingChanges).toHaveBeenCalled());

    const notice = await screen.findByTestId('table-insert-degraded');
    expect(notice).toHaveAttribute('role', 'status');
    expect(notice).toHaveAttribute('aria-live', 'polite');
    // The notice is not styled as an error: it uses the muted foreground, not
    // the warning / danger colour of the error banner.
    const className = notice.getAttribute('class') ?? '';
    expect(className).toContain('text-fg-muted');
    expect(className).not.toContain('text-danger');
    expect(className).not.toContain('text-warning');
    expect(className).not.toContain('text-red');
  });
});
