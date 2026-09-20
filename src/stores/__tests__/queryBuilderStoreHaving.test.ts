/**
 * HAVING state and the ORDER BY / GROUP BY entry bookkeeping the new clause
 * list depends on.
 */
import { describe, it, expect, beforeEach } from 'vitest';
import { useQueryBuilderStore } from '../queryBuilderStore';
import type { QbCondition } from '../../components/query-builder/types';

function reset() {
  useQueryBuilderStore.setState(useQueryBuilderStore.getInitialState());
}

function s() {
  return useQueryBuilderStore.getState();
}

function condition(patch: Partial<QbCondition> = {}): Omit<QbCondition, 'id'> {
  return {
    table: 'sales',
    column: 'qty',
    operator: '>=',
    value: '2000',
    conjunction: 'AND',
    aggregate: 'SUM',
    ...patch,
  };
}

describe('queryBuilderStore — HAVING', () => {
  beforeEach(reset);

  it('starts with an empty HAVING group', () => {
    expect(s().having.conditions).toEqual([]);
    expect(s().having.groups).toEqual([]);
    expect(s().having.logic).toBe('AND');
  });

  it('adds a condition with an aggregate into the root group', () => {
    s().addHavingCondition(s().having.id, condition());
    const [added] = s().having.conditions;
    expect(added).toMatchObject({ table: 'sales', column: 'qty', aggregate: 'SUM', value: '2000' });
    expect(added!.id).toBeTruthy();
  });

  it('updates and removes a HAVING condition', () => {
    s().addHavingCondition(s().having.id, condition());
    const id = s().having.conditions[0]!.id;

    s().updateHavingCondition(id, { operator: '<', value: '10' });
    expect(s().having.conditions[0]).toMatchObject({ operator: '<', value: '10' });

    s().removeHavingCondition(id);
    expect(s().having.conditions).toEqual([]);
  });

  it('nests a HAVING sub-group and rewrites its logic', () => {
    s().addHavingGroup(s().having.id, 'OR');
    const sub = s().having.groups[0]!;
    expect(sub.logic).toBe('OR');

    s().addHavingCondition(sub.id, condition({ operator: '=', value: 'EU', aggregate: undefined }));
    expect(s().having.groups[0]!.conditions).toHaveLength(1);

    s().setHavingGroupLogic(sub.id, 'AND');
    expect(s().having.groups[0]!.logic).toBe('AND');
  });

  it('keeps HAVING independent from WHERE', () => {
    s().addCondition(s().where.id, condition({ column: 'status', aggregate: undefined }));
    s().addHavingCondition(s().having.id, condition());
    expect(s().where.conditions).toHaveLength(1);
    expect(s().having.conditions).toHaveLength(1);
    expect(s().where.conditions[0]!.column).toBe('status');
  });

  it('counts HAVING as a canvas change (dirty check)', () => {
    s().openFor('panel-1');
    expect(s().hasChanges()).toBe(false);
    s().addHavingCondition(s().having.id, condition());
    expect(s().hasChanges()).toBe(true);
  });

  it('rolls HAVING back on cancel', () => {
    s().openFor('panel-1');
    s().addHavingCondition(s().having.id, condition());
    s().closeFor('cancel');
    expect(s().having.conditions).toEqual([]);
  });

  it('keeps HAVING on OK, so the query survives re-opening', () => {
    s().openFor('panel-1');
    s().addHavingCondition(s().having.id, condition());
    s().closeFor('ok');
    expect(s().having.conditions).toHaveLength(1);
  });

  it('prunes HAVING conditions that reference a removed table', () => {
    s().toggleTable('sales');
    s().toggleTable('regions');
    s().addHavingCondition(s().having.id, condition({ table: 'regions', column: 'name' }));
    s().addHavingCondition(s().having.id, condition());
    s().removeTable('regions');
    expect(s().having.conditions).toHaveLength(1);
    expect(s().having.conditions[0]!.table).toBe('sales');
  });

  it('prunes HAVING conditions inside a nested group too', () => {
    s().toggleTable('sales');
    s().toggleTable('regions');
    s().addHavingGroup(s().having.id, 'OR');
    const sub = s().having.groups[0]!;
    s().addHavingCondition(sub.id, condition({ table: 'regions', column: 'name' }));
    s().removeTable('regions');
    expect(s().having.groups[0]!.conditions).toEqual([]);
  });

  it('blanks HAVING when the owning panel is destroyed', () => {
    s().openFor('panel-1');
    s().addHavingCondition(s().having.id, condition());
    s().destroyFor('panel-1');
    expect(s().having.conditions).toEqual([]);
  });

  it('keeps HAVING per panel', () => {
    s().openFor('panel-1');
    s().addHavingCondition(s().having.id, condition());
    s().openFor('panel-2');
    expect(s().having.conditions).toEqual([]);
    s().openFor('panel-1');
    expect(s().having.conditions).toHaveLength(1);
  });
});

describe('queryBuilderStore — sort / group-by entries', () => {
  beforeEach(reset);

  it('patches an ORDER BY entry in place instead of reordering', () => {
    s().addSort({ table: 'sales', column: 'qty', direction: 'ASC' });
    s().addSort({ table: 'sales', column: 'region', direction: 'ASC' });
    s().updateSort(0, { direction: 'DESC' });

    expect(s().orderBy[0]).toMatchObject({ column: 'qty', direction: 'DESC' });
    expect(s().orderBy[1]).toMatchObject({ column: 'region', direction: 'ASC' });
  });

  it('ignores an out-of-range update', () => {
    s().addSort({ table: 'sales', column: 'qty', direction: 'ASC' });
    s().updateSort(9, { direction: 'DESC' });
    expect(s().orderBy[0]!.direction).toBe('ASC');
  });

  it('removes a GROUP BY entry by index', () => {
    s().addGroupBy({ table: 'sales', column: 'region' });
    s().addGroupBy({ table: 'sales', column: 'channel' });
    s().removeGroupBy(0);
    expect(s().groupBy).toEqual([{ table: 'sales', column: 'channel' }]);
  });
});
