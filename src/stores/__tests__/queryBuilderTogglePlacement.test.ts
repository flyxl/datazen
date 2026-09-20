/**
 * Regression: a table added from the FROM picker (toggleTable) got no entry in
 * `tablePositions`, so DiagramCanvas fell back to {0,0} and every picked table
 * stacked on the same spot — the later card hid the earlier ones.
 */
import { beforeEach, describe, expect, it } from 'vitest';
import { useQueryBuilderStore } from '../queryBuilderStore';
import { CARD_STRIDE } from '../../components/query-builder/DiagramCanvas/cardLayout';

function resetStore() {
  useQueryBuilderStore.setState(useQueryBuilderStore.getInitialState());
}

beforeEach(resetStore);

describe('toggleTable placement', () => {
  it('places the first table in the top-left corner of the canvas', () => {
    useQueryBuilderStore.getState().toggleTable('er_customers');
    expect(useQueryBuilderStore.getState().tablePositions['er_customers']).toEqual({ x: 0, y: 0 });
  });

  it('places each additional table to the right, never on top of an existing card', () => {
    useQueryBuilderStore.getState().toggleTable('a');
    useQueryBuilderStore.getState().toggleTable('b');
    useQueryBuilderStore.getState().toggleTable('c');
    const { tablePositions } = useQueryBuilderStore.getState();
    const spots = ['a', 'b', 'c'].map((t) => `${tablePositions[t]!.x},${tablePositions[t]!.y}`);
    expect(new Set(spots).size).toBe(3);
    // Strictly increasing x: each new card sits right of the previous one.
    const xs = spots.map((spot) => Number(spot.split(',')[0]));
    expect(xs[1]! - xs[0]!).toBeGreaterThanOrEqual(CARD_STRIDE - 24);
    expect(xs[2]! - xs[1]!).toBeGreaterThanOrEqual(CARD_STRIDE - 24);
  });

  it('does not move a table that already has a position when re-added after removal cleanup', () => {
    useQueryBuilderStore.getState().toggleTable('a');
    const posA = useQueryBuilderStore.getState().tablePositions['a'];
    useQueryBuilderStore.getState().toggleTable('b');
    expect(useQueryBuilderStore.getState().tablePositions['a']).toEqual(posA);
  });

  it('removing a table clears its position so a re-add gets a fresh slot', () => {
    useQueryBuilderStore.getState().toggleTable('a');
    useQueryBuilderStore.getState().toggleTable('b');
    useQueryBuilderStore.getState().toggleTable('a'); // toggle off
    expect(useQueryBuilderStore.getState().tablePositions['a']).toBeUndefined();
    useQueryBuilderStore.getState().toggleTable('a'); // toggle on again
    const pos = useQueryBuilderStore.getState().tablePositions['a'];
    expect(pos).toBeDefined();
    expect(pos!.x).not.toBe(useQueryBuilderStore.getState().tablePositions['b']!.x);
  });
});
