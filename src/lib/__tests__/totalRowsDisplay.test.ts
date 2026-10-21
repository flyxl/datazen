import { describe, expect, it } from 'vitest';

import {
  canPageForward,
  formatRowCount,
  totalRowsView,
} from '../totalRowsDisplay';

describe('totalRowsView', () => {
  it('defaults to exact when the kind is undefined', () => {
    // A backend that does not know about the three states counted exactly.
    expect(totalRowsView(undefined, 1234)).toEqual({ kind: 'exact', value: 1234 });
    expect(totalRowsView(null, 1234)).toEqual({ kind: 'exact', value: 1234 });
  });

  it('maps the estimated kind to its own shape', () => {
    expect(totalRowsView('estimated', 1_200_000)).toEqual({
      kind: 'estimated',
      value: 1_200_000,
    });
  });

  it('does not read the value for the unsupported kind', () => {
    // There is no number to read: the database cannot say how many rows it
    // has, and the number that happens to be in the panel is not one.
    expect(totalRowsView('unsupported', 0)).toEqual({ kind: 'unknown' });
  });
});

describe('formatRowCount', () => {
  it('compacts large numbers', () => {
    expect(formatRowCount(0)).toBe('0');
    expect(formatRowCount(999)).toBe('999');
    expect(formatRowCount(1234)).toBe('1,234');
    expect(formatRowCount(1_200_000)).toBe('1.2M');
    expect(formatRowCount(2_500_000_000)).toBe('2.5B');
  });

  it('never renders a number that is not one', () => {
    expect(formatRowCount(Number.NaN)).toBe('0');
    expect(formatRowCount(Number.POSITIVE_INFINITY)).toBe('0');
    expect(formatRowCount(-5)).toBe('0');
  });
});

describe('canPageForward', () => {
  it('only moves on when the page is full in the unknown state', () => {
    const unknown = { kind: 'unknown' } as const;
    expect(canPageForward(unknown, 50, 50)).toBe(true);
    expect(canPageForward(unknown, 49, 50)).toBe(false);
    expect(canPageForward(unknown, 0, 50)).toBe(false);
  });

  it('never closes the grid on an estimate', () => {
    // An estimate may be too small; the grid must not treat it as the end of
    // the data.
    expect(canPageForward({ kind: 'estimated', value: 10 }, 50, 50)).toBe(true);
  });

  it('stops at the end of an exact total', () => {
    expect(canPageForward({ kind: 'exact', value: 2 }, 2, 50)).toBe(false);
    expect(canPageForward({ kind: 'exact', value: 120 }, 50, 50)).toBe(true);
  });
});
