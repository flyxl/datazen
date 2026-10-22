import { describe, expect, it } from 'vitest';
import {
  GRID_ERROR_CODES,
  GRID_ERROR_NAMESPACE,
  classifyGridError,
  type GridErrorCode,
} from '../gridErrors';

describe('GRID_ERROR_CODES', () => {
  it('only registers `grid.` domain codes (never the `unknown` sentinel)', () => {
    expect(GRID_ERROR_CODES.length).toBeGreaterThan(0);
    for (const code of GRID_ERROR_CODES) {
      expect(code.startsWith(GRID_ERROR_NAMESPACE)).toBe(true);
      expect(code).not.toBe('unknown');
    }
  });

  it('is typed as the GridErrorCode union, so appenders get a compile error on typos', () => {
    const codes: readonly GridErrorCode[] = GRID_ERROR_CODES;
    expect(codes).toBe(GRID_ERROR_CODES);
  });
});

describe('classifyGridError', () => {
  it('matches every registered prefix as a prefix, not as a substring', () => {
    for (const code of GRID_ERROR_CODES) {
      expect(classifyGridError(`${code}: the human-readable reason`)).toBe(code);
    }
  });

  it('matches a bare code without a separator', () => {
    expect(classifyGridError('grid.selection.invalid')).toBe('grid.selection.invalid');
  });

  it('does not classify a longer reason that merely starts with the code text', () => {
    expect(classifyGridError('grid.selection.invalidity: not a registered code')).toBe('unknown');
  });

  it('does not classify a code mentioned inside ordinary prose', () => {
    expect(classifyGridError('unexpected grid.selection.invalid: wrapped in another message')).toBe(
      'unknown',
    );
  });

  it('picks the longest registered prefix when prefixes overlap', () => {
    expect(classifyGridError('grid.selection.outOfBounds: cell is outside the page')).toBe(
      'grid.selection.outOfBounds',
    );
  });

  it('falls back to unknown for unrelated, empty and whitespace-only messages', () => {
    expect(classifyGridError('permission denied for table users')).toBe('unknown');
    expect(classifyGridError('')).toBe('unknown');
    expect(classifyGridError('   ')).toBe('unknown');
  });

  it('does not treat an unregistered insert code as a registered one', () => {
    // `grid.insert.noWritableColumn` has **no producer** in the back end yet
    // (the NOT NULL check of an insert row is not implemented), so it is not
    // in the registry. A registered code without a producer would make the
    // interface branch on a message that can never arrive; an unregistered
    // code has to fall back to `unknown` — and the caller renders the raw
    // message.
    expect(GRID_ERROR_CODES).not.toContain('grid.insert.noWritableColumn');
    expect(classifyGridError('grid.insert.noWritableColumn: column "email" requires a value')).toBe(
      'unknown',
    );
  });

  it('maps the insert prefixes and falls back to the raw message', () => {
    expect(
      classifyGridError("grid.insert.unknownColumn: INSERT writes column(s) that do not exist on 'users'"),
    ).toBe('grid.insert.unknownColumn');
    // The "returning unavailable" code is a warning of the plan rather than an
    // error message, but it is registered so the interface can branch on it.
    expect(GRID_ERROR_CODES).toContain('grid.insert.returningUnavailable');
    // An unrelated message stays unknown, and the caller renders it verbatim.
    expect(classifyGridError('permission denied for table users')).toBe('unknown');
  });

  it('maps the filter prefixes and falls back to the raw message', () => {
    // The shared filter layer refuses a filter the database cannot serve and
    // a filter that is missing a value. Both messages name the offending
    // condition, so the interface can point at it.
    expect(
      classifyGridError('grid.filter.incomplete: column "id" needs a value (operator \'eq\')'),
    ).toBe('grid.filter.incomplete');
    expect(
      classifyGridError(
        'grid.filter.unsupportedOperator: this database does not support the \'regex\' filter on column "name"; try \'like\' or \'contains\' instead',
      ),
    ).toBe('grid.filter.unsupportedOperator');
  });

  it('has no error code in the count domain — the "cannot estimate" notice is an i18n key', () => {
    // "This database cannot estimate row counts" is a fact the front end has
    // on its own: the driver's declaration, or the answer in `totalRowsKind`.
    // The grid renders it directly, so it is an i18n key
    // (`pagination.estimateUnavailable`) and **not** an error code:
    // registering it would make the interface branch on a message that can
    // never arrive. The `grid.count.` domain therefore has no members at all.
    expect(GRID_ERROR_CODES.filter((code) => code.startsWith('grid.count.'))).toEqual([]);
    // And a prefix that merely looks like one stays unclassified.
    expect(classifyGridError('grid.count.estimateUnavailable: no estimate here')).toBe('unknown');
  });

  it('maps the result-grid prefixes and falls back to the raw message', () => {
    // The five codes of the result grid's write-back gate. Every one of them
    // has a producer in the back end, so the interface may branch on them.
    expect(
      classifyGridError(
        'grid.result.originUnavailable: the driver cannot report where this result came from',
      ),
    ).toBe('grid.result.originUnavailable');
    expect(
      classifyGridError(
        'grid.result.originAmbiguous: column origin count does not match the result column count',
      ),
    ).toBe('grid.result.originAmbiguous');
    expect(
      classifyGridError(
        'grid.result.rowNotAddressable: the row identity has NULL value(s) in column(s) id',
      ),
    ).toBe('grid.result.rowNotAddressable');
    expect(
      classifyGridError(
        'grid.result.multiRowNotAllowed: editing many query-result rows at once is not supported',
      ),
    ).toBe('grid.result.multiRowNotAllowed');
    expect(
      classifyGridError('grid.result.writeDenied: column "doubled" is not a writable column'),
    ).toBe('grid.result.writeDenied');
    // A code that merely looks like one of them stays unclassified.
    expect(classifyGridError('grid.result.writeDeniedish: nope')).toBe('unknown');
  });

  it('tolerates surrounding whitespace but stays case-sensitive', () => {
    expect(classifyGridError('  grid.selection.stale: rows changed\n')).toBe('grid.selection.stale');
    expect(classifyGridError('GRID.SELECTION.STALE: rows changed')).toBe('unknown');
  });
});
