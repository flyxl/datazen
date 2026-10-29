import { cleanup, fireEvent, render, screen } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { toId } from '@datazen/backend-client';
import type { StatementResult } from '../../../../types';
import type { DataExportCapability } from '../../../../lib/exportCapability';
import { ResultTableView } from '../ResultTableView';
import { usePanelResultEdit } from '../../../../stores/panelResultEdit';

const dataTableMock = vi.hoisted(() => ({
  render: vi.fn(),
}));

vi.mock('../../../../hooks/useI18n', () => ({
  useI18n: () => ({
    t: (key: string) => key,
  }),
}));

const settingsState = vi.hoisted(() => ({ queryResultLimit: 5000, safeMode: false }));

vi.mock('../../../../stores/settingsStore', () => ({
  useSettingsStore: (selector: (state: { settings: typeof settingsState }) => unknown) =>
    selector({ settings: settingsState }),
}));

vi.mock('../../../../components/DataTable/DataTable', () => ({
  DataTable: (props: {
    rows: unknown[][];
    exportTableName?: string;
    dataExportCapability?: DataExportCapability;
    onRowClick?: (rowIndex: number) => void;
    onCellDoubleClick?: (rowIndex: number, column: string) => void;
    onCellEdit?: (rowIndex: number, column: string, value: unknown) => void;
    highlightedRow?: number | null;
  }) => {
    dataTableMock.render(props);
    return (
      <div role="grid">
        <button type="button" onClick={() => props.onRowClick?.(0)}>
          select result row
        </button>
        <button type="button" onClick={() => props.onCellDoubleClick?.(0, 'id')}>
          edit result cell
        </button>
        <button type="button" onClick={() => props.onCellEdit?.(0, 'id', 2)}>
          commit result cell
        </button>
        <span>{props.rows.length}</span>
      </div>
    );
  },
}));

function result(rows: StatementResult['rows']): StatementResult {
  return {
    sql: 'select id from users',
    columns: [{ name: 'id', dataType: 'int4', nullable: false }],
    rows,
    executionTimeMs: 2,
  };
}

function writableResult(): StatementResult {
  const executionId = toId('execution-1')!;
  return {
    ...result([[1]]),
    runtimeBinding: {
      executionId,
      resourceBindingId: toId('resource-1')!,
      handle: { dbSessionId: toId('session-1')!, runtimeEpoch: toId('epoch-1')! },
    },
    source: {
      executionId,
      statementIndex: 0,
      writableMapping: 'verified',
      relation: {
        connectionId: toId('connection-1')!,
        namespace: { database: 'app', catalog: null, schema: 'public', path: [] },
        object: { kind: 'table', name: 'users', signature: null },
      },
      context: {
        namespace: { database: 'app', catalog: null, schema: 'public', path: [] },
        searchPath: null,
        effectiveIdentity: null,
        transactionState: 'none',
        autocommit: true,
        confidence: 'confirmed',
      },
    },
  };
}

afterEach(() => {
  cleanup();
  vi.clearAllMocks();
  settingsState.safeMode = false;
  usePanelResultEdit.setState({ verdicts: {}, resolving: {}, errors: {}, generations: {} });
});

