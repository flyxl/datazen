/**
 * Auto-join detection for the query builder's canvas.
 *
 * The property that matters after prediction was introduced: an inferred
 * relationship arrives as a candidate like any other, but it carries
 * `origin: 'predicted'` so the canvas can draw it as a guess rather than as a
 * constraint the database enforces.
 */
import { describe, expect, it } from 'vitest';
import { renderHook } from '@testing-library/react';
import { constraintKey, useAutoJoin, type ForeignKeyRelation } from '../useAutoJoin';

const relation = (over: Partial<ForeignKeyRelation> = {}): ForeignKeyRelation => ({
  fromTable: 'orders',
  fromColumn: 'user_id',
  toTable: 'users',
  toColumn: 'id',
  constraint: 'fk_orders_user',
  ordinal: 1,
  pairCount: 1,
  ...over,
});

const detect = (foreignKeys: ForeignKeyRelation[], tables = ['orders', 'users']) =>
  renderHook(() => useAutoJoin(tables, foreignKeys)).result.current;

describe('useAutoJoin', () => {
  it('defaults a relation with no origin to declared', () => {
    expect(detect([relation()])[0]!.origin).toBe('declared');
  });

  it('carries a predicted relation through to the candidate', () => {
    const joins = detect([relation({ constraint: 'predicted::c1', origin: 'predicted' })]);
    expect(joins[0]!.origin).toBe('predicted');
    expect(joins[0]!.constraint).toBe(constraintKey('orders', 'predicted::c1'));
  });

  it('still skips self-references and tables that are not on the canvas', () => {
    expect(detect([relation({ toTable: 'orders' })])).toEqual([]);
    expect(detect([relation({ toTable: 'users' })], ['orders'])).toEqual([]);
  });
});
