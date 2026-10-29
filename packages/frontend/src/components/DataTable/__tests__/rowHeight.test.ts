import { describe, expect, it } from 'vitest';
import {
  DATA_TABLE_ROW_HEIGHT,
  GRID_DENSITY_ROW_HEIGHT_DELTA,
  rowHeightForDensity,
} from '../rowHeight';
import { DEFAULT_GRID_DENSITY } from '../../../types/settings';

/**
 * The density setting of DB-17 is a **delta**, and the acceptance lives in
 * that word: "standard" must not be "28px" (the table grid's height) — it
 * must be "unchanged", for **every** grid, whatever its own default height
 * is. These tests lock the delta semantics, not one grid's pixels.
 */
describe('grid density row heights (DB-17)', () => {
  it('orders the densities: compact < standard < relaxed', () => {
    expect(GRID_DENSITY_ROW_HEIGHT_DELTA.compact).toBeLessThan(
      GRID_DENSITY_ROW_HEIGHT_DELTA.standard,
    );
    expect(GRID_DENSITY_ROW_HEIGHT_DELTA.standard).toBeLessThan(
      GRID_DENSITY_ROW_HEIGHT_DELTA.relaxed,
    );
  });

  it('standard is a delta of zero — the default changes no grid anywhere', () => {
    expect(GRID_DENSITY_ROW_HEIGHT_DELTA[DEFAULT_GRID_DENSITY]).toBe(0);
    // Both data grids, at their own defaults, keep them byte for byte.
    expect(rowHeightForDensity(DATA_TABLE_ROW_HEIGHT, 'standard')).toBe(DATA_TABLE_ROW_HEIGHT);
    expect(rowHeightForDensity(32, 'standard')).toBe(32);
  });

  it('shifts the table grid (28) and the result grid (32) by the same delta', () => {
    expect(rowHeightForDensity(DATA_TABLE_ROW_HEIGHT, 'compact')).toBe(
      DATA_TABLE_ROW_HEIGHT + GRID_DENSITY_ROW_HEIGHT_DELTA.compact,
    );
    expect(rowHeightForDensity(DATA_TABLE_ROW_HEIGHT, 'relaxed')).toBe(
      DATA_TABLE_ROW_HEIGHT + GRID_DENSITY_ROW_HEIGHT_DELTA.relaxed,
    );
    // The result grid starts higher, so it ends higher under the same density —
    // the two grids never collapse onto one height by the setting's accident.
    expect(rowHeightForDensity(32, 'compact')).toBe(32 + GRID_DENSITY_ROW_HEIGHT_DELTA.compact);
    expect(rowHeightForDensity(32, 'relaxed')).toBe(32 + GRID_DENSITY_ROW_HEIGHT_DELTA.relaxed);
  });

  it('every density keeps the row tall enough for the one-line cell content', () => {
    // The smallest combination any data grid can reach.
    const smallest = rowHeightForDensity(DATA_TABLE_ROW_HEIGHT, 'compact');
    expect(smallest).toBeGreaterThanOrEqual(20);
  });
});
