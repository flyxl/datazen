/**
 * Review audit, round 2 — the lifetime of the degradation notice (IF-D-1,
 * fixed in `f114fd426`).
 *
 * The notice says "the new row's primary key could not be located
 * automatically; the page was refreshed". It describes **one commit of one
 * page**, so it has to expire with the page state — and it must **not** expire
 * before the user could see it.
 */
import { describe, expect, it, vi, afterEach, beforeEach } from 'vitest';
import { render, fireEvent, cleanup, screen, waitFor } from '@testing-library/react';
import { TableView } from '../TableView';
import { useTableDataStore } from '../../../stores/tableDataStore';

const mockDatabaseCommands = vi.hoisted(() => ({
  getTableData: vi.fn(),
  previewPendingChanges: vi.fn(),
  commitPendingChanges: vi.fn(),
}));

vi.mock('../../../commands/database', () => ({ databaseCommands: mockDatabaseCommands }));
vi.mock('../../../hooks/useI18n', () => ({ useI18n: () => ({ t: (key: string) => key }) }));
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

const TWO_ROWS = {
  columns: COLUMNS,
  rows: [
    [1, 'Alice'],
    [2, 'Bob'],
  ],
  totalRows: 2,
  page: 0,
  pageSize: 50,
};

const THREE_ROWS = {
  columns: COLUMNS,
  rows: [
    [1, 'Alice'],
    [2, 'Bob'],
    [3, 'Carol'],
  ],
  totalRows: 3,
  page: 0,
  pageSize: 50,
};

const COMMIT_RESPONSE = {
  planId: 'plan-1',
  fingerprint: 'fingerprint-1',
  statements: [{ operation: 'insert', rowIdentity: {}, affectedRows: 1 }],
  affectedRows: 1,
};

afterEach(cleanup);

beforeEach(() => {
  vi.clearAllMocks();
  settingsState.confirmOnDelete = false;
  connectionsState.connections = [];
  useTableDataStore.getState().reset();
  mockDatabaseCommands.getTableData.mockResolvedValue(TWO_ROWS);
  // The plan carries the context it was built for: the store refuses a plan
  // whose table does not match the panel's, so the mock has to follow the
  // request (the panel is switched to another table in one of the cases).
  mockDatabaseCommands.previewPendingChanges.mockImplementation(
    async ({ context }: { context: Record<string, unknown> }) => ({
      planId: 'plan-1',
      fingerprint: 'fingerprint-1',
      table: context,
      updates: [],
      deletes: [],
      inserts: [],
      warnings: [],
    }),
  );
  mockDatabaseCommands.commitPendingChanges.mockResolvedValue(COMMIT_RESPONSE);
});

function renderTable(props: { tableName?: string; database?: string; schema?: string } = {}) {
  return render(
    <TableView
      panelId={PANEL}
      dbSessionId="conn-1"
      database={props.database ?? 'app'}
      schema={props.schema ?? null}
      tableName={props.tableName ?? 'users'}
      connectionId="conn-1"
      databaseType="postgres"
    />,
  );
}

async function waitForTable() {
  await waitFor(() => expect(mockDatabaseCommands.getTableData).toHaveBeenCalled());
}

/** Commit one draft and wait for the commit to have happened. */
async function commitOneDraft() {
  const before = mockDatabaseCommands.commitPendingChanges.mock.calls.length;
  // The commit button is disabled while the panel is loading: wait for an
  // idle panel, otherwise the click would be swallowed.
  await waitFor(() => {
    const ts = useTableDataStore.getState().byPanel.get(PANEL);
    expect(ts?.loading).not.toBe(true);
  });
  fireEvent.click(screen.getByTestId('table-insert-row'));
  await screen.findByTestId('draft-row-0');
  fireEvent.click(screen.getByTestId('pending-commit'));
  await waitFor(() =>
    expect(mockDatabaseCommands.commitPendingChanges.mock.calls.length).toBe(before + 1),
  );
}

function notice(): HTMLElement | null {
  return screen.queryByTestId('table-insert-degraded');
}

