import { describe, expect, it, vi, afterEach, beforeEach } from 'vitest';
import { render, fireEvent, cleanup, screen, waitFor } from '@testing-library/react';
import { TableView } from '../TableView';
import { useTableDataStore } from '../../../stores/tableDataStore';

/**
 * The new-row entry points of the table view (04 §5 Step 8).
 *
 * The real store and the real grid are used here: the point of the test is the
 * DOM the user sees and the request the host receives, not the state object.
 */

const mockDatabaseCommands = vi.hoisted(() => ({
  getTableData: vi.fn(),
  previewPendingChanges: vi.fn(),
  commitPendingChanges: vi.fn(),
}));

vi.mock('../../../commands/database', () => ({
  databaseCommands: mockDatabaseCommands,
}));

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

const SAMPLE_PLAN = {
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
};

afterEach(cleanup);

beforeEach(() => {
  vi.clearAllMocks();
  settingsState.confirmOnDelete = false;
  connectionsState.connections = [];
  // The store is a module singleton: every test starts from an empty panel.
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
  mockDatabaseCommands.previewPendingChanges.mockResolvedValue(SAMPLE_PLAN);
  mockDatabaseCommands.commitPendingChanges.mockResolvedValue({
    planId: SAMPLE_PLAN.planId,
    fingerprint: SAMPLE_PLAN.fingerprint,
    statements: [{ operation: 'insert', rowIdentity: {}, affectedRows: 1 }],
    affectedRows: 1,
  });
});

function renderTable(props: { readOnly?: boolean } = {}) {
  return render(
    <TableView
      panelId={PANEL}
      dbSessionId="conn-1"
      database="app"
      tableName="users"
      connectionId="conn-1"
      databaseType="postgres"
      readOnly={props.readOnly}
    />,
  );
}

async function waitForTable() {
  await waitFor(() => expect(mockDatabaseCommands.getTableData).toHaveBeenCalled());
}

