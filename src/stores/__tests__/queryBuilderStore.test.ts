import { describe, it, expect, beforeEach } from 'vitest';
import { useQueryBuilderStore } from '../queryBuilderStore';
import type {
  QbCondition,
  QbConditionGroup,
  QbJoin,
  QbJoinType,
} from '../../components/query-builder/types';

// ── Helpers ───────────────────────────────────────────────────

function getSnapshot() {
  return useQueryBuilderStore.getState();
}

function reset() {
  useQueryBuilderStore.setState(useQueryBuilderStore.getInitialState());
}

// ── Tests ─────────────────────────────────────────────────────

describe('queryBuilderStore', () => {
  beforeEach(() => {
    reset();
  });

  // ── Initial state ──────────────────────────────────────────

  describe('initial state', () => {
    it('has empty selectedTables', () => {
      expect(getSnapshot().selectedTables).toEqual([]);
    });

    it('has empty selectedColumns', () => {
      expect(getSnapshot().selectedColumns).toEqual([]);
    });

    it('has empty orderBy', () => {
      expect(getSnapshot().orderBy).toEqual([]);
    });

    it('has empty groupBy', () => {
      expect(getSnapshot().groupBy).toEqual([]);
    });

    it('has distinct false', () => {
      expect(getSnapshot().distinct).toBe(false);
    });

    it('has isOpen false', () => {
      expect(getSnapshot().isOpen).toBe(false);
    });

    it('has an empty WHERE root group', () => {
      const where = getSnapshot().where;
      expect(where.logic).toBe('AND');
      expect(where.conditions).toEqual([]);
      expect(where.groups).toEqual([]);
    });
  });

  // ── toggleTable ────────────────────────────────────────────

  describe('toggleTable', () => {
    it('adds a table when not already selected', () => {
      useQueryBuilderStore.getState().toggleTable('users');
      expect(getSnapshot().selectedTables).toEqual(['users']);
    });

    it('adds multiple tables', () => {
      useQueryBuilderStore.getState().toggleTable('users');
      useQueryBuilderStore.getState().toggleTable('orders');
      expect(getSnapshot().selectedTables).toEqual(['users', 'orders']);
    });

    it('removes a table when already selected', () => {
      useQueryBuilderStore.getState().toggleTable('users');
      useQueryBuilderStore.getState().toggleTable('users');
      expect(getSnapshot().selectedTables).toEqual([]);
    });

    it('removes a table and its columns from selectedColumns', () => {
      useQueryBuilderStore.getState().toggleTable('users');
      useQueryBuilderStore.getState().toggleColumn('users', 'id');
      useQueryBuilderStore.getState().toggleColumn('users', 'name');
      useQueryBuilderStore.getState().toggleTable('users');
      expect(getSnapshot().selectedColumns).toEqual([]);
    });

    it('does not remove columns from other tables when removing a table', () => {
      useQueryBuilderStore.getState().toggleTable('users');
      useQueryBuilderStore.getState().toggleTable('orders');
      useQueryBuilderStore.getState().toggleColumn('users', 'id');
      useQueryBuilderStore.getState().toggleColumn('orders', 'total');
      useQueryBuilderStore.getState().toggleTable('users');
      expect(getSnapshot().selectedColumns).toEqual([{ table: 'orders', column: 'total' }]);
    });
  });

  // ── toggleColumn ───────────────────────────────────────────

  describe('toggleColumn', () => {
    it('adds a column', () => {
      useQueryBuilderStore.getState().toggleColumn('users', 'id');
      expect(getSnapshot().selectedColumns).toEqual([{ table: 'users', column: 'id' }]);
    });

    it('adds multiple columns', () => {
      useQueryBuilderStore.getState().toggleColumn('users', 'id');
      useQueryBuilderStore.getState().toggleColumn('users', 'name');
      expect(getSnapshot().selectedColumns).toEqual([
        { table: 'users', column: 'id' },
        { table: 'users', column: 'name' },
      ]);
    });

    it('removes an already-selected column', () => {
      useQueryBuilderStore.getState().toggleColumn('users', 'id');
      useQueryBuilderStore.getState().toggleColumn('users', 'id');
      expect(getSnapshot().selectedColumns).toEqual([]);
    });

    it('does not affect other columns', () => {
      useQueryBuilderStore.getState().toggleColumn('users', 'id');
      useQueryBuilderStore.getState().toggleColumn('users', 'name');
      useQueryBuilderStore.getState().toggleColumn('users', 'id');
      expect(getSnapshot().selectedColumns).toEqual([{ table: 'users', column: 'name' }]);
    });
  });

  // ── setColumnAlias ─────────────────────────────────────────

  describe('setColumnAlias', () => {
    it('sets alias on an existing column', () => {
      useQueryBuilderStore.getState().toggleColumn('users', 'id');
      useQueryBuilderStore.getState().setColumnAlias('users', 'id', 'user_id');
      expect(getSnapshot().selectedColumns[0].alias).toBe('user_id');
    });

    it('clears alias when empty string is passed', () => {
      useQueryBuilderStore.getState().toggleColumn('users', 'id');
      useQueryBuilderStore.getState().setColumnAlias('users', 'id', 'user_id');
      useQueryBuilderStore.getState().setColumnAlias('users', 'id', '');
      expect(getSnapshot().selectedColumns[0].alias).toBeUndefined();
    });

    it('does not affect other columns', () => {
      useQueryBuilderStore.getState().toggleColumn('users', 'id');
      useQueryBuilderStore.getState().toggleColumn('users', 'name');
      useQueryBuilderStore.getState().setColumnAlias('users', 'id', 'uid');
      expect(getSnapshot().selectedColumns[0].alias).toBe('uid');
      expect(getSnapshot().selectedColumns[1].alias).toBeUndefined();
    });
  });

  // ── setColumnAggregate ─────────────────────────────────────

  describe('setColumnAggregate', () => {
    it('sets aggregate on an existing column', () => {
      useQueryBuilderStore.getState().toggleColumn('orders', 'total');
      useQueryBuilderStore.getState().setColumnAggregate('orders', 'total', 'SUM');
      expect(getSnapshot().selectedColumns[0].aggregate).toBe('SUM');
    });

    it('clears aggregate when undefined is passed', () => {
      useQueryBuilderStore.getState().toggleColumn('orders', 'total');
      useQueryBuilderStore.getState().setColumnAggregate('orders', 'total', 'SUM');
      useQueryBuilderStore.getState().setColumnAggregate('orders', 'total', undefined);
      expect(getSnapshot().selectedColumns[0].aggregate).toBeUndefined();
    });

    it('does not affect other columns', () => {
      useQueryBuilderStore.getState().toggleColumn('orders', 'total');
      useQueryBuilderStore.getState().toggleColumn('orders', 'id');
      useQueryBuilderStore.getState().setColumnAggregate('orders', 'total', 'COUNT');
      expect(getSnapshot().selectedColumns[0].aggregate).toBe('COUNT');
      expect(getSnapshot().selectedColumns[1].aggregate).toBeUndefined();
    });
  });

  // ── addCondition ───────────────────────────────────────────

  describe('addCondition', () => {
    it('adds a condition to the root group', () => {
      const rootId = getSnapshot().where.id;
      useQueryBuilderStore.getState().addCondition(rootId, {
        table: 'users',
        column: 'name',
        operator: '=',
        value: 'Alice',
        conjunction: 'AND',
      });
      const conditions = getSnapshot().where.conditions;
      expect(conditions).toHaveLength(1);
      expect(conditions[0].table).toBe('users');
      expect(conditions[0].column).toBe('name');
      expect(conditions[0].operator).toBe('=');
      expect(conditions[0].value).toBe('Alice');
      expect(conditions[0].id).toBeDefined();
    });

    it('generates a unique id for each condition', () => {
      const rootId = getSnapshot().where.id;
      useQueryBuilderStore.getState().addCondition(rootId, {
        table: 'users',
        column: 'id',
        operator: '=',
        value: '1',
        conjunction: 'AND',
      });
      useQueryBuilderStore.getState().addCondition(rootId, {
        table: 'users',
        column: 'id',
        operator: '=',
        value: '2',
        conjunction: 'AND',
      });
      const conditions = getSnapshot().where.conditions;
      expect(conditions).toHaveLength(2);
      expect(conditions[0].id).not.toBe(conditions[1].id);
    });

    it('adds a condition to a nested group', () => {
      const rootId = getSnapshot().where.id;
      useQueryBuilderStore.getState().addConditionGroup(rootId, 'OR');
      const subGroupId = getSnapshot().where.groups[0].id;
      useQueryBuilderStore.getState().addCondition(subGroupId, {
        table: 'users',
        column: 'role',
        operator: '=',
        value: 'admin',
        conjunction: 'OR',
      });
      expect(getSnapshot().where.groups[0].conditions).toHaveLength(1);
      expect(getSnapshot().where.conditions).toHaveLength(0);
    });
  });

  // ── updateCondition ────────────────────────────────────────

  describe('updateCondition', () => {
    it('updates a condition in the root group', () => {
      const rootId = getSnapshot().where.id;
      useQueryBuilderStore.getState().addCondition(rootId, {
        table: 'users',
        column: 'name',
        operator: '=',
        value: 'Alice',
        conjunction: 'AND',
      });
      const condId = getSnapshot().where.conditions[0].id;
      useQueryBuilderStore.getState().updateCondition(condId, { value: 'Bob' });
      expect(getSnapshot().where.conditions[0].value).toBe('Bob');
    });

    it('updates a condition in a nested group', () => {
      const rootId = getSnapshot().where.id;
      useQueryBuilderStore.getState().addConditionGroup(rootId, 'OR');
      const subGroupId = getSnapshot().where.groups[0].id;
      useQueryBuilderStore.getState().addCondition(subGroupId, {
        table: 'users',
        column: 'role',
        operator: '=',
        value: 'admin',
        conjunction: 'OR',
      });
      const condId = getSnapshot().where.groups[0].conditions[0].id;
      useQueryBuilderStore.getState().updateCondition(condId, { value: 'superadmin' });
      expect(getSnapshot().where.groups[0].conditions[0].value).toBe('superadmin');
    });

    it('can update operator', () => {
      const rootId = getSnapshot().where.id;
      useQueryBuilderStore.getState().addCondition(rootId, {
        table: 'users',
        column: 'age',
        operator: '=',
        value: '18',
        conjunction: 'AND',
      });
      const condId = getSnapshot().where.conditions[0].id;
      useQueryBuilderStore.getState().updateCondition(condId, { operator: '>' });
      expect(getSnapshot().where.conditions[0].operator).toBe('>');
    });

    it('can update multiple fields at once', () => {
      const rootId = getSnapshot().where.id;
      useQueryBuilderStore.getState().addCondition(rootId, {
        table: 'users',
        column: 'name',
        operator: '=',
        value: 'Alice',
        conjunction: 'AND',
      });
      const condId = getSnapshot().where.conditions[0].id;
      useQueryBuilderStore.getState().updateCondition(condId, {
        table: 'orders',
        column: 'total',
        operator: '>',
        value: '100',
      });
      const updated = getSnapshot().where.conditions[0];
      expect(updated.table).toBe('orders');
      expect(updated.column).toBe('total');
      expect(updated.operator).toBe('>');
      expect(updated.value).toBe('100');
    });
  });

  // ── removeCondition ────────────────────────────────────────

  describe('removeCondition', () => {
    it('removes a condition from the root group', () => {
      const rootId = getSnapshot().where.id;
      useQueryBuilderStore.getState().addCondition(rootId, {
        table: 'users',
        column: 'id',
        operator: '=',
        value: '1',
        conjunction: 'AND',
      });
      const condId = getSnapshot().where.conditions[0].id;
      useQueryBuilderStore.getState().removeCondition(condId);
      expect(getSnapshot().where.conditions).toHaveLength(0);
    });

    it('removes a condition from a nested group', () => {
      const rootId = getSnapshot().where.id;
      useQueryBuilderStore.getState().addConditionGroup(rootId, 'OR');
      const subGroupId = getSnapshot().where.groups[0].id;
      useQueryBuilderStore.getState().addCondition(subGroupId, {
        table: 'users',
        column: 'role',
        operator: '=',
        value: 'admin',
        conjunction: 'OR',
      });
      const condId = getSnapshot().where.groups[0].conditions[0].id;
      useQueryBuilderStore.getState().removeCondition(condId);
      expect(getSnapshot().where.groups[0].conditions).toHaveLength(0);
    });

    it('does nothing when id does not match any condition', () => {
      const rootId = getSnapshot().where.id;
      useQueryBuilderStore.getState().addCondition(rootId, {
        table: 'users',
        column: 'id',
        operator: '=',
        value: '1',
        conjunction: 'AND',
      });
      useQueryBuilderStore.getState().removeCondition('nonexistent-id');
      expect(getSnapshot().where.conditions).toHaveLength(1);
    });
  });

  // ── addConditionGroup ──────────────────────────────────────

  describe('addConditionGroup', () => {
    it('adds a sub-group to the root group', () => {
      const rootId = getSnapshot().where.id;
      useQueryBuilderStore.getState().addConditionGroup(rootId, 'OR');
      expect(getSnapshot().where.groups).toHaveLength(1);
      expect(getSnapshot().where.groups[0].logic).toBe('OR');
    });

    it('adds a sub-group to a nested group', () => {
      const rootId = getSnapshot().where.id;
      useQueryBuilderStore.getState().addConditionGroup(rootId, 'OR');
      const subGroupId = getSnapshot().where.groups[0].id;
      useQueryBuilderStore.getState().addConditionGroup(subGroupId, 'AND');
      expect(getSnapshot().where.groups[0].groups).toHaveLength(1);
      expect(getSnapshot().where.groups[0].groups[0].logic).toBe('AND');
    });

    it('generates unique ids for sub-groups', () => {
      const rootId = getSnapshot().where.id;
      useQueryBuilderStore.getState().addConditionGroup(rootId, 'OR');
      useQueryBuilderStore.getState().addConditionGroup(rootId, 'AND');
      const groups = getSnapshot().where.groups;
      expect(groups).toHaveLength(2);
      expect(groups[0].id).not.toBe(groups[1].id);
    });
  });

  // ── addSort / removeSort ───────────────────────────────────

  describe('addSort', () => {
    it('adds a sort item', () => {
      useQueryBuilderStore.getState().addSort({
        table: 'users',
        column: 'name',
        direction: 'ASC',
      });
      expect(getSnapshot().orderBy).toEqual([{ table: 'users', column: 'name', direction: 'ASC' }]);
    });

    it('adds multiple sort items', () => {
      useQueryBuilderStore.getState().addSort({
        table: 'users',
        column: 'name',
        direction: 'ASC',
      });
      useQueryBuilderStore.getState().addSort({
        table: 'users',
        column: 'age',
        direction: 'DESC',
      });
      expect(getSnapshot().orderBy).toHaveLength(2);
    });
  });

  describe('removeSort', () => {
    it('removes a sort item by index', () => {
      useQueryBuilderStore.getState().addSort({
        table: 'users',
        column: 'name',
        direction: 'ASC',
      });
      useQueryBuilderStore.getState().addSort({
        table: 'users',
        column: 'age',
        direction: 'DESC',
      });
      useQueryBuilderStore.getState().removeSort(0);
      expect(getSnapshot().orderBy).toEqual([{ table: 'users', column: 'age', direction: 'DESC' }]);
    });

    it('handles removing last item', () => {
      useQueryBuilderStore.getState().addSort({
        table: 'users',
        column: 'name',
        direction: 'ASC',
      });
      useQueryBuilderStore.getState().removeSort(0);
      expect(getSnapshot().orderBy).toEqual([]);
    });
  });

  // ── addGroupBy / removeGroupBy ─────────────────────────────

  describe('addGroupBy', () => {
    it('adds a group-by item', () => {
      useQueryBuilderStore.getState().addGroupBy({
        table: 'orders',
        column: 'user_id',
      });
      expect(getSnapshot().groupBy).toEqual([{ table: 'orders', column: 'user_id' }]);
    });
  });

  describe('removeGroupBy', () => {
    it('removes a group-by item by index', () => {
      useQueryBuilderStore.getState().addGroupBy({
        table: 'orders',
        column: 'user_id',
      });
      useQueryBuilderStore.getState().addGroupBy({
        table: 'orders',
        column: 'date',
      });
      useQueryBuilderStore.getState().removeGroupBy(0);
      expect(getSnapshot().groupBy).toEqual([{ table: 'orders', column: 'date' }]);
    });

    it('handles removing last item', () => {
      useQueryBuilderStore.getState().addGroupBy({
        table: 'orders',
        column: 'user_id',
      });
      useQueryBuilderStore.getState().removeGroupBy(0);
      expect(getSnapshot().groupBy).toEqual([]);
    });
  });

  // ── setDistinct ────────────────────────────────────────────

  describe('setDistinct', () => {
    it('sets distinct to true', () => {
      useQueryBuilderStore.getState().setDistinct(true);
      expect(getSnapshot().distinct).toBe(true);
    });

    it('sets distinct to false', () => {
      useQueryBuilderStore.getState().setDistinct(true);
      useQueryBuilderStore.getState().setDistinct(false);
      expect(getSnapshot().distinct).toBe(false);
    });
  });

  // ── toggleOpen ─────────────────────────────────────────────

  describe('toggleOpen', () => {
    it('toggles from false to true', () => {
      useQueryBuilderStore.getState().toggleOpen();
      expect(getSnapshot().isOpen).toBe(true);
    });

    it('toggles from true to false', () => {
      useQueryBuilderStore.getState().toggleOpen();
      useQueryBuilderStore.getState().toggleOpen();
      expect(getSnapshot().isOpen).toBe(false);
    });
  });

  // ── reset ──────────────────────────────────────────────────

  describe('reset', () => {
    it('resets all state to initial values', () => {
      const store = useQueryBuilderStore.getState();
      store.toggleTable('users');
      store.toggleColumn('users', 'id');
      store.setDistinct(true);
      store.toggleOpen();
      store.addSort({ table: 'users', column: 'name', direction: 'ASC' });
      store.addGroupBy({ table: 'users', column: 'name' });

      const rootId = getSnapshot().where.id;
      store.addCondition(rootId, {
        table: 'users',
        column: 'name',
        operator: '=',
        value: 'Alice',
        conjunction: 'AND',
      });

      useQueryBuilderStore.getState().reset();

      const s = getSnapshot();
      expect(s.selectedTables).toEqual([]);
      expect(s.selectedColumns).toEqual([]);
      expect(s.distinct).toBe(false);
      // `reset` clears the *canvas*. The builder stays open, because the header
      // "Reset" button must not close the panel under the user's cursor.
      // Closing is `closeFor()` — covered in the view-state suite below.
      expect(s.isOpen).toBe(true);
      expect(s.orderBy).toEqual([]);
      expect(s.groupBy).toEqual([]);
      expect(s.where.conditions).toEqual([]);
      expect(s.where.groups).toEqual([]);
    });

    it('resets WHERE group to a fresh group (different id)', () => {
      const oldId = getSnapshot().where.id;
      useQueryBuilderStore.getState().reset();
      expect(getSnapshot().where.id).not.toBe(oldId);
    });

    it('resets all new state fields', () => {
      const store = useQueryBuilderStore.getState();
      store.addJoin({
        type: 'INNER',
        leftTable: 'a',
        leftColumn: 'id',
        rightTable: 'b',
        rightColumn: 'a_id',
        isManual: true,
      });
      store.setTableAlias('users', 'u');
      store.updateTablePosition('users', { x: 100, y: 200 });
      store.setZoom(1.5);
      store.setLimit(100);
      store.setOffset(20);

      useQueryBuilderStore.getState().reset();

      const s = getSnapshot();
      expect(s.joins).toEqual([]);
      expect(s.autoJoins).toEqual([]);
      expect(s.tableAliases).toEqual({});
      expect(s.tablePositions).toEqual({});
      expect(s.zoom).toBe(1);
      expect(s.limit).toBeNull();
      expect(s.offset).toBeNull();
    });
  });

  // ── addJoin ─────────────────────────────────────────────────

  describe('addJoin', () => {
    it('adds a join with a generated id', () => {
      useQueryBuilderStore.getState().addJoin({
        type: 'INNER',
        leftTable: 'users',
        leftColumn: 'id',
        rightTable: 'orders',
        rightColumn: 'user_id',
        isManual: true,
      });
      const joins = getSnapshot().joins;
      expect(joins).toHaveLength(1);
      expect(joins[0].id).toBeDefined();
      expect(joins[0].type).toBe('INNER');
      expect(joins[0].leftTable).toBe('users');
      expect(joins[0].leftColumn).toBe('id');
      expect(joins[0].rightTable).toBe('orders');
      expect(joins[0].rightColumn).toBe('user_id');
      expect(joins[0].isManual).toBe(true);
    });

    it('adds multiple joins with unique ids', () => {
      const store = useQueryBuilderStore.getState();
      store.addJoin({
        type: 'INNER',
        leftTable: 'a',
        leftColumn: 'id',
        rightTable: 'b',
        rightColumn: 'a_id',
        isManual: true,
      });
      store.addJoin({
        type: 'LEFT',
        leftTable: 'b',
        leftColumn: 'id',
        rightTable: 'c',
        rightColumn: 'b_id',
        isManual: false,
      });
      const joins = getSnapshot().joins;
      expect(joins).toHaveLength(2);
      expect(joins[0].id).not.toBe(joins[1].id);
    });
  });

  // ── removeJoin ──────────────────────────────────────────────

  describe('removeJoin', () => {
    it('removes a join by id', () => {
      const store = useQueryBuilderStore.getState();
      store.addJoin({
        type: 'INNER',
        leftTable: 'a',
        leftColumn: 'id',
        rightTable: 'b',
        rightColumn: 'a_id',
        isManual: true,
      });
      const joinId = getSnapshot().joins[0].id;
      store.removeJoin(joinId);
      expect(getSnapshot().joins).toEqual([]);
    });

    it('does nothing when id does not match', () => {
      useQueryBuilderStore.getState().addJoin({
        type: 'INNER',
        leftTable: 'a',
        leftColumn: 'id',
        rightTable: 'b',
        rightColumn: 'a_id',
        isManual: true,
      });
      useQueryBuilderStore.getState().removeJoin('nonexistent-id');
      expect(getSnapshot().joins).toHaveLength(1);
    });
  });

  // ── updateJoinType ──────────────────────────────────────────

  describe('updateJoinType', () => {
    it('updates the join type', () => {
      const store = useQueryBuilderStore.getState();
      store.addJoin({
        type: 'INNER',
        leftTable: 'a',
        leftColumn: 'id',
        rightTable: 'b',
        rightColumn: 'a_id',
        isManual: true,
      });
      const joinId = getSnapshot().joins[0].id;
      store.updateJoinType(joinId, 'LEFT');
      expect(getSnapshot().joins[0].type).toBe('LEFT');
    });

    it('does nothing when id does not match', () => {
      useQueryBuilderStore.getState().addJoin({
        type: 'INNER',
        leftTable: 'a',
        leftColumn: 'id',
        rightTable: 'b',
        rightColumn: 'a_id',
        isManual: true,
      });
      useQueryBuilderStore.getState().updateJoinType('nonexistent-id', 'RIGHT');
      expect(getSnapshot().joins[0].type).toBe('INNER');
    });
  });

  // ── setTableAlias ───────────────────────────────────────────

  describe('setTableAlias', () => {
    it('sets an alias for a table', () => {
      useQueryBuilderStore.getState().setTableAlias('users', 'u');
      expect(getSnapshot().tableAliases).toEqual({ users: 'u' });
    });

    it('overwrites an existing alias', () => {
      useQueryBuilderStore.getState().setTableAlias('users', 'u');
      useQueryBuilderStore.getState().setTableAlias('users', 'usr');
      expect(getSnapshot().tableAliases).toEqual({ users: 'usr' });
    });
  });

  // ── updateTablePosition ─────────────────────────────────────

  describe('updateTablePosition', () => {
    it('sets position for a table', () => {
      useQueryBuilderStore.getState().updateTablePosition('users', { x: 100, y: 200 });
      expect(getSnapshot().tablePositions).toEqual({ users: { x: 100, y: 200 } });
    });

    it('updates position for a table', () => {
      useQueryBuilderStore.getState().updateTablePosition('users', { x: 100, y: 200 });
      useQueryBuilderStore.getState().updateTablePosition('users', { x: 150, y: 250 });
      expect(getSnapshot().tablePositions).toEqual({ users: { x: 150, y: 250 } });
    });
  });

  // ── setZoom ─────────────────────────────────────────────────

  describe('setZoom', () => {
    it('sets zoom level', () => {
      useQueryBuilderStore.getState().setZoom(1.5);
      expect(getSnapshot().zoom).toBe(1.5);
    });
  });

  // ── setLimit / setOffset ────────────────────────────────────

  describe('setLimit', () => {
    it('sets limit', () => {
      useQueryBuilderStore.getState().setLimit(100);
      expect(getSnapshot().limit).toBe(100);
    });

    it('clears limit to null', () => {
      useQueryBuilderStore.getState().setLimit(100);
      useQueryBuilderStore.getState().setLimit(null);
      expect(getSnapshot().limit).toBeNull();
    });
  });

  describe('setOffset', () => {
    it('sets offset', () => {
      useQueryBuilderStore.getState().setOffset(20);
      expect(getSnapshot().offset).toBe(20);
    });

    it('clears offset to null', () => {
      useQueryBuilderStore.getState().setOffset(20);
      useQueryBuilderStore.getState().setOffset(null);
      expect(getSnapshot().offset).toBeNull();
    });
  });

  // ── updateColumnConfig ──────────────────────────────────────

  describe('updateColumnConfig', () => {
    it('patches alias on a column', () => {
      useQueryBuilderStore.getState().toggleColumn('users', 'id');
      useQueryBuilderStore.getState().updateColumnConfig('users', 'id', { alias: 'uid' });
      expect(getSnapshot().selectedColumns[0].alias).toBe('uid');
    });

    it('patches aggregate on a column', () => {
      useQueryBuilderStore.getState().toggleColumn('orders', 'total');
      useQueryBuilderStore.getState().updateColumnConfig('orders', 'total', { aggregate: 'SUM' });
      expect(getSnapshot().selectedColumns[0].aggregate).toBe('SUM');
    });

    it('patches sort on a column', () => {
      useQueryBuilderStore.getState().toggleColumn('users', 'name');
      useQueryBuilderStore.getState().updateColumnConfig('users', 'name', { sort: 'ASC' });
      expect(getSnapshot().selectedColumns[0].sort).toBe('ASC');
    });

    it('patches groupBy on a column', () => {
      useQueryBuilderStore.getState().toggleColumn('users', 'id');
      useQueryBuilderStore.getState().updateColumnConfig('users', 'id', { groupBy: true });
      expect(getSnapshot().selectedColumns[0].groupBy).toBe(true);
    });

    it('clears alias when empty string is passed', () => {
      useQueryBuilderStore.getState().toggleColumn('users', 'id');
      useQueryBuilderStore.getState().updateColumnConfig('users', 'id', { alias: 'uid' });
      useQueryBuilderStore.getState().updateColumnConfig('users', 'id', { alias: '' });
      expect(getSnapshot().selectedColumns[0].alias).toBeUndefined();
    });

    it('does not affect other columns', () => {
      useQueryBuilderStore.getState().toggleColumn('users', 'id');
      useQueryBuilderStore.getState().toggleColumn('users', 'name');
      useQueryBuilderStore.getState().updateColumnConfig('users', 'id', { alias: 'uid' });
      expect(getSnapshot().selectedColumns[0].alias).toBe('uid');
      expect(getSnapshot().selectedColumns[1].alias).toBeUndefined();
    });
  });
});

