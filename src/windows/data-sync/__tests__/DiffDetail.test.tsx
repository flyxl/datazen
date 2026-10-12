import { useState } from 'react';
import { cleanup, fireEvent, render, screen, within } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { DiffDetail } from '../DiffDetail';
import { applyOptionsToRows } from '../mappingView';
import type { DataSyncRowChange, DataSyncTableResult } from '../../../commands/sync';

vi.mock('../../../hooks/useI18n', () => ({ useI18n: () => ({ t: (key: string) => key }) }));
afterEach(cleanup);
const options = { insert: true, update: true, delete: false };
const changedRow = (id: number): DataSyncRowChange => ({
  operation: 'UPDATE',
  key: [id],
  sourceRow: [id, 'new', 20],
  targetRow: [id, 'old', 20],
  changedColumns: ['name'],
  selected: true,
});

function Journey({ count = 1, pageSize }: { count?: number; pageSize?: number }) {
  const [rows, setRows] = useState(Array.from({ length: count }, (_, i) => changedRow(i)));
  const table: DataSyncTableResult = {
    sourceTable: 'source',
    targetTable: 'target',
    status: 'MATCHED',
    columns: ['id', 'name', 'age'],
    rows,
  };
  return <DiffDetail table={table} options={options} onUpdateRows={setRows} pageSize={pageSize} />;
}

function ScopedJourney() {
  const [rows, setRows] = useState([changedRow(1), changedRow(2)]);
  const [scoped, setScoped] = useState(false);
  const [excluded, setExcluded] = useState<number[]>([]);
  return (
    <>
      <DiffDetail
        table={{
          sourceTable: 'source',
          targetTable: 'target',
          status: 'MATCHED',
          columns: ['id', 'name'],
          rows,
        }}
        options={{ insert: true, update: true, delete: false }}
        onUpdateRows={(next) => {
          setRows(next);
          if (scoped)
            setExcluded(next.filter((row) => !row.selected).map((row) => row.key[0] as number));
        }}
        onSelectAllOperation={() => {
          setScoped(true);
          setRows((current) => current.map((row) => ({ ...row, selected: true })));
        }}
        onClearAllOperation={() => setScoped(false)}
        isOperationSelected={() => scoped}
      />
      <output data-testid="scope-state">{`${scoped}:${excluded.join(',')}`}</output>
    </>
  );
}

describe('diff review journeys', () => {
  it('uses real column names and highlights only changed fields with source as new value', () => {
    const { container } = render(<Journey />);
    expect(screen.getByRole('columnheader', { name: 'name' })).toBeTruthy();
    expect(container.querySelector('td[data-column="name"]')).toHaveAttribute(
      'data-changed',
      'true',
    );
    expect(container.querySelector('td[data-column="age"]')).toHaveAttribute(
      'data-changed',
      'false',
    );
    expect(screen.getByText('sync.sourceShort: new')).not.toHaveClass('line-through');
    expect(screen.getByText('sync.targetShort: old')).toHaveClass('line-through');
  });

  // count 必须恒等于 pageSize + 1:断言第 2 页只有 1 个 checkbox,且对它用单数
  // getByRole('checkbox'),都依赖第 2 页恰好剩 1 行。页大小取 6 而非模块默认的 500,
  // 是因为本地分页的页大小原本没有注入口,只能堆 501 行凑出第二页——那一次渲染在
  // jsdom 里实测 38.5s,是 20s 预算的 1.93 倍,足以让并发下的整套前端测试超时变红。
  it('keeps cross-page row deselection when navigating in both directions', () => {
    render(<Journey count={7} pageSize={6} />);
    const review = screen.getByTestId('data-sync-row-diff');
    fireEvent.click(within(review).getAllByRole('checkbox')[0]);
    fireEvent.click(screen.getByText('sync.pageNext'));
    expect(within(review).getAllByRole('checkbox')).toHaveLength(1);
    fireEvent.click(within(review).getByRole('checkbox'));
    fireEvent.click(screen.getByText('sync.pagePrev'));
    expect(within(review).getAllByRole('checkbox')[0]).not.toBeChecked();
    expect(within(review).getAllByRole('checkbox')[1]).toBeChecked();
    fireEvent.click(screen.getByText('sync.pageNext'));
    expect(within(review).getByRole('checkbox')).not.toBeChecked();
  });

  it('option changes never reselect manually excluded rows', () => {
    const rows = [{ ...changedRow(1), selected: false }, changedRow(2)];
    const disabled = applyOptionsToRows(rows, { ...options, update: false });
    expect(disabled.every((row) => !row.selected)).toBe(true);
    expect(applyOptionsToRows(disabled, options).every((row) => !row.selected)).toBe(true);
    expect(applyOptionsToRows(rows, options).map((row) => row.selected)).toEqual([false, true]);
  });

  it('selects a table scope and records a row exclusion without collecting row payloads', () => {
    render(<ScopedJourney />);
    fireEvent.click(screen.getByTestId('data-sync-select-all-UPDATE'));
    expect(screen.getByTestId('scope-state')).toHaveTextContent('true:');
    const checkboxes = within(screen.getByTestId('data-sync-row-diff')).getAllByRole('checkbox');
    fireEvent.click(checkboxes[0]);
    expect(screen.getByTestId('scope-state')).toHaveTextContent('true:1');
    expect(screen.getByTestId('data-sync-clear-all-UPDATE')).toBeTruthy();
  });

  it('does not also select explicit rows when a table-scope handler is provided', () => {
    const onUpdateRows = vi.fn();
    const onSelectAllOperation = vi.fn();
    render(
      <DiffDetail
        table={{
          sourceTable: 'source',
          targetTable: 'target',
          status: 'MATCHED',
          columns: ['id', 'name'],
          rows: [changedRow(1)],
        }}
        options={{ insert: true, update: true, delete: false }}
        onUpdateRows={onUpdateRows}
        onSelectAllOperation={onSelectAllOperation}
      />,
    );

    fireEvent.click(screen.getByTestId('data-sync-select-all-UPDATE'));

    expect(onSelectAllOperation).toHaveBeenCalledTimes(1);
    expect(onUpdateRows).not.toHaveBeenCalled();
  });
});