describe('TableView (new rows)', () => {
  it('offers the "+ row" entry on an editable connection', async () => {
    renderTable();
    await waitForTable();

    const button = screen.getByTestId('table-insert-row');
    expect(button).toBeInTheDocument();
    // The shortcut is part of the hint, so the user learns it.
    expect(button).toHaveAttribute('title', 'tableData.insertRowHint');
  });

  it('hides the "+ row" entry on a read-only connection', async () => {
    renderTable({ readOnly: true });
    await waitForTable();

    // A missing capability means the entry is hidden, not rendered and
    // rejected afterwards (04 §8, R4).
    expect(screen.queryByTestId('table-insert-row')).toBeNull();
  });

  it('adds a draft row when the entry is used', async () => {
    renderTable();
    await waitForTable();

    fireEvent.click(screen.getByTestId('table-insert-row'));

    // The draft is rendered as a derived row after the real rows.
    const draft = await screen.findByTestId('draft-row-0');
    expect(draft).toHaveAttribute('data-row-kind', 'draft');
    // The real rows are untouched.
    expect(screen.getByText('Alice')).toBeInTheDocument();
  });

  it('adds a draft row with the Cmd/Ctrl+I shortcut', async () => {
    renderTable();
    await waitForTable();

    fireEvent.keyDown(screen.getByTestId('table-insert-row'), { key: 'i', metaKey: true });

    expect(await screen.findByTestId('draft-row-0')).toBeInTheDocument();
  });

  it('ignores the shortcut on a read-only connection', async () => {
    renderTable({ readOnly: true });
    await waitForTable();

    fireEvent.keyDown(screen.getByTestId('table-data-refresh'), { key: 'i', metaKey: true });

    expect(screen.queryByTestId('draft-row-0')).toBeNull();
  });

  it('sends the drafts to the host as insert-marked row changes', async () => {
    renderTable();
    await waitForTable();

    fireEvent.click(screen.getByTestId('table-insert-row'));
    const draftCell = await screen.findByTestId('draft-row-0');
    const nameCell = draftCell.querySelector('[data-draft-col="name"]')!;
    fireEvent.doubleClick(nameCell);
    const input = document.querySelector('input[data-testid="table-edit-input"]')!;
    fireEvent.change(input, { target: { value: 'Carol' } });
    fireEvent.keyDown(input, { key: 'Enter' });

    // The pending bar counts the staged new row.
    expect(await screen.findByTestId('pending-changes-bar')).toHaveTextContent(
      'tableData.pendingInserts',
    );

    fireEvent.click(screen.getByTestId('pending-preview'));
    await waitFor(() => expect(mockDatabaseCommands.previewPendingChanges).toHaveBeenCalled());

    const request = mockDatabaseCommands.previewPendingChanges.mock.calls[0][0];
    // The request shape is unchanged: the drafts ride along in `changes`.
    expect(Object.keys(request).sort()).toEqual(['changes', 'context']);
    expect(request.changes).toHaveLength(1);
    expect(request.changes[0].insertMarked).toBe(true);
    expect(request.changes[0].changedColumns).toEqual(['name']);
    expect(request.changes[0].currentValues).toEqual({ name: 'Carol' });
  });

  it('duplicates the active draft row with the Cmd/Ctrl+D shortcut', async () => {
    renderTable();
    await waitForTable();

    fireEvent.click(screen.getByTestId('table-insert-row'));
    const draft = await screen.findByTestId('draft-row-0');
    // The pointer focus makes the draft the active one.
    fireEvent.click(draft);

    fireEvent.keyDown(draft, { key: 'd', metaKey: true });

    expect(await screen.findByTestId('draft-row-1')).toBeInTheDocument();
  });

  it('discards the active draft row with the Delete key', async () => {
    renderTable();
    await waitForTable();

    fireEvent.click(screen.getByTestId('table-insert-row'));
    const draft = await screen.findByTestId('draft-row-0');
    fireEvent.click(draft);

    fireEvent.keyDown(draft, { key: 'Delete' });

    await waitFor(() => expect(screen.queryByTestId('draft-row-0')).toBeNull());
  });

  it('keeps a filled draft when Escape is pressed and drops the empty ones', async () => {
    renderTable();
    await waitForTable();

    fireEvent.click(screen.getByTestId('table-insert-row'));
    const filled = await screen.findByTestId('draft-row-0');
    const nameCell = filled.querySelector('[data-draft-col="name"]')!;
    fireEvent.doubleClick(nameCell);
    const input = document.querySelector('input[data-testid="table-edit-input"]')!;
    fireEvent.change(input, { target: { value: 'Carol' } });
    fireEvent.keyDown(input, { key: 'Enter' });
    // A second, still empty draft.
    fireEvent.click(screen.getByTestId('table-insert-row'));
    await screen.findByTestId('draft-row-1');

    fireEvent.keyDown(screen.getByTestId('table-insert-row'), { key: 'Escape' });

    // The empty draft is gone, the filled one stays: a row with content is
    // never thrown away by a key press.
    await waitFor(() => expect(screen.queryByTestId('draft-row-1')).toBeNull());
    expect(screen.getByTestId('draft-row-0')).toBeInTheDocument();
  });

  it('says that the new row could not be located instead of claiming a highlight', async () => {
    renderTable();
    await waitForTable();

    fireEvent.click(screen.getByTestId('table-insert-row'));
    await screen.findByTestId('draft-row-0');
    fireEvent.click(screen.getByTestId('pending-commit'));
    await waitFor(() => expect(mockDatabaseCommands.commitPendingChanges).toHaveBeenCalled());

    const notice = await screen.findByTestId('table-insert-degraded');
    expect(notice).toHaveTextContent('tableData.insertReturningUnavailable');
    // The wording is neutral: it is a notice, not an error.
    expect(notice).toHaveAttribute('role', 'status');
  });

  it('drops the notice once a later commit located the new row', async () => {
    renderTable();
    await waitForTable();

    // The first commit cannot locate the new row (the page still holds the
    // old maximum), so the notice appears.
    fireEvent.click(screen.getByTestId('table-insert-row'));
    await screen.findByTestId('draft-row-0');
    fireEvent.click(screen.getByTestId('pending-commit'));
    await waitFor(() => expect(mockDatabaseCommands.commitPendingChanges).toHaveBeenCalled());
    await screen.findByTestId('table-insert-degraded');

    // The second commit **can** locate it: the refreshed page carries a
    // primary key that grew.
    mockDatabaseCommands.getTableData.mockResolvedValue({
      columns: COLUMNS,
      rows: [
        [1, 'Alice'],
        [2, 'Bob'],
        [3, 'Carol'],
      ],
      totalRows: 3,
      page: 0,
      pageSize: 50,
    });
    fireEvent.click(screen.getByTestId('table-insert-row'));
    await screen.findByTestId('draft-row-0');
    fireEvent.click(screen.getByTestId('pending-commit'));
    await waitFor(() => expect(mockDatabaseCommands.commitPendingChanges).toHaveBeenCalledTimes(2));

    // The notice of the first commit is gone: it would contradict the
    // highlight the user is looking at now.
    await waitFor(() => expect(screen.queryByTestId('table-insert-degraded')).toBeNull());
  });

  it('drops the notice when the page state changes', async () => {
    renderTable();
    await waitForTable();

    fireEvent.click(screen.getByTestId('table-insert-row'));
    await screen.findByTestId('draft-row-0');
    fireEvent.click(screen.getByTestId('pending-commit'));
    await waitFor(() => expect(mockDatabaseCommands.commitPendingChanges).toHaveBeenCalled());
    await screen.findByTestId('table-insert-degraded');

    // A new filter is a different set of rows: the notice of the previous
    // commit says nothing about them.
    const quickFilter = screen.getByTestId('table-quick-filter');
    fireEvent.change(quickFilter, { target: { value: "name = 'Alice'" } });
    fireEvent.keyDown(quickFilter, { key: 'Enter' });

    await waitFor(() => expect(screen.queryByTestId('table-insert-degraded')).toBeNull());
  });

  it('drops the notice when the page is reloaded', async () => {
    renderTable();
    await waitForTable();

    fireEvent.click(screen.getByTestId('table-insert-row'));
    await screen.findByTestId('draft-row-0');
    fireEvent.click(screen.getByTestId('pending-commit'));
    await waitFor(() => expect(mockDatabaseCommands.commitPendingChanges).toHaveBeenCalled());
    await screen.findByTestId('table-insert-degraded');

    // A reload replaces the rows the notice was talking about.
    fireEvent.click(screen.getByTestId('table-data-refresh'));

    await waitFor(() => expect(screen.queryByTestId('table-insert-degraded')).toBeNull());
  });
});
