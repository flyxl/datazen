import { describe, expect, it } from 'vitest';
import {
  computeStageCellValueResult,
  computeStageExpressionValueResult,
  hasPendingColumnChange,
} from '../stageCellValue';
import type { PendingRowChange } from '../../../lib/tableChanges';

/**
 * The staged write expression (DBR-05 3/3): the expression is a **fourth
 * write intent** — not a value, never a bound literal. The kernel decides
 * how it meets the existing three states, because that meeting is the part
 * that can be wrong:
 *
 * - staging it enters `changedColumns` with a placeholder in `currentValues`
 *   (the plan invariant), while `expressionValues` carries the intent;
 * - a literal staged on the same column afterwards **replaces** it — one
 *   write intent per column;
 * - `undefined` (revert) clears both;
 * - the pending question answers `true` for an expression even when the
 *   placeholder happens to equal the original.
 */

const identity = { id: 1 };
const rowIdentity = identity as PendingRowChange['rowIdentity'];

describe('computeStageExpressionValueResult (DBR-05)', () => {
  it('stages the expression: the column is written and the intent is recorded', () => {
    const result = computeStageExpressionValueResult({
      existing: undefined,
      column: 'updated_at',
      originalValue: '2020-01-01 00:00:00',
      rowIdentity,
      rowIndex: 0,
    });
    expect(result.changedColumns).toEqual(['updated_at']);
    expect(result.change?.expressionValues).toEqual({ updated_at: 'currentTimestamp' });
    // The placeholder keeps the plan invariant ("every changed column has a
    // current value"); the expression, not the placeholder, renders the SQL.
    expect(result.change?.currentValues.updated_at).toBe('2020-01-01 00:00:00');
    expect(result.change?.originalValues.updated_at).toBe('2020-01-01 00:00:00');
  });

  it('merges into an existing staged change without touching its other columns', () => {
    const existing: PendingRowChange = {
      rowIdentity,
      originalValues: { name: 'ada' },
      currentValues: { name: 'grace' },
      changedColumns: ['name'],
      deleteMarked: false,
    };
    const result = computeStageExpressionValueResult({
      existing,
      column: 'updated_at',
      originalValue: null,
      rowIdentity,
      rowIndex: 0,
    });
    expect(result.change?.changedColumns).toEqual(['name', 'updated_at']);
    expect(result.change?.currentValues.name).toBe('grace');
    expect(result.change?.expressionValues).toEqual({ updated_at: 'currentTimestamp' });
  });
});

describe('the expression meets the three existing states (DBR-05)', () => {
  const stagedExpression: PendingRowChange = {
    rowIdentity,
    originalValues: { updated_at: '2020-01-01 00:00:00' },
    currentValues: { updated_at: '2020-01-01 00:00:00' },
    changedColumns: ['updated_at'],
    deleteMarked: false,
    expressionValues: { updated_at: 'currentTimestamp' },
  };

  it('a literal staged afterwards replaces the expression — one intent per column', () => {
    const result = computeStageCellValueResult({
      existing: stagedExpression,
      column: 'updated_at',
      value: '2030-06-06 06:06:06',
      originalValue: '2020-01-01 00:00:00',
      rowIdentity,
      rowIndex: 0,
    });
    expect(result.change?.currentValues.updated_at).toBe('2030-06-06 06:06:06');
    expect(result.change?.expressionValues).toBeUndefined();
  });

  it('revert (undefined) clears the expression with the write', () => {
    const result = computeStageCellValueResult({
      existing: stagedExpression,
      column: 'updated_at',
      value: undefined,
      originalValue: '2020-01-01 00:00:00',
      rowIdentity,
      rowIndex: 0,
    });
    // The expression was the only changed column: the row unstages entirely.
    expect(result.change).toBeNull();
  });

  it('a NULL staged afterwards is a value and replaces the expression too', () => {
    const result = computeStageCellValueResult({
      existing: stagedExpression,
      column: 'updated_at',
      value: null,
      originalValue: '2020-01-01 00:00:00',
      rowIdentity,
      rowIndex: 0,
    });
    expect(result.change?.currentValues.updated_at).toBeNull();
    expect(result.change?.expressionValues).toBeUndefined();
  });

  it('the revert of an expression keeps other expressions of the same row', () => {
    const two: PendingRowChange = {
      ...stagedExpression,
      originalValues: { updated_at: 'a', synced_at: 'b' },
      currentValues: { updated_at: 'a', synced_at: 'b' },
      changedColumns: ['updated_at', 'synced_at'],
      expressionValues: {
        updated_at: 'currentTimestamp',
        synced_at: 'currentTimestamp',
      },
    };
    const result = computeStageCellValueResult({
      existing: two,
      column: 'updated_at',
      value: undefined,
      originalValue: 'a',
      rowIdentity,
      rowIndex: 0,
    });
    expect(result.change?.expressionValues).toEqual({ synced_at: 'currentTimestamp' });
  });
});

describe('hasPendingColumnChange with a staged expression (DBR-05)', () => {
  it('answers true even when the placeholder equals the original (NULL → expression)', () => {
    const existing: PendingRowChange = {
      rowIdentity,
      originalValues: { updated_at: null },
      currentValues: { updated_at: null },
      changedColumns: ['updated_at'],
      deleteMarked: false,
      expressionValues: { updated_at: 'currentTimestamp' },
    };
    // Without the expression check, `null → null` would read as "unchanged"
    // and the revert entry would never appear for a staged expression.
    expect(hasPendingColumnChange(existing, 'updated_at')).toBe(true);
  });
});
