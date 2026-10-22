import { describe, expect, it } from 'vitest';
import type { ClipboardMatrix } from '../gridClipboard';
import {
  planPaste,
  recordClipboardSource,
  peekClipboardSource,
  clearClipboardSource,
  sourceForClipboardText,
  tableChangeKey,
  type ClipboardSource,
} from '../gridPastePlan';
import type { TableChangeContext } from '../tableChanges';
import type { ColumnSchema } from '../../types';

/**
 * DB-03 §3.2b + G12 — the three alignment layers, as pure logic.
 *
 * Kept in its own file so the cross-table gate can be reviewed and accepted on its own terms:
 * every assertion here is about **which column receives which value** and about whether the
 * user is asked first.
 */

const context: TableChangeContext = {
  connectionId: 'conn-1',
  dbSessionId: 'session-1',
  driverType: 'postgres',
  database: 'app',
  schema: 'public',
  table: 'users',
};

const columns: ColumnSchema[] = [
  { name: 'id', dataType: 'integer', nullable: false, isPrimaryKey: true },
  { name: 'name', dataType: 'text', nullable: true },
  { name: 'email', dataType: 'text', nullable: true },
];
const columnOrder = ['id', 'name', 'email'];

function matrixOf(cells: string[][]): ClipboardMatrix {
  return {
    cells,
    quoted: cells.map((row) => row.map(() => false)),
    delimiter: '\t',
    multiline: cells.length > 1,
    byteLength: cells.flat().join('').length,
  };
}

const source = (columnNames: string[], table = 'users'): ClipboardSource => ({
  tableKey: tableChangeKey({ ...context, table }),
  columnNames,
  columnTypes: columnNames.map(() => 'text'),
});

const base = {
  matrix: matrixOf([['x']]),
  anchor: { rowIndex: 0, columnName: 'name' },
  columnOrder,
  columns,
  region: null,
  pageRowCount: 5,
};

describe('alignment: by name (L2)', () => {
  it('maps each source column onto the same-named target column', () => {
    const plan = planPaste({
      ...base,
      matrix: matrixOf([['a@x', 'Alice']]),
      // The source order differs from the target order on purpose.
      source: source(['email', 'name']),
      context,
    });
    expect(plan.alignment).toBe('byName');
    expect(plan.writes).toEqual([
      { rowIndex: 0, columnName: 'email', value: 'a@x' },
      { rowIndex: 0, columnName: 'name', value: 'Alice' },
    ]);
  });

  it('drops a source column that does not exist here and writes none of its cells', () => {
    const plan = planPaste({
      ...base,
      matrix: matrixOf([['Alice', 'ghost-value']]),
      source: source(['name', 'ghost']),
      context,
    });
    expect(plan.alignment).toBe('byName');
    expect(plan.droppedColumns).toEqual(['ghost']);
    expect(plan.matchedColumns).toBe(1);
    expect(plan.totalColumns).toBe(2);
    expect(plan.writes).toEqual([{ rowIndex: 0, columnName: 'name', value: 'Alice' }]);
    expect(plan.problems.map((problem) => problem.code)).toContain('grid.paste.columnMismatch');
  });

  it('does not shift the remaining columns when one is dropped', () => {
    const plan = planPaste({
      ...base,
      matrix: matrixOf([['ghost', 'Alice', 'a@x']]),
      source: source(['ghost', 'name', 'email']),
      context,
    });
    expect(plan.writes.map((write) => [write.columnName, write.value])).toEqual([
      ['name', 'Alice'],
      ['email', 'a@x'],
    ]);
  });

  it('falls back to position when not one name matches', () => {
    const plan = planPaste({
      ...base,
      matrix: matrixOf([['p', 'q']]),
      source: source(['x', 'y']),
      context,
    });
    expect(plan.alignment).toBe('byPosition');
    expect(plan.writes.map((write) => write.columnName)).toEqual(['name', 'email']);
  });

  it('falls back to position when the recorded width disagrees with the block', () => {
    const plan = planPaste({
      ...base,
      matrix: matrixOf([['p']]),
      source: source(['name', 'email']),
      context,
    });
    expect(plan.alignment).toBe('byPosition');
    expect(plan.writes).toEqual([{ rowIndex: 0, columnName: 'name', value: 'p' }]);
  });
});