describe('ResultTableView', () => {
  it('passes result rows and row detail state to DataTable', () => {
    const onRowDetail = vi.fn();
    render(<ResultTableView result={result([[1]])} rowDetailIndex={0} onRowDetail={onRowDetail} />);

    expect(dataTableMock.render).toHaveBeenCalledWith(
      expect.objectContaining({ rows: [[1]], highlightedRow: 0 }),
    );
    fireEvent.click(screen.getByRole('button', { name: 'select result row' }));
    expect(onRowDetail).toHaveBeenCalledWith(0);
  });

  it('enters editing for a verified result with a runtime binding', () => {
    const onRowDetail = vi.fn();
    render(
      <ResultTableView
        result={writableResult()}
        onCellEdit={vi.fn().mockResolvedValue(undefined)}
        onRowDetail={onRowDetail}
      />,
    );

    fireEvent.click(screen.getByRole('button', { name: 'edit result cell' }));
    expect(onRowDetail).toHaveBeenCalledWith(0);
    expect(dataTableMock.render).toHaveBeenLastCalledWith(
      expect.objectContaining({ editingCell: { row: 0, col: 'id' } }),
    );
  });

  it('keeps verified persisted results read-only until their runtime binding is refreshed', () => {
    const onRowDetail = vi.fn();
    render(
      <ResultTableView
        result={{ ...writableResult(), runtimeBinding: null }}
        onCellEdit={vi.fn().mockResolvedValue(undefined)}
        onRowDetail={onRowDetail}
      />,
    );
    fireEvent.click(screen.getByRole('button', { name: 'edit result cell' }));
    expect(onRowDetail).toHaveBeenCalledWith(0);
    expect(screen.getByText('query.resultEditRequiresRefresh')).toBeInTheDocument();
    expect(dataTableMock.render).toHaveBeenLastCalledWith(
      expect.objectContaining({ editingCell: null }),
    );
  });

  it('CM-36/61 cancels an open editor when the original binding dies and never submits its stale edit', () => {
    const onCellEdit = vi.fn().mockResolvedValue(undefined);
    const live = writableResult();
    const { rerender } = render(<ResultTableView result={live} onCellEdit={onCellEdit} />);
    fireEvent.click(screen.getByRole('button', { name: 'edit result cell' }));
    expect(dataTableMock.render).toHaveBeenLastCalledWith(
      expect.objectContaining({ editingCell: { row: 0, col: 'id' } }),
    );
    // Closing/replacing the original runtime preserves the immutable result,
    // but the open input must stop being a write path immediately.
    rerender(
      <ResultTableView result={{ ...live, runtimeBinding: null }} onCellEdit={onCellEdit} />,
    );
    expect(dataTableMock.render).toHaveBeenLastCalledWith(
      expect.objectContaining({ rows: [[1]], editingCell: null }),
    );
    fireEvent.click(screen.getByRole('button', { name: 'commit result cell' }));
    expect(onCellEdit).not.toHaveBeenCalled();
    expect(screen.getByText('query.resultEditRequiresRefresh')).toBeInTheDocument();
    // A freshly queried result must not resurrect the previously typed edit.
    rerender(
      <ResultTableView
        result={{
          ...live,
          runtimeBinding: {
            ...live.runtimeBinding!,
            executionId: toId('execution-2')!,
          },
        }}
        onCellEdit={onCellEdit}
      />,
    );
    expect(dataTableMock.render).toHaveBeenLastCalledWith(
      expect.objectContaining({ editingCell: null }),
    );
  });

  it('refuses to open an editor when the write-back verdict refuses the result, and says why', () => {
    // The narrow host-side check did not verify the mapping and the driver's
    // origin verdict refuses the result: the grid must not enter edit mode,
    // and it must say why — a read-only grid that says nothing is the
    // "unresponsive grey cell" the contract forbids.
    usePanelResultEdit.setState({
      verdicts: {
        'session-1#0': {
          writable: false,
          identityColumns: [],
          editableColumns: [],
          table: null,
          reason: 'multiTable',
        },
      },
      resolving: {},
      errors: {},
      generations: {},
    });
    const onRowDetail = vi.fn();
    const { container } = render(
      <ResultTableView
        result={result([[1]])}
        writeBackKey="session-1#0"
        onCellEdit={vi.fn().mockResolvedValue(undefined)}
        onRowDetail={onRowDetail}
      />,
    );

    fireEvent.click(screen.getByRole('button', { name: 'edit result cell' }));

    // The row detail still opens; the editor must not.
    expect(onRowDetail).toHaveBeenCalledWith(0);
    expect(dataTableMock.render).toHaveBeenLastCalledWith(
      expect.objectContaining({ editingCell: null }),
    );
    expect(
      container
        .querySelector('[data-dt-grid-readonly-reason]')
        ?.getAttribute('data-dt-grid-readonly-reason'),
    ).toBe('multiTable');
  });

  it('does not enter edit mode for result cells when Safe Mode is on', () => {
    settingsState.safeMode = true;
    const onRowDetail = vi.fn();
    render(
      <ResultTableView
        result={writableResult()}
        onCellEdit={vi.fn().mockResolvedValue(undefined)}
        onRowDetail={onRowDetail}
      />,
    );

    fireEvent.click(screen.getByRole('button', { name: 'edit result cell' }));

    // Row detail still opens, but the cell editor must never activate.
    expect(onRowDetail).toHaveBeenCalledWith(0);
    expect(dataTableMock.render).toHaveBeenLastCalledWith(
      expect.objectContaining({ editingCell: null }),
    );
  });

  it('renders an explicit empty-result status while retaining table adapter semantics', () => {
    render(<ResultTableView result={result([])} />);

    expect(screen.getByRole('grid')).toBeInTheDocument();
    expect(screen.getByText('sqlFile.noResults')).toBeInTheDocument();
    expect(dataTableMock.render).toHaveBeenCalledWith(expect.objectContaining({ rows: [] }));
  });

  it('passes dataExportCapability to DataTable for query result export gating', () => {
    render(<ResultTableView result={result([[1]])} dataExportCapability="none" />);
    expect(dataTableMock.render).toHaveBeenCalledWith(
      expect.objectContaining({ exportTableName: 'query_result', dataExportCapability: 'none' }),
    );
  });

  it('renders ExecutionSummaryCard instead of DataTable for mutation executions (e.g. INSERT)', () => {
    const insertResult: StatementResult = {
      sql: "INSERT INTO users (name) VALUES ('Alice')",
      columns: [],
      rows: [],
      rowsAffected: 1,
      executionTimeMs: 15,
    };
    render(<ResultTableView result={insertResult} />);

    expect(screen.queryByRole('grid')).not.toBeInTheDocument();
    expect(screen.getByText('query.executionSuccess')).toBeInTheDocument();
    expect(
      screen.getAllByText("INSERT INTO users (name) VALUES ('Alice')").length,
    ).toBeGreaterThanOrEqual(1);
  });
});
