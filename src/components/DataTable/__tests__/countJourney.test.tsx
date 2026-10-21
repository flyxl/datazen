import { describe, expect, it, vi, beforeEach, afterEach } from 'vitest';
import { act, cleanup, fireEvent, render, waitFor } from '@testing-library/react';
import { useEffect } from 'react';
import { Pagination } from '../Pagination';
import { useTableActions } from '../../../windows/connection/useTableActions';
import { useTableDataStore } from '../../../stores/tableDataStore';
import { useSchemaStore } from '../../../stores/schemaStore';
import { createEmptyConnectionSchema } from '../../../stores/schemaStoreState';
import { useSettingsStore } from '../../../stores/settingsStore';
import type { TableChangeContext } from '../../../lib/tableChanges';

/**
 * The count journey, from the setting to the screen.
 *
 * The three states are not three pictures of a number — they are three
 * different conversations with the back end:
 *
 *   setting → read → "≈" → the user asks for the truth → the truth.
 *
 * And one more: when the database cannot say how many rows it has, the grid
 * has to say *that* — a "0 rows" would be a number nobody counted, and it
 * would pin the user to the first page forever.
 */
const { mockDatabaseCommands } = vi.hoisted(() => ({
  mockDatabaseCommands: { getTableData: vi.fn() },
}));

vi.mock('../../../lib/session/tableSession', () => ({ tableManualHandle: vi.fn(() => null) }));

vi.mock('../../../commands/database', () => ({
  databaseCommands: mockDatabaseCommands,
}));

vi.mock('../../ui/Select', () => ({
  Select: ({
    value,
    options,
    onChange,
    disabled,
  }: {
    value: string | number;
    options: { value: string; label: string }[];
    onChange: (v: string) => void;
    disabled?: boolean;
  }) => (
    <select disabled={disabled} value={String(value)} onChange={(e) => onChange(e.target.value)}>
      {options.map((o) => (
        <option key={o.value} value={o.value}>{o.label}</option>
      ))}
    </select>
  ),
}));

const PANEL = 'panel-users';

const context: TableChangeContext = {
  connectionId: 'conn-1',
  dbSessionId: 'conn-1',
  driverType: 'postgres',
  database: 'app',
  schema: null,
  table: 'users',
};

const columns = [
  { name: 'id', dataType: 'integer', isPrimaryKey: true, isNullable: false },
  { name: 'name', dataType: 'text', isPrimaryKey: false, isNullable: true },
];

function pageRows(count: number): (string | number | null)[][] {
  return Array.from({ length: count }, (_, index) => [index + 1, `row-${index + 1}`]);
}

const invalidateNotice = () => {};

/**
 * The grid of the journey: the panel's pager, wired to the store — and the
 * panel's read, driven by the same setting the user changed.
 */
function Grid() {
  const ts = useTableDataStore((s) => s.byPanel.get(PANEL));
  const actions = useTableActions({ panelId: PANEL, context, invalidateNotice });
  const needsLoad = !ts || ts.rows.length === 0;

  useEffect(() => {
    if (!needsLoad) return;
    actions.load();
  }, [actions, needsLoad]);

  if (!ts) return null;
  return (
    <Pagination
      page={ts.page}
      pageSize={ts.pageSize}
      totalRows={ts.totalRows}
      totalRowsKind={ts.totalRowsKind}
      rowCount={ts.rows.length}
      hasMore={ts.rows.length === ts.pageSize}
      onPageChange={actions.setPage}
      onPageSizeChange={actions.setPageSize}
      onCountExact={actions.countExact}
    />
  );
}

