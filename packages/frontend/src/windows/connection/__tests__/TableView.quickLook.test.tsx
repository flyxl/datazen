/**
 * The grid's Quick Look entries (DBR-05): the middle-click on a cell and the
 * `Cmd/Ctrl+Shift+L` shortcut, through the **production wiring** — TableView →
 * DataTable → VirtualBody → GridCell — because the defect this guards against
 * is exactly a layer forgetting to thread the callback (the same reasoning as
 * the filter-operator journey test).
 *
 * The shortcut is a *read* gesture: a read-only panel keeps the viewer, only
 * the dialog's hand-typed save exit needs the write path.
 */
import { describe, expect, it, vi, afterEach, beforeEach } from 'vitest';
import { cleanup, fireEvent, render, screen, within } from '@testing-library/react';
import { TableView } from '../TableView';
import { emptyTableState } from '../../../stores/tableData/connectionState';
import type { TableState } from '../../../stores/tableData/types';
import type { TableChangeContext } from '../../../lib/tableChanges';

vi.mock('../../../hooks/useI18n', () => ({
  useI18n: () => ({ t: (key: string) => key }),
}));

vi.mock('../../../hooks/useConfirmDialog', () => ({
  useConfirmDialog: () => [vi.fn().mockResolvedValue(false), null],
}));

const settingsState = vi.hoisted(() => ({ confirmOnDelete: false, safeMode: false }));
const connectionsState = vi.hoisted(() => ({
  connections: [] as { id: string; name: string; readOnly?: boolean }[],
}));

