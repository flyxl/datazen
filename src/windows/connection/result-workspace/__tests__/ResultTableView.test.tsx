import { cleanup, fireEvent, render, screen } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';
import type { StatementResult } from '../../../types';
import type { DataExportCapability } from '../../../lib/exportCapability';
import { ResultTableView } from '../ResultTableView';

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

afterEach(() => {
  cleanup();
  vi.clearAllMocks();
  settingsState.safeMode = false;
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

  it('keeps cell editing local and reports the row detail before editing', () => {
    const onRowDetail = vi.fn();
    render(<ResultTableView result={result([[1]])} onRowDetail={onRowDetail} />);

    fireEvent.click(screen.getByRole('button', { name: 'edit result cell' }));
    expect(onRowDetail).toHaveBeenCalledWith(0);
    expect(dataTableMock.render).toHaveBeenLastCalledWith(
      expect.objectContaining({ editingCell: { row: 0, col: 'id' } }),
    );
  });

  it('does not enter edit mode for result cells when Safe Mode is on', () => {
    settingsState.safeMode = true;
    const onRowDetail = vi.fn();
    render(<ResultTableView result={result([[1]])} onRowDetail={onRowDetail} />);

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
});
