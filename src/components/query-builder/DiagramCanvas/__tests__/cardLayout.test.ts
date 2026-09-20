/**
 * Card placement rules: grid snapping and row alignment.
 *
 * These exist because two tables dragged onto the canvas previously landed a
 * few pixels apart vertically, which made the diagram look broken with no way
 * to fix it by hand.
 */
import { describe, expect, it } from 'vitest';
import {
  CARD_GRID,
  CARD_STRIDE,
  alignDroppedCard,
  resolveDragPosition,
  snapToGrid,
} from '../cardLayout';

describe('snapToGrid', () => {
  it('rounds to the nearest grid step', () => {
    expect(snapToGrid(0)).toBe(0);
    expect(snapToGrid(CARD_GRID + 1)).toBe(CARD_GRID);
    expect(snapToGrid(CARD_GRID * 1.6)).toBe(CARD_GRID * 2);
  });

  it('never goes negative', () => {
    expect(snapToGrid(-50)).toBe(0);
  });
});

describe('alignDroppedCard', () => {
  it('snaps to the grid when the canvas is empty', () => {
    expect(alignDroppedCard({ x: 40, y: 25 }, {})).toEqual({ x: 48, y: 24 });
  });

  it('adopts the top edge of a card in the same row', () => {
    const pos = alignDroppedCard({ x: 400, y: 61 }, { users: { x: 48, y: 24 } });
    expect(pos.y).toBe(24);
  });

  it('places the new card to the right of the row it joins', () => {
    const pos = alignDroppedCard({ x: 50, y: 30 }, { users: { x: 48, y: 24 } });
    expect(pos.x).toBe(48 + CARD_STRIDE);
  });

  it('keeps its own row when dropped far below', () => {
    const pos = alignDroppedCard({ x: 300, y: 400 }, { users: { x: 48, y: 24 } });
    expect(pos.y).toBe(408);
    expect(pos.x).toBe(312);
  });

  it('stacks a third card after the rightmost card of the row', () => {
    const row = { a: { x: 48, y: 24 }, b: { x: 48 + CARD_STRIDE, y: 24 } };
    const pos = alignDroppedCard({ x: 60, y: 30 }, row);
    expect(pos.y).toBe(24);
    expect(pos.x).toBe(48 + CARD_STRIDE * 2);
  });
});

describe('resolveDragPosition', () => {
  it('only snaps to grid, no row alignment', () => {
    // Dragging `users` at (300, 400): grid-snap only, no row jump.
    const pos = resolveDragPosition({ x: 300, y: 400 }, { users: { x: 48, y: 24 } }, 'users');
    expect(pos).toEqual({ x: 312, y: 408 });
  });

  it('does not force row alignment when y is near another card', () => {
    // BUG scenario: er_customers at (0,0), er_orders at (264,0).
    // Dragging er_customers slightly down should NOT jump it to the right of er_orders.
    const pos = resolveDragPosition(
      { x: 5, y: 5 },
      { er_customers: { x: 0, y: 0 }, er_orders: { x: 264, y: 0 } },
      'er_customers',
    );
    // Must NOT be forced to rightMost + CARD_STRIDE
    expect(pos.x).toBe(0);
    expect(pos.y).toBe(0);
  });

  it('preserves user-intended free positioning during drag', () => {
    const pos = resolveDragPosition(
      { x: 150, y: 300 },
      { a: { x: 0, y: 0 }, b: { x: 264, y: 0 } },
      'a',
    );
    expect(pos).toEqual({ x: 144, y: 312 });
  });
});