// ── View state: panel-scoped open, snapshot rollback, tabs ─────

describe('queryBuilderStore view state', () => {
  beforeEach(() => {
    reset();
  });

  describe('openFor / closeFor', () => {
    it('opens scoped to a panel with a clean canvas', () => {
      // The builder belongs to the panel that opens it: a panel that never
      // owned a canvas must not inherit one from earlier activity.
      const s = useQueryBuilderStore.getState();
      s.toggleTable('users');
      s.openFor('panel-a');
      const after = getSnapshot();
      expect(after.isOpen).toBe(true);
      expect(after.openPanelId).toBe('panel-a');
      expect(after.entrySnapshot?.selectedTables).toEqual([]);
      expect(after.selectedTables).toEqual([]);
      expect(after.hasChanges()).toBe(false);
    });

    it('keeps the original snapshot when reopened for the same panel', () => {
      const s = useQueryBuilderStore.getState();
      s.openFor('panel-a');
      s.toggleTable('users');
      // Re-render / re-open for the same panel must not rebase the baseline.
      useQueryBuilderStore.getState().openFor('panel-a');
      expect(getSnapshot().entrySnapshot?.selectedTables).toEqual([]);
      expect(getSnapshot().hasChanges()).toBe(true);
    });

    it('keeps each panel’s canvas separate', () => {
      const s = useQueryBuilderStore.getState();
      s.openFor('panel-a');
      s.toggleTable('users');
      s.setBottomTab('preview');

      // Switching panels swaps in panel-b's own (still empty) canvas …
      useQueryBuilderStore.getState().openFor('panel-b');
      expect(getSnapshot().openPanelId).toBe('panel-b');
      expect(getSnapshot().selectedTables).toEqual([]);
      expect(getSnapshot().bottomTab).toBe('build');

      // … and stashes panel-a's, so coming back restores it verbatim.
      useQueryBuilderStore.getState().openFor('panel-a');
      expect(getSnapshot().selectedTables).toEqual(['users']);
      expect(getSnapshot().bottomTab).toBe('preview');
    });

    it('closeFor(ok) keeps the canvas', () => {
      const s = useQueryBuilderStore.getState();
      s.openFor('panel-a');
      s.toggleTable('users');
      useQueryBuilderStore.getState().closeFor('ok');
      const after = getSnapshot();
      expect(after.isOpen).toBe(false);
      expect(after.openPanelId).toBeNull();
      expect(after.entrySnapshot).toBeNull();
      expect(after.selectedTables).toEqual(['users']);
    });

    it('reopening the same panel restores the canvas it closed with', () => {
      const s = useQueryBuilderStore.getState();
      s.openFor('panel-a');
      s.toggleTable('users');
      s.toggleColumn('users', 'id');
      useQueryBuilderStore.getState().closeFor('ok');

      useQueryBuilderStore.getState().openFor('panel-a');
      expect(getSnapshot().selectedTables).toEqual(['users']);
      expect(getSnapshot().selectedColumns).toHaveLength(1);
    });

    it('closeFor(cancel) rolls the canvas back to the entry snapshot', () => {
      const s = useQueryBuilderStore.getState();
      // Seed a canvas, commit it away and re-open: the restored canvas is the
      // baseline this session rolls back to.
      s.openFor('panel-a');
      s.toggleTable('users');
      s.toggleColumn('users', 'id');
      useQueryBuilderStore.getState().closeFor('ok');
      useQueryBuilderStore.getState().openFor('panel-a');

      const after1 = useQueryBuilderStore.getState();
      after1.toggleTable('orders');
      after1.setDistinct(true);
      after1.setLimit(50);
      expect(getSnapshot().hasChanges()).toBe(true);

      useQueryBuilderStore.getState().closeFor('cancel');
      const after = getSnapshot();
      expect(after.isOpen).toBe(false);
      expect(after.selectedTables).toEqual(['users']);
      expect(after.selectedColumns).toHaveLength(1);
      expect(after.distinct).toBe(false);
      expect(after.limit).toBeNull();
    });

    it('hideFor keeps the canvas but clears the open state', () => {
      const s = useQueryBuilderStore.getState();
      s.openFor('panel-a');
      s.toggleTable('users');
      useQueryBuilderStore.getState().hideFor();
      expect(getSnapshot().isOpen).toBe(false);
      expect(getSnapshot().openPanelId).toBeNull();
      expect(getSnapshot().selectedTables).toEqual(['users']);
      // …and it comes back on reopen (hide is not a cancel).
      useQueryBuilderStore.getState().openFor('panel-a');
      expect(getSnapshot().selectedTables).toEqual(['users']);
    });

    it('two panels do not share the open state', () => {
      useQueryBuilderStore.getState().openFor('panel-a');
      expect(getSnapshot().openPanelId).toBe('panel-a');
      useQueryBuilderStore.getState().openFor('panel-b');
      expect(getSnapshot().openPanelId).toBe('panel-b');
    });

    it('destroyFor blanks the builder of a destroyed panel', () => {
      const s = useQueryBuilderStore.getState();
      s.openFor('panel-a');
      s.toggleTable('users');
      s.setBottomTab('preview');

      useQueryBuilderStore.getState().destroyFor('panel-a');
      const after = getSnapshot();
      expect(after.isOpen).toBe(false);
      expect(after.openPanelId).toBeNull();
      expect(after.selectedTables).toEqual([]);
      expect(after.bottomTab).toBe('build');

      // A later panel must not resurrect the destroyed panel's canvas either.
      useQueryBuilderStore.getState().openFor('panel-b');
      expect(getSnapshot().selectedTables).toEqual([]);
      useQueryBuilderStore.getState().openFor('panel-a');
      expect(getSnapshot().selectedTables).toEqual([]);
    });

    it('destroyFor of an unrelated panel leaves the live builder alone', () => {
      const s = useQueryBuilderStore.getState();
      s.openFor('panel-a');
      s.toggleTable('users');
      useQueryBuilderStore.getState().destroyFor('panel-b');
      expect(getSnapshot().openPanelId).toBe('panel-a');
      expect(getSnapshot().selectedTables).toEqual(['users']);
    });
  });

  describe('hasChanges', () => {
    it('is false right after opening', () => {
      useQueryBuilderStore.getState().openFor('panel-a');
      expect(getSnapshot().hasChanges()).toBe(false);
    });

    it('is false without an entry snapshot', () => {
      useQueryBuilderStore.getState().toggleTable('users');
      expect(getSnapshot().hasChanges()).toBe(false);
    });

    it('detects a column toggle', () => {
      const s = useQueryBuilderStore.getState();
      s.toggleTable('users');
      s.openFor('panel-a');
      useQueryBuilderStore.getState().toggleColumn('users', 'id');
      expect(getSnapshot().hasChanges()).toBe(true);
    });

    it('detects a table alias edit', () => {
      const s = useQueryBuilderStore.getState();
      s.toggleTable('users');
      s.openFor('panel-a');
      useQueryBuilderStore.getState().setTableAlias('users', 'u');
      expect(getSnapshot().hasChanges()).toBe(true);
    });

    it('does not treat view toggles as changes', () => {
      useQueryBuilderStore.getState().openFor('panel-a');
      useQueryBuilderStore.getState().setBottomTab('preview');
      useQueryBuilderStore.getState().setCanvasCollapsed(true);
      expect(getSnapshot().hasChanges()).toBe(false);
    });
  });

  describe('bottom tab + canvas collapse', () => {
    it('defaults to the build tab with the canvas expanded', () => {
      expect(getSnapshot().bottomTab).toBe('build');
      expect(getSnapshot().canvasCollapsed).toBe(false);
    });

    it('switches tabs', () => {
      useQueryBuilderStore.getState().setBottomTab('preview');
      expect(getSnapshot().bottomTab).toBe('preview');
      useQueryBuilderStore.getState().setBottomTab('build');
      expect(getSnapshot().bottomTab).toBe('build');
    });

    it('toggles the canvas collapse flag', () => {
      useQueryBuilderStore.getState().toggleCanvasCollapsed();
      expect(getSnapshot().canvasCollapsed).toBe(true);
      useQueryBuilderStore.getState().toggleCanvasCollapsed();
      expect(getSnapshot().canvasCollapsed).toBe(false);
    });

    it('survives a canvas reset', () => {
      const s = useQueryBuilderStore.getState();
      s.openFor('panel-a');
      s.setBottomTab('preview');
      s.setCanvasCollapsed(true);
      useQueryBuilderStore.getState().reset();
      expect(getSnapshot().bottomTab).toBe('preview');
      expect(getSnapshot().canvasCollapsed).toBe(true);
      expect(getSnapshot().openPanelId).toBe('panel-a');
    });
  });
});

