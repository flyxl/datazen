import { describe, expect, it } from 'vitest';
import {
  estimateQueryResultPaneHeight,
  queryResultPaneSizing,
  RESULT_PANE_MIN_HEIGHT,
} from '../queryResultPaneHeight';
import type { StatementResult } from '../../../../types';

function selectResult(rowCount: number): StatementResult {
  return {
    sql: 'SELECT 1',
    columns: [{ name: 'id', dataType: 'int' }],
    rows: Array.from({ length: rowCount }, (_, i) => [i]),
    executionTimeMs: 1,
  };
}

function insertResult(): StatementResult {
  return {
    sql: 'INSERT INTO t VALUES (1)',
    columns: [],
    rows: [],
    rowsAffected: 4,
    executionTimeMs: 1,
  };
}

const base = {
  view: 'table' as const,
  hasResultTabs: false,
  hasError: false,
  showExplain: false,
  showDiagnosis: false,
};

describe('estimateQueryResultPaneHeight', () => {
  it('falls back to the floor before anything ran', () => {
    expect(estimateQueryResultPaneHeight({ ...base, result: null })).toBe(RESULT_PANE_MIN_HEIGHT);
  });

  it('never sizes below the floor, even for a single row', () => {
    // The floor lives in the estimate (not in CSS `min-height`) so it gives way
    // on a panel smaller than itself instead of overflowing it.
    expect(estimateQueryResultPaneHeight({ ...base, result: selectResult(1) })).toBe(
      RESULT_PANE_MIN_HEIGHT,
    );
  });

  it('grows with the number of rows instead of claiming half the panel', () => {
    const threeRows = estimateQueryResultPaneHeight({ ...base, result: selectResult(3) });
    const tenRows = estimateQueryResultPaneHeight({ ...base, result: selectResult(10) });

    expect(threeRows).toBeLessThan(tenRows);
    // A handful of rows must stay far below a half-height pane.
    expect(threeRows).toBeLessThan(300);
  });

  it('accounts for the empty-result placeholder, which is taller than a row', () => {
    const empty = estimateQueryResultPaneHeight({ ...base, result: selectResult(0) });
    const oneRow = estimateQueryResultPaneHeight({ ...base, result: selectResult(1) });

    expect(empty).toBeGreaterThan(oneRow);
  });

  it('reserves a plotting area for the chart view', () => {
    expect(estimateQueryResultPaneHeight({ ...base, view: 'chart', result: selectResult(2) })).toBe(
      estimateQueryResultPaneHeight({ ...base, view: 'chart', result: selectResult(30) }),
    );
  });

  it('uses the summary height for a mutation, whatever the view', () => {
    expect(estimateQueryResultPaneHeight({ ...base, result: insertResult() })).toBe(
      estimateQueryResultPaneHeight({ ...base, view: 'chart', result: insertResult() }),
    );
  });

  it('charges the tab strip and explain/diagnosis surfaces on top', () => {
    const plain = estimateQueryResultPaneHeight({ ...base, result: selectResult(3) });
    const tabbed = estimateQueryResultPaneHeight({
      ...base,
      result: selectResult(3),
      hasResultTabs: true,
    });
    expect(tabbed).toBeGreaterThan(plain);

    expect(
      estimateQueryResultPaneHeight({ ...base, result: null, showExplain: true }),
    ).toBeGreaterThan(RESULT_PANE_MIN_HEIGHT);
    expect(
      estimateQueryResultPaneHeight({ ...base, result: null, showDiagnosis: true }),
    ).toBeGreaterThan(RESULT_PANE_MIN_HEIGHT);
  });

  it('keeps enough room for an error panel, which is not a result', () => {
    const errorHeight = estimateQueryResultPaneHeight({
      ...base,
      result: null,
      hasError: true,
    });
    expect(errorHeight).toBeGreaterThan(RESULT_PANE_MIN_HEIGHT);

    // The diagnosis dock stacks under the error panel, so both must fit.
    expect(
      estimateQueryResultPaneHeight({
        ...base,
        result: null,
        hasError: true,
        showDiagnosis: true,
      }),
    ).toBeGreaterThan(errorHeight);
  });
});

describe('queryResultPaneSizing', () => {
  it('claims the content height, capped at half', () => {
    expect(queryResultPaneSizing(false, 166)).toEqual({ height: 166, maxHeight: '50%' });
  });

  it('lets the editor keep half of the panel even for a tall result', () => {
    // The cap lives in CSS: `max-height: 50%` resolves against the query
    // content area, so a 5000-row estimate can never evict the editor.
    const style = queryResultPaneSizing(false, 5000 * 32);
    expect(style?.maxHeight).toBe('50%');
  });

  it('stops sizing the pane once the user pinned the split', () => {
    expect(queryResultPaneSizing(true, 166)).toBeUndefined();
  });
});
