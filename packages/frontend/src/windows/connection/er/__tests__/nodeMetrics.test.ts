import { describe, it, expect } from 'vitest';
import {
  ER_COLLAPSED_FOOTER_HEIGHT,
  ER_COLUMN_ROW_HEIGHT,
  ER_HEADER_HEIGHT,
  ER_NODE_BORDER,
  ER_NODE_WIDTH,
  erColumnRowCenterY,
  erNodeBodyHeight,
  erNodeHeight,
} from '../nodeMetrics';

/**
 * These numbers are a contract with `TableNode`, which applies each one as an
 * explicit inline height. If they drift, the layout packs nodes against a size
 * that is not what renders — which is precisely how the previous grid came to
 * reserve 1000px for a table that draws at 340px.
 */
describe('ER node metrics', () => {
  it('gives a node a fixed width the layout can pack against', () => {
    expect(ER_NODE_WIDTH).toBeGreaterThan(0);
  });

  it('sizes a node from its header, its rows and its border', () => {
    const columns = 5;
    expect(erNodeHeight(columns)).toBe(
      ER_HEADER_HEIGHT + columns * ER_COLUMN_ROW_HEIGHT + ER_NODE_BORDER * 2,
    );
  });

  it('grows with every column, with no scroll cap', () => {
    // An internal scroll would move the connection points with `scrollTop`.
    expect(erNodeBodyHeight(40)).toBe(40 * ER_COLUMN_ROW_HEIGHT);
    expect(erNodeHeight(40)).toBe(
      ER_HEADER_HEIGHT + 40 * ER_COLUMN_ROW_HEIGHT + ER_NODE_BORDER * 2,
    );
  });

  it('matches the body height', () => {
    expect(erNodeBodyHeight(3)).toBe(3 * ER_COLUMN_ROW_HEIGHT);
    expect(erNodeBodyHeight(0)).toBe(0);
  });

  it('centres each column row inside the node', () => {
    // The first row starts after the border and the header.
    expect(erColumnRowCenterY(0)).toBe(
      ER_NODE_BORDER + ER_HEADER_HEIGHT + ER_COLUMN_ROW_HEIGHT / 2,
    );
    // Consecutive rows are exactly one row apart.
    expect(erColumnRowCenterY(3) - erColumnRowCenterY(2)).toBe(ER_COLUMN_ROW_HEIGHT);
    // And the centre must land inside the node's own height.
    expect(erColumnRowCenterY(9)).toBeLessThan(erNodeHeight(10));
  });

  it('clamps a negative column index to the first row', () => {
    expect(erColumnRowCenterY(-1)).toBe(erColumnRowCenterY(0));
  });

  it('ignores a negative column count rather than going above the header', () => {
    expect(erNodeBodyHeight(-4)).toBe(0);
    expect(erNodeHeight(-4)).toBe(ER_HEADER_HEIGHT + ER_NODE_BORDER * 2);
  });

  it('makes a collapsed node shorter than the same node expanded', () => {
    const expanded = erNodeHeight(20);
    const collapsed = erNodeHeight(20, true);
    expect(collapsed).toBe(ER_HEADER_HEIGHT + ER_COLLAPSED_FOOTER_HEIGHT + ER_NODE_BORDER * 2);
    expect(collapsed).toBeLessThan(expanded);
  });

  it('collapses to the same height regardless of column count', () => {
    // The column list is not rendered when collapsed, so its length is irrelevant.
    expect(erNodeHeight(3, true)).toBe(erNodeHeight(60, true));
  });
});
