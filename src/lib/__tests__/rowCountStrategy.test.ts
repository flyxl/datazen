import { describe, expect, it } from 'vitest';
import { resolveCountStrategy } from '../rowCountStrategy';
import { resolveDefaultTableSort } from '../defaultSort';

describe('resolveCountStrategy', () => {
  it('maps the explicit modes onto the wire strategies', () => {
    expect(resolveCountStrategy({ mode: 'exact', threshold: 200_000 })).toBe('exact');
    expect(resolveCountStrategy({ mode: 'estimated', threshold: 200_000 })).toBe('estimated');
    expect(resolveCountStrategy({ mode: 'none', threshold: 200_000 })).toBe('unsupported');
  });

  it('leaves the decision to the driver in auto mode when the size is unknown', () => {
    // No opinion at all: the request carries no strategy, so the host uses the
    // driver's own declaration. Forcing "exact" here would make every
    // PostgreSQL read pay for a full COUNT(*).
    expect(resolveCountStrategy({ mode: 'auto', threshold: 200_000 })).toBeUndefined();
    expect(
      resolveCountStrategy({ mode: 'auto', threshold: 200_000, knownRowCount: null }),
    ).toBeUndefined();
    expect(
      resolveCountStrategy({ mode: 'auto', threshold: 200_000, knownRowCount: Number.NaN }),
    ).toBeUndefined();
  });

  it('estimates above the threshold and counts below it in auto mode', () => {
    expect(
      resolveCountStrategy({ mode: 'auto', threshold: 200_000, knownRowCount: 200_001 }),
    ).toBe('estimated');
    expect(
      resolveCountStrategy({ mode: 'auto', threshold: 200_000, knownRowCount: 200_000 }),
    ).toBe('exact');
    expect(
      resolveCountStrategy({ mode: 'auto', threshold: 200_000, knownRowCount: 12 }),
    ).toBe('exact');
  });

  it('treats a non-positive threshold as "always count"', () => {
    expect(
      resolveCountStrategy({ mode: 'auto', threshold: 0, knownRowCount: 9_000_000 }),
    ).toBe('exact');
    expect(
      resolveCountStrategy({ mode: 'auto', threshold: -5, knownRowCount: 9_000_000 }),
    ).toBe('exact');
  });

  it('never lets a dirty threshold turn the auto mode into a guess', () => {
    expect(
      resolveCountStrategy({ mode: 'auto', threshold: Number.NaN, knownRowCount: 9_000_000 }),
    ).toBe('exact');
  });
});

describe('resolveDefaultTableSort', () => {
  it('follows the global setting while the panel inherits', () => {
    expect(resolveDefaultTableSort('inherit', 'primaryKeyAsc')).toBe('primaryKeyAsc');
    expect(resolveDefaultTableSort('inherit', 'primaryKeyDesc')).toBe('primaryKeyDesc');
    expect(resolveDefaultTableSort('inherit', 'none')).toBe('none');
  });

  it('lets the panel override win over the global setting', () => {
    expect(resolveDefaultTableSort('none', 'primaryKeyAsc')).toBe('none');
    expect(resolveDefaultTableSort('primaryKeyDesc', 'primaryKeyAsc')).toBe('primaryKeyDesc');
  });
});
