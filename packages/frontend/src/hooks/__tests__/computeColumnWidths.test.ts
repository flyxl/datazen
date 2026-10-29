import { describe, expect, it } from 'vitest';
import { computeInitialColumnWidths, adjustWidthsForSort, SORT_ICON_WIDTH } from '../useColumnResize';

describe('computeInitialColumnWidths', () => {
  it('gives narrow width to id/serial columns', () => {
    const cols = [{ name: 'id', type: 'bigint' }];
    const widths = computeInitialColumnWidths(cols, []);
    expect(widths[0]).toBeLessThanOrEqual(90);
  });

  it('gives narrow width to boolean columns', () => {
    const cols = [{ name: 'ok', type: 'boolean' }];
    const widths = computeInitialColumnWidths(cols, []);
    expect(widths[0]).toBeLessThanOrEqual(90);
  });

  it('gives medium width to varchar columns', () => {
    const cols = [{ name: 'name', type: 'character varying' }];
    const widths = computeInitialColumnWidths(cols, []);
    expect(widths[0]).toBeGreaterThanOrEqual(120);
    expect(widths[0]).toBeLessThanOrEqual(200);
  });

  it('gives wide width to json columns', () => {
    const cols = [{ name: 'metadata', type: 'jsonb' }];
    const widths = computeInitialColumnWidths(cols, []);
    expect(widths[0]).toBeGreaterThanOrEqual(240);
  });

  it('gives medium width to timestamp columns', () => {
    const cols = [{ name: 'created_at', type: 'timestamp with time zone' }];
    const widths = computeInitialColumnWidths(cols, []);
    expect(widths[0]).toBeGreaterThanOrEqual(170);
    expect(widths[0]).toBeLessThanOrEqual(200);
  });

  it('gives narrow width to integer columns with short names', () => {
    const cols = [
      { name: 'qty', type: 'integer' },
      { name: 'stock', type: 'integer' },
    ];
    const widths = computeInitialColumnWidths(cols, []);
    expect(widths[0]).toBeLessThanOrEqual(100);
    expect(widths[1]).toBeLessThanOrEqual(100);
  });

  it('gives medium width to numeric/decimal price columns', () => {
    const cols = [{ name: 'price', type: 'numeric' }];
    const widths = computeInitialColumnWidths(cols, []);
    expect(widths[0]).toBeGreaterThanOrEqual(100);
    expect(widths[0]).toBeLessThanOrEqual(140);
  });

  it('widens columns when actual data is longer than type default', () => {
    const cols = [{ name: 'name', type: 'character varying' }];
    const rows = [['a very long product name that needs more space really']];
    const widths = computeInitialColumnWidths(cols, rows);
    expect(widths[0]).toBeGreaterThan(150);
  });

  it('ensures header text fits (long column names get wider)', () => {
    const cols = [{ name: 'a_very_long_column_name_here', type: 'integer' }];
    const widths = computeInitialColumnWidths(cols, []);
    expect(widths[0]).toBeGreaterThanOrEqual(100);
  });

  it('handles mixed column types correctly', () => {
    const cols = [
      { name: 'id', type: 'bigint' },
      { name: 'name', type: 'character varying' },
      { name: 'price', type: 'numeric' },
      { name: 'stock', type: 'integer' },
      { name: 'metadata', type: 'jsonb' },
      { name: 'created_at', type: 'timestamp with time zone' },
    ];
    const widths = computeInitialColumnWidths(cols, []);
    expect(widths[0]).toBeLessThan(widths[4]);
    expect(widths[3]).toBeLessThanOrEqual(widths[1]);
  });

  it('clamps to max width', () => {
    const cols = [{ name: 'data', type: 'jsonb' }];
    const rows = [['{"a":1,"b":2,"c":3,"d":4,"e":"a very very long json field that could be unlimited"}']];
    const widths = computeInitialColumnWidths(cols, rows);
    expect(widths[0]).toBeLessThanOrEqual(400);
  });

  it('numeric column is not excessively wide', () => {
    const cols = [{ name: 'price', type: 'numeric' }];
    const widths = computeInitialColumnWidths(cols, []);
    expect(widths[0]).toBeLessThanOrEqual(105);
  });

  it('initial width does not include sort icon space', () => {
    const cols = [{ name: 'stock', type: 'integer' }];
    const widths = computeInitialColumnWidths(cols, [[0]]);
    expect(widths[0]).toBe(90);
  });

  it('long-named integer column is compact', () => {
    const cols = [{ name: 'category_id', type: 'integer' }];
    const rows = [[2], [12], [32]];
    const widths = computeInitialColumnWidths(cols, rows);
    expect(widths[0]).toBeLessThanOrEqual(130);
  });
});

describe('adjustWidthsForSort', () => {
  it('returns original widths when no sort is active', () => {
    const widths = [90, 150, 100];
    const cols = [{ name: 'id' }, { name: 'name' }, { name: 'price' }];
    const result = adjustWidthsForSort(widths, cols, []);
    expect(result).toEqual([90, 150, 100]);
  });

  it('expands only the sorted column by SORT_ICON_WIDTH', () => {
    const widths = [90, 150, 100];
    const cols = [{ name: 'id' }, { name: 'name' }, { name: 'price' }];
    const result = adjustWidthsForSort(widths, cols, [{ column: 'name', descending: false }]);
    expect(result[0]).toBe(90);
    expect(result[1]).toBe(150 + SORT_ICON_WIDTH);
    expect(result[2]).toBe(100);
  });

  it('does not mutate the original widths array', () => {
    const widths = [90, 150, 100];
    const cols = [{ name: 'id' }, { name: 'name' }, { name: 'price' }];
    adjustWidthsForSort(widths, cols, [{ column: 'name', descending: false }]);
    expect(widths).toEqual([90, 150, 100]);
  });
});
