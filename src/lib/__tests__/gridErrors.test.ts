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

  it('does not classify the "cannot estimate" notice — the front end knows it already', () => {
    // "This database cannot estimate row counts" is a fact the front end has
    // on its own: the driver's declaration, or the answer in `totalRowsKind`.
    // The grid renders it directly, so it is an i18n key and **not** an
    // error code — registering it would make the interface branch on a
    // message that can never arrive.
    expect(GRID_ERROR_CODES).not.toContain('grid.count.estimateUnavailable');
    expect(classifyGridError('grid.count.estimateUnavailable: no estimate here')).toBe('unknown');
  });

  it('tolerates surrounding whitespace but stays case-sensitive', () => {
    expect(classifyGridError('  grid.selection.stale: rows changed\n')).toBe('grid.selection.stale');
    expect(classifyGridError('GRID.SELECTION.STALE: rows changed')).toBe('unknown');
  });
});
