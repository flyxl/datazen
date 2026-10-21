/**
 * The pure staged-edit computations (DB-06 §5 Step 3).
 *
 * These are the rules the dirty marks depend on, extracted from the store
 * action so they can be tested without a store:
 *
 * - "the user reverted the cell to its original value → the row is clean again
 *   and its key leaves `pendingChanges`";
 * - "the user reverted one of several changed cells → only that cell is
 *   unstaged";
 * - "undoing the delete mark of a row that also had cell changes → the row
 *   becomes dirty again, with its changes intact".
 */
import { describe, expect, it } from 'vitest';
import { computeStageResult, computeUnstageDeleteResult } from '../stageEdit';
import type { PendingRowChange } from '../../../lib/tableChanges';

function existing(overrides: Partial<PendingRowChange> = {}): PendingRowChange {
  return {
    rowIdentity: { id: 1 },
    originalValues: {},
    currentValues: {},
    changedColumns: [],
    deleteMarked: false,
    ...overrides,
  };
}

describe('computeStageResult', () => {
  it('stages a value that differs from the original', () => {
    const result = computeStageResult({
      existing: undefined,
      column: 'name',
      originalValue: 'Alice',
      currentValue: 'Alicia',
      rowIdentity: { id: 1 },
      rowIndex: 0,
    });

    expect(result.change).toEqual({
      rowIndex: 0,
      rowIdentity: { id: 1 },
      originalValues: { name: 'Alice' },
      currentValues: { name: 'Alicia' },
      changedColumns: ['name'],
      deleteMarked: false,
    });
  });

  it('unstages the row when the only changed cell is reverted', () => {
    const result = computeStageResult({
      existing: existing({
        originalValues: { name: 'Alice' },
        currentValues: { name: 'Alicia' },
        changedColumns: ['name'],
      }),
      column: 'name',
      originalValue: 'Alice',
      currentValue: 'Alice',
      rowIdentity: { id: 1 },
      rowIndex: 0,
    });

    expect(result.change).toBeNull();
    expect(result.changedColumns).toEqual([]);
  });

  it('unstages only the reverted cell and keeps the others', () => {
    const result = computeStageResult({
      existing: existing({
        originalValues: { name: 'Alice', score: 10 },
        currentValues: { name: 'Alicia', score: 11 },
        changedColumns: ['name', 'score'],
      }),
      column: 'name',
      originalValue: 'Alice',
      currentValue: 'Alice',
      rowIdentity: { id: 1 },
      rowIndex: 0,
    });

    expect(result.change?.changedColumns).toEqual(['score']);
    // The reverted cell leaves the snapshots entirely: it is not staged any
    // more, so it is not part of the change either.
    expect(result.change?.originalValues).toEqual({ score: 10 });
    expect(result.change?.currentValues).toEqual({ score: 11 });
  });

  it('keeps the entry of a row that is marked for deletion, even when it is clean', () => {
    const result = computeStageResult({
      existing: existing({ deleteMarked: true }),
      column: 'name',
      originalValue: 'Alice',
      currentValue: 'Alice',
      rowIdentity: { id: 1 },
      rowIndex: 0,
    });

    expect(result.change).not.toBeNull();
    expect(result.change?.deleteMarked).toBe(true);
    expect(result.change?.changedColumns).toEqual([]);
  });

  it('treats SQL NULL as a value that can be changed', () => {
    const result = computeStageResult({
      existing: undefined,
      column: 'name',
      originalValue: null,
      currentValue: 'Alice',
      rowIdentity: { id: 1 },
      rowIndex: 0,
    });

    expect(result.change?.originalValues).toEqual({ name: null });
    expect(result.change?.currentValues).toEqual({ name: 'Alice' });
  });

  it('treats the empty string and NULL as different values', () => {
    const result = computeStageResult({
      existing: undefined,
      column: 'name',
      originalValue: null,
      currentValue: '',
      rowIdentity: { id: 1 },
      rowIndex: 0,
    });

    expect(result.change?.changedColumns).toEqual(['name']);
  });

  it('does not stage the same column twice', () => {
    const result = computeStageResult({
      existing: existing({
        originalValues: { name: 'Alice' },
        currentValues: { name: 'Alicia' },
        changedColumns: ['name'],
      }),
      column: 'name',
      originalValue: 'Alice',
      currentValue: 'Alicia 2',
      rowIdentity: { id: 1 },
      rowIndex: 0,
    });

    expect(result.change?.changedColumns).toEqual(['name']);
    expect(result.change?.currentValues).toEqual({ name: 'Alicia 2' });
  });
});

describe('computeUnstageDeleteResult', () => {
  it('drops a row that was only marked for deletion', () => {
    expect(computeUnstageDeleteResult(existing({ deleteMarked: true }))).toBeNull();
  });

  it('keeps the cell changes of a row that also had them', () => {
    const change = existing({
      deleteMarked: true,
      originalValues: { name: 'Alice' },
      currentValues: { name: 'Alicia' },
      changedColumns: ['name'],
    });

    const result = computeUnstageDeleteResult(change);
    expect(result?.deleteMarked).toBe(false);
    expect(result?.changedColumns).toEqual(['name']);
    expect(result?.currentValues).toEqual({ name: 'Alicia' });
  });
});