describe('confirmation gate (L1 / L3)', () => {
  it('needs no confirmation for a paste inside the same table', () => {
    const plan = planPaste({ ...base, source: source(['name']), context });
    expect(plan.crossTable).toBe(false);
    expect(plan.requiresConfirmation).toBe(false);
  });

  it('forces confirmation across tables', () => {
    const plan = planPaste({ ...base, source: source(['name'], 'orders'), context });
    expect(plan.crossTable).toBe(true);
    expect(plan.requiresConfirmation).toBe(true);
  });

  it('forces confirmation when the source is unknown', () => {
    const plan = planPaste({ ...base, source: null, context });
    expect(plan.crossTable).toBeNull();
    expect(plan.requiresConfirmation).toBe(true);
  });

  it('forces confirmation when the target context is unknown', () => {
    const plan = planPaste({ ...base, source: source(['name']), context: null });
    expect(plan.crossTable).toBeNull();
    expect(plan.requiresConfirmation).toBe(true);
  });

  it('ignores the runtime session id when proving "same table"', () => {
    const plan = planPaste({
      ...base,
      source: source(['name']),
      context: { ...context, dbSessionId: 'another-session' },
    });
    expect(plan.crossTable).toBe(false);
    expect(plan.requiresConfirmation).toBe(false);
  });

  it('separates two tables that differ only by database', () => {
    const plan = planPaste({
      ...base,
      source: source(['name']),
      context: { ...context, database: 'other' },
    });
    expect(plan.crossTable).toBe(true);
  });
});

describe('recorded source', () => {
  it('round-trips through the module-level record', () => {
    clearClipboardSource();
    expect(peekClipboardSource()).toBeNull();
    const recorded = source(['name']);
    recordClipboardSource(recorded);
    expect(peekClipboardSource()).toEqual(recorded);
    clearClipboardSource();
    expect(peekClipboardSource()).toBeNull();
  });

  it('replaces the previous record, so a stale same-table proof cannot survive', () => {
    // The important direction: a copy that knows *less* (or knows another table) must win,
    // because the consumer then falls back to "ask the user" (L3) rather than to a proof it
    // can no longer make. The record may only ever raise the gate.
    clearClipboardSource();
    recordClipboardSource(source(['name', 'email']));
    recordClipboardSource({ tableKey: '', columnNames: [], columnTypes: [] });
    expect(peekClipboardSource()).toEqual({ tableKey: '', columnNames: [], columnTypes: [] });
    clearClipboardSource();
  });
});

describe('the record only counts while the clipboard still holds it', () => {
  it('hands the source back when the clipboard holds exactly what we copied', () => {
    const recorded = source(['name']);
    recordClipboardSource(recorded, 'a\tb');
    expect(sourceForClipboardText('a\tb')).toEqual(recorded);
    clearClipboardSource();
  });

  it('discards the record once another application overwrote the clipboard', () => {
    // The write this prevents: copy a block from `users`, copy a block in Excel, paste into
    // `users`. The stale record still claims "same table", which would waive the
    // confirmation **and** align the Excel block by our column names — a silent write into
    // columns the user never aimed at.
    const recorded = source(['name']);
    recordClipboardSource(recorded, 'a\tb');
    expect(sourceForClipboardText('x\ty')).toBeNull();
    clearClipboardSource();
  });

  it('does not mistake a Windows line-ending rewrite for a changed clipboard', () => {
    // Windows rewrites `\n` to `\r\n` on the way through the clipboard. Comparing raw
    // bytes would put a confirmation in front of every same-table paste on that platform.
    const recorded = source(['name']);
    recordClipboardSource(recorded, 'a\nb');
    expect(sourceForClipboardText('a\r\nb')).toEqual(recorded);
    clearClipboardSource();
  });

  it('stays unusable when the copier could not say what it copied', () => {
    // A caller that records a source without the text leaves us unable to verify the claim,
    // so the paste falls back to "ask" — the safe direction, never a silent write.
    recordClipboardSource(source(['name']));
    expect(sourceForClipboardText('a\tb')).toBeNull();
    clearClipboardSource();
  });

  it('forgets the text when the record is cleared', () => {
    recordClipboardSource(source(['name']), 'a\tb');
    clearClipboardSource();
    expect(sourceForClipboardText('a\tb')).toBeNull();
  });
});
