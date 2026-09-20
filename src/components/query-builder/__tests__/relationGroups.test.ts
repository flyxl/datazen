/**
 * Folding joins + detected foreign keys into the relation groups the canvas
 * draws. The important property is that a **composite** FK stays ONE group:
 * that is what lets the canvas draw a single trunk and the store confirm /
 * remove every column pair together.
 */
import { describe, expect, it } from 'vitest';
import { buildRelationGroups } from '../relationGroups';
import { constraintKey, type ForeignKeyRelation } from '../hooks/useAutoJoin';
import type { QbJoin } from '../types';

const relation = (
  fromColumn: string,
  toColumn: string,
  constraint = 'fk_shipment_stock',
  fromTable = 'shipment',
  toTable = 'stock',
  ordinal = 1,
  pairCount = 2,
): ForeignKeyRelation => ({
  fromTable,
  fromColumn,
  toTable,
  toColumn,
  constraint,
  ordinal,
  pairCount,
});

const join = (over: Partial<QbJoin> = {}): QbJoin => ({
  id: 'j1',
  type: 'INNER',
  leftTable: 'shipment',
  leftColumn: 'item_id',
  rightTable: 'stock',
  rightColumn: 'item_id',
  isManual: false,
  ...over,
});

const build = (
  joins: QbJoin[],
  fkRelations: ForeignKeyRelation[],
  tables = ['shipment', 'stock'],
) => buildRelationGroups({ joins, fkRelations, selectedTables: tables });

describe('buildRelationGroups — composite FK', () => {
  const composite = [
    relation('item_id', 'item_id', 'fk_x', 'shipment', 'stock', 1, 2),
    relation('wh_id', 'wh_id', 'fk_x', 'shipment', 'stock', 2, 2),
  ];

  it('keeps both column pairs in one group', () => {
    const groups = build([], composite);
    expect(groups).toHaveLength(1);
    expect(groups[0]!.pairs).toHaveLength(2);
    expect(groups[0]!.id).toBe(constraintKey('shipment', 'fk_x'));
    expect(groups[0]!.constraint).toBe('fk_x');
  });

  it('reports an unconfirmed group as a candidate', () => {
    const groups = build([], composite);
    expect(groups[0]!.pairs.every((pair) => !pair.confirmed)).toBe(true);
  });

  it('marks pairs confirmed from the joins, pair by pair', () => {
    // Only the first pair is in the SQL → the group is half confirmed, which is
    // exactly the state the validator flags as an incomplete composite join.
    const groups = build([join()], composite);
    expect(groups[0]!.pairs.map((pair) => pair.confirmed)).toEqual([true, false]);
  });

  it('takes the JOIN type from the confirmed pairs', () => {
    const groups = build([join({ type: 'LEFT' })], composite);
    expect(groups[0]!.type).toBe('LEFT');
  });

  it('previews INNER for a completely unconfirmed group', () => {
    expect(build([], composite)[0]!.type).toBe('INNER');
  });
});

describe('buildRelationGroups — separate constraints', () => {
  it('produces one group per constraint', () => {
    const groups = build(
      [],
      [
        relation('item_id', 'item_id', 'fk_a', 'shipment', 'stock', 1, 1),
        relation('wh_id', 'id', 'fk_b', 'shipment', 'stock', 1, 1),
      ],
    );
    expect(groups).toHaveLength(2);
    expect(new Set(groups.map((group) => group.constraint))).toEqual(new Set(['fk_a', 'fk_b']));
  });

  it('ignores relations whose table is off the canvas', () => {
    expect(build([], [relation('item_id', 'item_id', 'fk_x', 'shipment', 'ghost')])).toEqual([]);
  });

  it('ignores self-referencing relations (not expressible)', () => {
    const groups = build(
      [],
      [relation('manager_id', 'id', 'fk_self', 'employee', 'employee')],
      ['employee'],
    );
    expect(groups).toEqual([]);
  });
});

describe('buildRelationGroups — manual joins', () => {
  it('draws each manual join as its own confirmed group', () => {
    const groups = build([join({ id: 'm1', isManual: true })], []);
    expect(groups).toHaveLength(1);
    expect(groups[0]!.kind).toBe('manual');
    expect(groups[0]!.id).toBe('manual:m1');
    expect(groups[0]!.pairs[0]!.confirmed).toBe(true);
  });

  it('does not duplicate an FK-derived join as a manual group', () => {
    const groups = build(
      [join()],
      [relation('item_id', 'item_id', 'fk_x', 'shipment', 'stock', 1, 1)],
    );
    expect(groups).toHaveLength(1);
    expect(groups[0]!.kind).toBe('fk');
  });
});

describe('buildRelationGroups — safety net', () => {
  it('still draws a confirmed FK join with no detected relation', () => {
    // Detection can fail (schema cache miss); the SQL would otherwise show a
    // join with no line on the canvas.
    const groups = build([join()], []);
    expect(groups).toHaveLength(1);
    expect(groups[0]!.id).toBe('join:j1');
    expect(groups[0]!.pairs[0]!.confirmed).toBe(true);
  });
});

describe('buildRelationGroups — candidate type preview', () => {
  // Regression: choosing LEFT/RIGHT/FULL on an unconfirmed FK wrote the type
  // into `autoJoins`, but the group still previewed a hardcoded INNER — the
  // popover radio never moved, so the buttons looked dead.
  it('previews the type the user picked on the candidate group', () => {
    const composite = [relation('item_id', 'item_id', 'fk_x', 'shipment', 'stock', 1, 1)];
    const groups = buildRelationGroups({
      joins: [],
      fkRelations: composite,
      selectedTables: ['shipment', 'stock'],
      candidateTypes: { [constraintKey('shipment', 'fk_x')]: 'LEFT' },
    });
    expect(groups[0]!.type).toBe('LEFT');
  });

  it('still prefers the confirmed join type over the candidate preview', () => {
    const composite = [relation('item_id', 'item_id', 'fk_x', 'shipment', 'stock', 1, 1)];
    const groups = buildRelationGroups({
      joins: [join({ type: 'FULL' })],
      fkRelations: composite,
      selectedTables: ['shipment', 'stock'],
      candidateTypes: { [constraintKey('shipment', 'fk_x')]: 'LEFT' },
    });
    expect(groups[0]!.type).toBe('FULL');
  });

  it('falls back to INNER when the user has not picked a type', () => {
    const composite = [relation('item_id', 'item_id', 'fk_x', 'shipment', 'stock', 1, 1)];
    const groups = buildRelationGroups({
      joins: [],
      fkRelations: composite,
      selectedTables: ['shipment', 'stock'],
      candidateTypes: {},
    });
    expect(groups[0]!.type).toBe('INNER');
  });

  it('keeps the INNER default when the preview map is omitted', () => {
    const composite = [relation('item_id', 'item_id', 'fk_x', 'shipment', 'stock', 1, 1)];
    expect(build([], composite)[0]!.type).toBe('INNER');
  });
});