// ── Default aliases, table removal, select-all ─────────────────

describe('queryBuilderStore table lifecycle', () => {
  beforeEach(() => {
    reset();
  });

  describe('default aliases', () => {
    it('assigns an alias as soon as a table is added', () => {
      useQueryBuilderStore.getState().toggleTable('actor');
      expect(getSnapshot().tableAliases.actor).toBe('a');
    });

    it('derives initials for multi-word names', () => {
      useQueryBuilderStore.getState().toggleTable('film_actor');
      expect(getSnapshot().tableAliases.film_actor).toBe('fa');
    });

    it('keeps aliases unique', () => {
      const s = useQueryBuilderStore.getState();
      s.toggleTable('actor');
      s.toggleTable('address');
      const aliases = getSnapshot().tableAliases;
      expect(aliases.actor).toBe('a');
      expect(aliases.address).not.toBe('a');
      expect(new Set(Object.values(aliases)).size).toBe(2);
    });

    it('does not reassign an alias the user has edited', () => {
      const s = useQueryBuilderStore.getState();
      s.toggleTable('actor');
      s.setTableAlias('actor', 'my_alias');
      // Remove + re-add must not silently overwrite the curated alias.
      useQueryBuilderStore.getState().toggleTable('film');
      expect(getSnapshot().tableAliases.actor).toBe('my_alias');
    });

    it('drops the alias when the table is removed', () => {
      useQueryBuilderStore.getState().toggleTable('actor');
      useQueryBuilderStore.getState().removeTable('actor');
      expect(getSnapshot().tableAliases.actor).toBeUndefined();
    });
  });

  describe('removeTable', () => {
    it('removes the table, its columns, its joins and its position', () => {
      const s = useQueryBuilderStore.getState();
      s.toggleTable('users');
      s.toggleTable('orders');
      s.toggleColumn('users', 'id');
      s.toggleColumn('orders', 'total');
      s.updateTablePosition('orders', { x: 100, y: 200 });
      useQueryBuilderStore.getState().addJoin({
        type: 'INNER',
        leftTable: 'users',
        leftColumn: 'id',
        rightTable: 'orders',
        rightColumn: 'user_id',
        isManual: true,
      });

      useQueryBuilderStore.getState().removeTable('orders');

      const after = getSnapshot();
      expect(after.selectedTables).toEqual(['users']);
      expect(after.selectedColumns.map((c) => c.table)).toEqual(['users']);
      expect(after.joins).toHaveLength(0);
      expect(after.tablePositions.orders).toBeUndefined();
      expect(after.tableAliases.orders).toBeUndefined();
    });

    it('drops conditions that reference the removed table, including nested ones', () => {
      const s = useQueryBuilderStore.getState();
      s.toggleTable('users');
      s.toggleTable('orders');
      const rootId = getSnapshot().where.id;
      s.addCondition(rootId, {
        table: 'users',
        column: 'name',
        operator: '=',
        value: 'Alice',
        conjunction: 'AND',
      });
      useQueryBuilderStore.getState().addConditionGroup(rootId, 'OR');
      const subGroupId = getSnapshot().where.groups[0]!.id;
      useQueryBuilderStore.getState().addCondition(subGroupId, {
        table: 'orders',
        column: 'total',
        operator: '>',
        value: '10',
        conjunction: 'AND',
      });

      useQueryBuilderStore.getState().removeTable('orders');

      const after = getSnapshot();
      expect(after.where.conditions).toHaveLength(1);
      expect(after.where.groups[0]!.conditions).toHaveLength(0);
    });

    it('also drops auto-detected candidates that touch the table', () => {
      const s = useQueryBuilderStore.getState();
      s.toggleTable('users');
      s.toggleTable('orders');
      useQueryBuilderStore.setState({
        autoJoins: [
          {
            id: 'auto-1',
            type: 'INNER',
            leftTable: 'orders',
            leftColumn: 'user_id',
            rightTable: 'users',
            rightColumn: 'id',
            isManual: false,
          },
        ],
      });

      useQueryBuilderStore.getState().removeTable('users');
      expect(getSnapshot().autoJoins).toHaveLength(0);
    });

    it('is a no-op for an unknown table', () => {
      const s = useQueryBuilderStore.getState();
      s.toggleTable('users');
      useQueryBuilderStore.getState().removeTable('nope');
      expect(getSnapshot().selectedTables).toEqual(['users']);
    });
  });

  describe('setAllColumns', () => {
    it('selects every column of one table only', () => {
      const s = useQueryBuilderStore.getState();
      s.toggleTable('users');
      s.toggleTable('orders');
      s.toggleColumn('orders', 'total');

      useQueryBuilderStore.getState().setAllColumns('users', ['id', 'name'], true);

      const cols = getSnapshot().selectedColumns;
      expect(cols.filter((c) => c.table === 'users').map((c) => c.column)).toEqual(['id', 'name']);
      expect(cols.filter((c) => c.table === 'orders').map((c) => c.column)).toEqual(['total']);
    });

    it('clears every column of the table when unchecked', () => {
      const s = useQueryBuilderStore.getState();
      s.toggleTable('users');
      useQueryBuilderStore.getState().setAllColumns('users', ['id', 'name'], true);
      useQueryBuilderStore.getState().setAllColumns('users', ['id', 'name'], false);
      expect(getSnapshot().selectedColumns).toHaveLength(0);
    });

    it('is idempotent', () => {
      const s = useQueryBuilderStore.getState();
      s.toggleTable('users');
      useQueryBuilderStore.getState().setAllColumns('users', ['id', 'name'], true);
      useQueryBuilderStore.getState().setAllColumns('users', ['id', 'name'], true);
      expect(getSnapshot().selectedColumns).toHaveLength(2);
    });
  });
});
