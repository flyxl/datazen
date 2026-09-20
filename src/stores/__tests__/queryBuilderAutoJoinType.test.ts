/**
 * updateAutoJoinType: change the JOIN type of an unconfirmed FK candidate
 * before it is confirmed into the SQL.
 *
 * Before the fix, clicking LEFT/RIGHT/FULL on a candidate relation line had
 * no effect because handleSetGroupType only searched `joins` (confirmed) and
 * ignored `autoJoins` (candidates).
 */
import { describe, expect, it, beforeEach } from 'vitest';
import { useQueryBuilderStore } from '../queryBuilderStore';

function resetStore() {
  useQueryBuilderStore.setState(useQueryBuilderStore.getInitialState());
}

/** Seed the store with an auto-join candidate. */
function seedAutoJoin(overrides: Record<string, unknown> = {}) {
  useQueryBuilderStore.setState({
    autoJoins: [
      {
        id: 'auto-orders.customer_id-customers.id',
        type: 'INNER',
        leftTable: 'orders',
        leftColumn: 'customer_id',
        rightTable: 'customers',
        rightColumn: 'id',
        isManual: false,
        constraint: 'orders::fk_customer',
        ...overrides,
      },
    ],
  });
}

describe('updateAutoJoinType', () => {
  beforeEach(resetStore);

  it('updates the type of an auto-join by constraint key', () => {
    seedAutoJoin();
    useQueryBuilderStore.getState().updateAutoJoinType('orders::fk_customer', 'LEFT');
    const { autoJoins } = useQueryBuilderStore.getState();
    expect(autoJoins).toHaveLength(1);
    expect(autoJoins[0]!.type).toBe('LEFT');
  });

  it('does not affect other auto-joins with different constraints', () => {
    useQueryBuilderStore.setState({
      autoJoins: [
        {
          id: 'auto-a.x-b.x',
          type: 'INNER',
          leftTable: 'a',
          leftColumn: 'x',
          rightTable: 'b',
          rightColumn: 'x',
          isManual: false,
          constraint: 'a::fk1',
        },
        {
          id: 'auto-c.y-d.y',
          type: 'INNER',
          leftTable: 'c',
          leftColumn: 'y',
          rightTable: 'd',
          rightColumn: 'y',
          isManual: false,
          constraint: 'c::fk2',
        },
      ],
    });
    useQueryBuilderStore.getState().updateAutoJoinType('a::fk1', 'RIGHT');
    const { autoJoins } = useQueryBuilderStore.getState();
    expect(autoJoins[0]!.type).toBe('RIGHT');
    expect(autoJoins[1]!.type).toBe('INNER');
  });

  it('is a no-op when the constraint key does not match any auto-join', () => {
    seedAutoJoin();
    useQueryBuilderStore.getState().updateAutoJoinType('nonexistent::fk', 'FULL');
    const { autoJoins } = useQueryBuilderStore.getState();
    expect(autoJoins[0]!.type).toBe('INNER');
  });

  it('updates all pairs of a composite FK constraint', () => {
    useQueryBuilderStore.setState({
      autoJoins: [
        {
          id: 'auto-a.x-b.x',
          type: 'INNER',
          leftTable: 'a',
          leftColumn: 'x',
          rightTable: 'b',
          rightColumn: 'x',
          isManual: false,
          constraint: 'a::fk_composite',
        },
        {
          id: 'auto-a.y-b.y',
          type: 'INNER',
          leftTable: 'a',
          leftColumn: 'y',
          rightTable: 'b',
          rightColumn: 'y',
          isManual: false,
          constraint: 'a::fk_composite',
        },
      ],
    });
    useQueryBuilderStore.getState().updateAutoJoinType('a::fk_composite', 'FULL');
    const { autoJoins } = useQueryBuilderStore.getState();
    expect(autoJoins[0]!.type).toBe('FULL');
    expect(autoJoins[1]!.type).toBe('FULL');
  });

  it('preserves other auto-join fields (id, columns, etc.)', () => {
    seedAutoJoin();
    useQueryBuilderStore.getState().updateAutoJoinType('orders::fk_customer', 'LEFT');
    const join = useQueryBuilderStore.getState().autoJoins[0]!;
    expect(join.id).toBe('auto-orders.customer_id-customers.id');
    expect(join.leftTable).toBe('orders');
    expect(join.leftColumn).toBe('customer_id');
    expect(join.rightTable).toBe('customers');
    expect(join.rightColumn).toBe('id');
    expect(join.constraint).toBe('orders::fk_customer');
  });
});