vi.mock('../../../stores/settingsStore', () => ({
  useSettingsStore: (sel: (s: { settings: typeof settingsState }) => unknown) =>
    sel({ settings: settingsState }),
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

const registryState = vi.hoisted(() => ({ registry: {} as Record<string, unknown> }));

vi.mock('../../../lib/databaseTypes', () => ({
  get DB_REGISTRY() {
    return registryState.registry;
  },
  escapeIdent: (ident: string) => ident,
}));

const tableController = vi.hoisted(() => ({
  snapshot: { state: 'ready' },
  refresh: vi
    .fn()
    .mockResolvedValue({ state: 'ready', observedContext: { transactionState: 'none' } }),
  execute: vi.fn().mockResolvedValue({ executionId: 'table-tx' }),
  getExecution: vi.fn().mockResolvedValue({ state: 'succeeded' }),
}));
vi.mock('../../../lib/session/tableSession', () => ({
  tableSessionController: vi.fn().mockResolvedValue(tableController),
}));

vi.mock('../../../commands/database', () => ({
  databaseCommands: {
    useDatabase: vi.fn().mockResolvedValue(undefined),
  },
}));

vi.mock('../../../components/ai/NlFilterInput', () => ({
  NlFilterInput: () => <div data-testid="mock-nl-filter" />,
}));

vi.mock('../../../hooks/useColumnResize', () => ({
  useColumnResize: () => ({ columnWidths: [160, 160], onResizeStart: vi.fn() }),
  adjustWidthsForSort: (widths: number[]) => widths,
}));

vi.mock('../../../hooks/useVirtualTable', () => ({
  useVirtualTable: ({ rows }: { rows: unknown[][] }) => ({
    virtualRows: rows.map((_, index) => ({ index, key: String(index), start: index * 40 })),
    totalHeight: rows.length * 40,
  }),
}));

const tableStore = vi.hoisted(() => ({
  byPanel: new Map<string, TableState>(),
  loadTableData: vi.fn(),
  reloadPanel: vi.fn(),
  setSort: vi.fn(),
  removeFilter: vi.fn(),
  clearFilters: vi.fn(),
  addFilter: vi.fn(),
  setFilters: vi.fn(),
  updateFilter: vi.fn(),
  setFilterLogic: vi.fn(),
  applyFilters: vi.fn(),
  setFilterPanelOpen: vi.fn(),
  setVisibleColumns: vi.fn(),
  setPage: vi.fn(),
  setPageSize: vi.fn(),
  startEdit: vi.fn(),
  stageCellChange: vi.fn(),
  stageCellValue: vi.fn(),
  cancelEdit: vi.fn(),
  selectRow: vi.fn(),
  toggleSelectAll: vi.fn(),
  deleteRows: vi.fn(),
  previewPendingChanges: vi.fn().mockResolvedValue(null),
  commitPendingChanges: vi.fn().mockResolvedValue({}),
  rollbackPendingChanges: vi.fn(),
  setDetailRow: vi.fn(),
}));

vi.mock('../../../stores/tableDataStore', () => ({
  useTableDataStore: Object.assign((sel: (s: typeof tableStore) => unknown) => sel(tableStore), {
    getState: () => tableStore,
  }),
}));

const PANEL = 'panel-quick-look';

/** A 200-character bio: long enough that a Quick Look is worth its dialog. */
const LONG_TEXT = 'a'.repeat(200);

function contextOf(): TableChangeContext {
  return {
    connectionId: null,
    dbSessionId: 'c1',
    driverType: 'postgresql',
    database: 'app',
    schema: null,
    table: 'people',
  };
}

function seedPanel(withFocus: boolean): void {
  tableStore.byPanel.set(PANEL, {
    ...emptyTableState(contextOf()),
    columns: [
      { name: 'name', dataType: 'text', nullable: true, isPrimaryKey: true },
      { name: 'bio', dataType: 'text', nullable: true },
    ],
    rows: [{ name: 'ada', bio: LONG_TEXT }],
    gridSelection: withFocus
      ? {
          mode: 'cell',
          anchor: { rowIndex: 0, columnName: 'bio' },
          focus: { rowIndex: 0, columnName: 'bio' },
          extraRanges: [],
        }
      : undefined,
  } as TableState);
}

function renderTable(readOnly = false) {
  return render(
    <TableView
      panelId={PANEL}
      dbSessionId="c1"
      database="app"
      tableName="people"
      databaseType="postgresql"
      readOnly={readOnly}
    />,
  );
}

/** The grid cell of the seeded bio value. */
function bioCell(): HTMLElement {
  const cell = document.querySelector('[data-dt-row="0"][data-dt-col="bio"]');
  expect(cell).not.toBeNull();
  return cell as HTMLElement;
}

afterEach(cleanup);

beforeEach(() => {
  vi.clearAllMocks();
  settingsState.confirmOnDelete = false;
  settingsState.safeMode = false;
  connectionsState.connections = [];
  tableStore.byPanel = new Map();
});

describe("the grid's Quick Look entries (DBR-05)", () => {
  it('Cmd/Ctrl+Shift+L opens the quick look of the focused cell', () => {
    seedPanel(true);
    renderTable();

    fireEvent.keyDown(bioCell(), { key: 'l', metaKey: true, shiftKey: true });

    // The dialog of the focused cell: an editable long text opens its editor
    // with the cell's own value in it.
    const dialog = screen.getByRole('dialog');
    const editor = within(dialog).getByRole('textbox') as HTMLTextAreaElement;
    expect(editor.value).toBe(LONG_TEXT);
  });

  it('ignores Cmd/Ctrl+Shift+L when no cell is focused', () => {
    seedPanel(false);
    renderTable();

    fireEvent.keyDown(bioCell(), { key: 'l', metaKey: true, shiftKey: true });

    expect(screen.queryByRole('dialog')).toBeNull();
  });

  it('middle-click on a cell opens its quick look', () => {
    // No cell selection state at all: the entry is the gesture, not a mode.
    seedPanel(false);
    renderTable();

    fireEvent.mouseDown(bioCell(), { button: 1 });

    const dialog = screen.getByRole('dialog');
    const editor = within(dialog).getByRole('textbox') as HTMLTextAreaElement;
    expect(editor.value).toBe(LONG_TEXT);
  });

  it('keeps the left mouse button out of the quick look', () => {
    seedPanel(false);
    renderTable();

    fireEvent.mouseDown(bioCell(), { button: 0 });

    expect(screen.queryByRole('dialog')).toBeNull();
  });

  it('opens the viewer on a read-only panel, without the hand-typed save exit', () => {
    seedPanel(true);
    renderTable(true);

    fireEvent.keyDown(bioCell(), { key: 'l', metaKey: true, shiftKey: true });

    // The read-only long text renders its content, and the save button — the
    // exit that stages a typed value — is not offered.
    const dialog = screen.getByRole('dialog');
    expect(within(dialog).getByText(LONG_TEXT)).toBeInTheDocument();
    expect(within(dialog).queryByRole('button', { name: 'detail.save' })).toBeNull();
  });
});