describe('B · the degradation notice expires with the page state', () => {
  it('is shown after a commit that could not locate the new row', async () => {
    renderTable();
    await waitForTable();

    await commitOneDraft();

    await waitFor(() => expect(notice()).not.toBeNull());
  });

  it('goes away when the page is fetched again', async () => {
    renderTable();
    await waitForTable();
    await commitOneDraft();
    await waitFor(() => expect(notice()).not.toBeNull());

    fireEvent.click(screen.getByTestId('table-data-refresh'));

    await waitFor(() => expect(notice()).toBeNull());
  });

  it('goes away when the filter changes', async () => {
    renderTable();
    await waitForTable();
    await commitOneDraft();
    await waitFor(() => expect(notice()).not.toBeNull());

    const quickFilter = screen.getByTestId('table-quick-filter');
    fireEvent.change(quickFilter, { target: { value: "name = 'Alice'" } });
    fireEvent.keyDown(quickFilter, { key: 'Enter' });

    await waitFor(() => expect(notice()).toBeNull());
  });

  it('goes away when the staged work is rolled back', async () => {
    renderTable();
    await waitForTable();
    await commitOneDraft();
    await waitFor(() => expect(notice()).not.toBeNull());

    // A new draft brings the pending bar back, and the rollback drops it.
    fireEvent.click(screen.getByTestId('table-insert-row'));
    await screen.findByTestId('draft-row-0');
    fireEvent.click(screen.getByTestId('pending-rollback'));

    await waitFor(() => expect(notice()).toBeNull());
  });

  it('goes away when another table, database or schema is shown', async () => {
    const view = renderTable();
    await waitForTable();
    await commitOneDraft();
    await waitFor(() => expect(notice()).not.toBeNull());

    // Another table.
    view.rerender(
      <TableView
        panelId={PANEL}
        dbSessionId="conn-1"
        database="app"
        schema={null}
        tableName="other_table"
        connectionId="conn-1"
        databaseType="postgres"
      />,
    );
    await waitFor(() => expect(notice()).toBeNull());

    // Another database.
    await commitOneDraft();
    await waitFor(() => expect(notice()).not.toBeNull());
    view.rerender(
      <TableView
        panelId={PANEL}
        dbSessionId="conn-1"
        database="other_db"
        schema={null}
        tableName="users"
        connectionId="conn-1"
        databaseType="postgres"
      />,
    );
    await waitFor(() => expect(notice()).toBeNull());

    // Another schema.
    await commitOneDraft();
    await waitFor(() => expect(notice()).not.toBeNull());
    view.rerender(
      <TableView
        panelId={PANEL}
        dbSessionId="conn-1"
        database="app"
        schema="other_schema"
        tableName="users"
        connectionId="conn-1"
        databaseType="postgres"
      />,
    );
    await waitFor(() => expect(notice()).toBeNull());
  });

  it('is not dropped before the result of the commit is known', async () => {
    renderTable();
    await waitForTable();
    await commitOneDraft();
    await waitFor(() => expect(notice()).not.toBeNull());

    // The next commit is left pending on purpose.
    let resolveCommit: (value: unknown) => void = () => {};
    mockDatabaseCommands.commitPendingChanges.mockImplementationOnce(
      () =>
        new Promise((resolve) => {
          resolveCommit = resolve;
        }),
    );

    fireEvent.click(screen.getByTestId('table-insert-row'));
    await screen.findByTestId('draft-row-0');
    fireEvent.click(screen.getByTestId('pending-commit'));
    await waitFor(() => expect(mockDatabaseCommands.commitPendingChanges).toHaveBeenCalledTimes(2));

    // While the commit is in flight the notice of the previous commit is still
    // there: the user must not lose the information before the new commit
    // decided anything.
    expect(notice()).not.toBeNull();

    // The new commit locates the row (the refreshed page carries a grown
    // primary key) — now the old notice has to go.
    mockDatabaseCommands.getTableData.mockResolvedValue(THREE_ROWS);
    resolveCommit(COMMIT_RESPONSE);

    await waitFor(() => expect(notice()).toBeNull());
  });

  it('shows the degradation of the commit that produced it', async () => {
    renderTable();
    await waitForTable();

    // The very first commit is left pending on purpose: before its result is
    // known there is nothing to say.
    let resolveCommit: (value: unknown) => void = () => {};
    mockDatabaseCommands.commitPendingChanges.mockImplementationOnce(
      () =>
        new Promise((resolve) => {
          resolveCommit = resolve;
        }),
    );

    fireEvent.click(screen.getByTestId('table-insert-row'));
    await screen.findByTestId('draft-row-0');
    fireEvent.click(screen.getByTestId('pending-commit'));
    await waitFor(() => expect(mockDatabaseCommands.commitPendingChanges).toHaveBeenCalled());

    // Nothing is claimed while the commit is in flight …
    expect(notice()).toBeNull();

    // … and the degradation is announced as soon as it is known.
    resolveCommit(COMMIT_RESPONSE);
    await waitFor(() => expect(notice()).not.toBeNull());
  });
});
