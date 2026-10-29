/**
 * The in-result find and replace rules (DB-14).
 *
 * These pin the four facts that keep the bar honest: the filter keeps the
 * rows that match in the addressed column (or any), "does not contain" keeps
 * the NULL cells (they contain nothing), a replacement writes **inside** the
 * matching cells (the rest of a cell's text survives), and the plan speaks
 * the statement's own coordinates — never the filtered view's.
 */
import { describe, expect, it } from 'vitest';
import {
  cellMatches,
  filterResultRows,
  nextPlanItem,
  planResultReplacements,
  rowMatches,
  type ResultFindQuery,
} from '../resultFindReplace';

const COLUMNS = ['id', 'name', 'email'];

function query(overrides: Partial<ResultFindQuery> = {}): ResultFindQuery {
  return { needle: 'acme', column: null, operator: 'contains', matchCase: false, ...overrides };
}

describe('cellMatches', () => {
  it('finds the needle anywhere, ignoring case by default', () => {
    expect(cellMatches('Acme Corp', query())).toBe(true);
    expect(cellMatches('ACME', query())).toBe(true);
    expect(cellMatches('acme', query())).toBe(true);
    expect(cellMatches('corp', query())).toBe(false);
  });

  it('honours the case when the user asks for it', () => {
    expect(cellMatches('Acme', query({ matchCase: true }))).toBe(false);
    expect(cellMatches('acme', query({ matchCase: true }))).toBe(true);
  });

  it('answers every operator', () => {
    expect(cellMatches('acme corp', query({ operator: 'prefix' }))).toBe(true);
    expect(cellMatches('corp acme', query({ operator: 'prefix' }))).toBe(false);
    expect(cellMatches('corp acme', query({ operator: 'suffix' }))).toBe(true);
    expect(cellMatches('acme corp', query({ operator: 'suffix' }))).toBe(false);
    expect(cellMatches('corp', query({ operator: 'notContains' }))).toBe(true);
    expect(cellMatches('acme corp', query({ operator: 'notContains' }))).toBe(false);
  });

  it('treats a NULL as containing nothing — and answers "not contains" for it', () => {
    expect(cellMatches(null, query())).toBe(false);
    expect(cellMatches(null, query({ operator: 'notContains' }))).toBe(true);
    expect(cellMatches(null, query({ operator: 'prefix' }))).toBe(false);
  });

  it('matches numbers by their text form', () => {
    expect(cellMatches(42, query({ needle: '42' }))).toBe(true);
  });

  it('matches nothing for an empty needle', () => {
    expect(cellMatches('anything', query({ needle: '' }))).toBe(false);
  });
});

describe('rowMatches and filterResultRows', () => {
  const rows: (string | number | null)[][] = [
    [1, 'acme corp', 'a@acme.io'],
    [2, 'beta llc', 'b@beta.io'],
    [3, null, null],
  ];

  it('keeps the rows that match in any column', () => {
    expect(filterResultRows(rows, COLUMNS, query())).toEqual([0]);
  });

  it('keeps the NULL row for "does not contain"', () => {
    // A NULL does not contain the needle — losing it would read as "the
    // filter ate my data".
    expect(filterResultRows(rows, COLUMNS, query({ operator: 'notContains' }))).toEqual([1, 2]);
  });

  it('looks only in the addressed column when one is named', () => {
    expect(filterResultRows(rows, COLUMNS, query({ column: 'name' }))).toEqual([0]);
    expect(filterResultRows(rows, COLUMNS, query({ column: 'email', needle: 'beta' }))).toEqual([
      1,
    ]);
  });

  it('keeps every row for an empty needle — opening the bar must not blank the grid', () => {
    expect(filterResultRows(rows, COLUMNS, query({ needle: '' }))).toEqual([0, 1, 2]);
  });

  it('keeps every row for a column the result does not have', () => {
    expect(filterResultRows(rows, COLUMNS, query({ column: 'missing' }))).toEqual([0, 1, 2]);
  });

  it('rowMatches answers for one row', () => {
    expect(rowMatches(rows[1] ?? [], COLUMNS, query({ needle: 'beta' }))).toBe(true);
  });
});

describe('planResultReplacements', () => {
  const rows: (string | number | null)[][] = [
    [1, 'acme corp', 'a@acme.io'],
    [2, 'acme and acme again', 'b@beta.io'],
    [3, 'beta', null],
  ];

  it('writes only inside the matching cells — the rest of the text survives', () => {
    const plan = planResultReplacements(rows, COLUMNS, query(), 'acorp');

    expect(plan).toEqual([
      { row: 0, column: 'name', value: 'acorp corp' },
      { row: 0, column: 'email', value: 'a@acorp.io' },
      { row: 1, column: 'name', value: 'acorp and acorp again' },
    ]);
  });

  it('plans nothing for an empty needle or a no-op replacement', () => {
    expect(planResultReplacements(rows, COLUMNS, query({ needle: '' }), 'x')).toEqual([]);
    expect(planResultReplacements(rows, COLUMNS, query(), 'acme')).toEqual([]);
  });

  it('respects the addressed column', () => {
    const plan = planResultReplacements(rows, COLUMNS, query({ column: 'email' }), 'acorp');
    expect(plan).toEqual([{ row: 0, column: 'email', value: 'a@acorp.io' }]);
  });

  it('honours the match-case switch and the operators', () => {
    const plan = planResultReplacements(
      rows,
      COLUMNS,
      query({ needle: 'ACME', matchCase: true, column: 'name' }),
      'X',
    );
    expect(plan).toEqual([]);
  });

  it('limits the walk when a limit is given', () => {
    const plan = planResultReplacements(rows, COLUMNS, query(), 'x', { limit: 2 });
    expect(plan).toHaveLength(2);
  });

  it('nextPlanItem steps the walk from a row', () => {
    const plan = planResultReplacements(rows, COLUMNS, query(), 'x');
    expect(nextPlanItem(plan, 1)).toEqual({ row: 1, column: 'name', value: 'x and x again' });
    expect(nextPlanItem(plan, 2)).toBeNull();
  });
});
