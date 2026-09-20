/**
 * How a condition row's AND/OR reaches the generated SQL.
 *
 * This is one behaviour spread over three places, so it is tested together:
 *  - the generator joins rows with each row's own conjunction;
 *  - the store seeds a new row's conjunction from its group's logic and moves
 *    the existing rows when the group's logic changes;
 *  - the editor seeds new rows the same way (covered in BuildStatement.test).
 *
 * It used to be broken in both directions: the generator ignored the row's
 * conjunction (so `a OR b` came out `a AND b`), while the group's logic drove
 * everything — meaning a row set to OR at the root silently did nothing.
 */
import { describe, expect, it, beforeEach } from 'vitest';
import { generateSql } from '../hooks/useSqlGenerator';
import type { GenerateSqlInput } from '../hooks/useSqlGenerator';
import { useQueryBuilderStore } from '../../../stores/queryBuilderStore';
import type { QbCondition, QbConditionGroup } from '../types';

function group(id: string, logic: 'AND' | 'OR' = 'AND'): QbConditionGroup {
  return { id, logic, conditions: [], groups: [] };
}

function cond(id: string, conjunction: 'AND' | 'OR'): QbCondition {
  return {
    id,
    table: 't',
    column: `c${id}`,
    operator: '=',
    value: '1',
    conjunction,
  };
}

function baseInput(where: QbConditionGroup): GenerateSqlInput {
  return {
    selectedTables: ['t'],
    selectedColumns: [{ table: 't', column: 'id' }],
    joins: [],
    tableAliases: {},
    where,
    orderBy: [],
    groupBy: [],
    distinct: false,
    limit: null,
    offset: null,
    databaseType: 'postgresql',
  };
}

describe('generateSql — condition conjunctions', () => {
  it('joins root rows with each row own AND/OR', () => {
    const where: QbConditionGroup = {
      ...group('root'),
      conditions: [cond('1', 'AND'), cond('2', 'OR')],
    };
    expect(generateSql(baseInput(where))).toContain('WHERE "t"."c1" = 1 OR "t"."c2" = 1');
  });

  it('honours a group logic of OR when the rows carry OR', () => {
    const sub: QbConditionGroup = {
      ...group('sub', 'OR'),
      conditions: [cond('1', 'OR'), cond('2', 'OR')],
    };
    const where: QbConditionGroup = {
      ...group('root'),
      conditions: [cond('root1', 'AND')],
      groups: [sub],
    };
    expect(generateSql(baseInput(where))).toContain(
      'WHERE "t"."croot1" = 1 AND ("t"."c1" = 1 OR "t"."c2" = 1)',
    );
  });

  it('keeps the first row of a group unprefixed', () => {
    const where: QbConditionGroup = {
      ...group('root'),
      conditions: [cond('1', 'OR'), cond('2', 'AND')],
    };
    // A leading OR would be a syntax error; the first row's conjunction is
    // ignored on purpose.
    expect(generateSql(baseInput(where))).toContain('WHERE "t"."c1" = 1 AND "t"."c2" = 1');
  });

  it('links sub-groups with the group logic, not a row conjunction', () => {
    const where: QbConditionGroup = {
      ...group('root', 'AND'),
      conditions: [cond('1', 'AND')],
      groups: [
        { ...group('a', 'AND'), conditions: [cond('a1', 'AND')] },
        { ...group('b', 'AND'), conditions: [cond('b1', 'AND')] },
      ],
    };
    expect(generateSql(baseInput(where))).toContain(
      'WHERE "t"."c1" = 1 AND ("t"."ca1" = 1) AND ("t"."cb1" = 1)',
    );
  });
});

describe('store — condition conjunctions', () => {
  beforeEach(() => {
    useQueryBuilderStore.setState(useQueryBuilderStore.getInitialState());
  });

  it('moves a group’s rows when its logic changes', () => {
    const store = () => useQueryBuilderStore.getState();
    store().addConditionGroup(store().where.id, 'OR');
    const subId = store().where.groups[0]!.id;

    store().addCondition(subId, { ...cond('a', 'OR') });
    store().addCondition(subId, { ...cond('b', 'OR') });
    expect(store().where.groups[0]!.conditions.map((c) => c.conjunction)).toEqual(['OR', 'OR']);

    store().setGroupLogic(subId, 'AND');
    expect(store().where.groups[0]!.logic).toBe('AND');
    // Otherwise the control would look like it did nothing: the generator reads
    // the rows, not the group.
    expect(store().where.groups[0]!.conditions.map((c) => c.conjunction)).toEqual(['OR', 'AND']);
  });

  it('does the same for HAVING', () => {
    const store = () => useQueryBuilderStore.getState();
    store().addHavingGroup(store().having.id, 'OR');
    const subId = store().having.groups[0]!.id;
    store().addHavingCondition(subId, { ...cond('a', 'OR') });
    store().setHavingGroupLogic(subId, 'AND');
    expect(store().having.groups[0]!.conditions[0]!.conjunction).toBe('OR');
    store().addHavingCondition(subId, { ...cond('b', 'AND') });
    expect(store().having.groups[0]!.conditions.map((c) => c.conjunction)).toEqual(['OR', 'AND']);
  });
});