describe('the row count journey', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    useTableDataStore.getState().reset();
    // The catalog knows this table is huge, so the automatic mode estimates
    // instead of counting: the threshold decides, not the panel.
    useSchemaStore.setState((state) => {
      const schemas = new Map(state.schemas);
      schemas.set(
        'conn-1',
        Object.assign(createEmptyConnectionSchema(), {
          tables: [
            {
              name: 'users',
              schema: null,
              tableType: 'table' as const,
              rowCount: 5_000_000,
            },
          ],
        }),
      );
      return { schemas };
    });
  });

  afterEach(cleanup);

  it('goes from the setting to an estimated count and then to the exact one', async () => {
    // ⓪ The user asked for the automatic mode.
    useSettingsStore.setState((state) => ({
      settings: { ...state.settings, rowCountMode: 'auto' },
    }));

    // ① The read asks for an estimate: the table is above the threshold.
    mockDatabaseCommands.getTableData.mockResolvedValueOnce({
      columns,
      rows: pageRows(50),
      totalRows: 1_200_000,
      totalRowsKind: 'estimated',
      page: 0,
      pageSize: 50,
    });

    const view = render(<Grid />);
    await waitFor(() => {
      expect(mockDatabaseCommands.getTableData).toHaveBeenCalled();
    });
    const request = mockDatabaseCommands.getTableData.mock.calls.at(-1)?.[0];
    expect(request.countStrategy).toBe('estimated');

    // ② The grid shows the estimate as an estimate.
    await waitFor(() => {
      expect(view.getByTestId('pagination-row-count').textContent).toBe('1-50 / ≈ 1.2M');
    });
    expect(view.getByTestId('pagination-count-exact')).toBeInTheDocument();

    // ③ The user asks for the truth — and the truth is not what the estimate
    // said, which is exactly why the estimate was marked as one.
    mockDatabaseCommands.getTableData.mockResolvedValueOnce({
      columns,
      rows: pageRows(50),
      totalRows: 987_654,
      totalRowsKind: 'exact',
      page: 0,
      pageSize: 50,
    });
    await act(async () => {
      fireEvent.click(view.getByTestId('pagination-count-exact'));
    });

    const exactRequest = mockDatabaseCommands.getTableData.mock.calls.at(-1)?.[0];
    expect(exactRequest.countStrategy).toBe('exact');
    // The exact count is a full re-read, not a paged one: the count has to run.
    expect(exactRequest.skipCount).toBeFalsy();

    // ④ The truth replaces the estimate — and the "about" sign is gone.
    await waitFor(() => {
      expect(view.getByTestId('pagination-row-count').textContent).toBe('1-50 / 987,654');
    });
    expect(view.getByTestId('pagination-row-count').textContent).not.toContain('≈');
    expect(view.queryByTestId('pagination-count-exact')).toBeNull();
  });

  it('says that the count is unavailable instead of showing zero rows', async () => {
    // ⓪ The user asked for no count at all.
    useSettingsStore.setState((state) => ({
      settings: { ...state.settings, rowCountMode: 'none' },
    }));

    // ① The read asks for no count — and the database has none to give.
    mockDatabaseCommands.getTableData.mockResolvedValueOnce({
      columns,
      rows: pageRows(50),
      totalRows: null,
      totalRowsKind: 'unsupported',
      page: 0,
      pageSize: 50,
    });

    const view = render(<Grid />);
    await waitFor(() => {
      expect(mockDatabaseCommands.getTableData).toHaveBeenCalled();
    });
    const request = mockDatabaseCommands.getTableData.mock.calls.at(-1)?.[0];
    expect(request.countStrategy).toBe('unsupported');

    // ② The grid says so, instead of showing a "0 rows" nobody counted.
    await waitFor(() => {
      expect(view.getByTestId('pagination-row-count').textContent).toBe('1-50 / —');
    });
    expect(view.getByTestId('pagination-row-count').textContent).not.toContain('0 rows');
    // No total, so no total number of pages either.
    expect(view.queryByText('Page 1 / 1')).toBeNull();
    // And the user is not stuck on the first page: the page is full.
    expect(view.getByLabelText('Next')).not.toBeDisabled();
  });
});
