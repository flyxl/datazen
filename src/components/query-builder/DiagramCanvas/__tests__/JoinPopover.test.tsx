/**
 * Join popover interactions (regression for "buttons do nothing"):
 * clicking a relation opens the popover; the type buttons and the
 * confirm/remove action must reach the canvas callbacks.
 */
import { afterEach, describe, expect, it, vi } from 'vitest';
import { cleanup, fireEvent, render, screen } from '@testing-library/react';
import { DiagramCanvas } from '../DiagramCanvas';
import type { RelationGroup } from '../fkGeometry';

vi.mock('../../../../hooks/useI18n', () => ({
  useI18n: () => ({ t: (key: string) => key }),
}));

// jsdom has no scrollIntoView; the canvas calls it when a relation activates.
Element.prototype.scrollIntoView = () => {};

const GROUPS: RelationGroup[] = [
  {
    id: 'er_orders_customer_id_fkey',
    kind: 'fk',
    type: 'INNER',
    constraint: 'er_orders_customer_id_fkey',
    pairs: [
      {
        fromTable: 'er_customers',
        fromColumn: 'id',
        toTable: 'er_orders',
        toColumn: 'customer_id',
        confirmed: false,
      },
    ],
  },
];

function renderCanvas() {
  const handlers = {
    onSetGroupType: vi.fn(),
    onConfirmGroup: vi.fn(),
    onRemoveGroup: vi.fn(),
    onToggleColumn: vi.fn(),
    onUpdatePosition: vi.fn(),
    onSetTableAlias: vi.fn(),
  };
  render(
    <DiagramCanvas
      selectedTables={['er_customers', 'er_orders']}
      tablePositions={{
        er_customers: { x: 24, y: 24 },
        er_orders: { x: 288, y: 24 },
      }}
      relationGroups={GROUPS}
      columnMap={{
        er_customers: ['id', 'name'],
        er_orders: ['id', 'customer_id', 'ordered_at'],
      }}
      columnInfoMap={{}}
      selectedColumns={[]}
      tableAliases={{}}
      {...handlers}
    />,
  );
  return handlers;
}

afterEach(cleanup);

describe('Join popover', () => {
  it('opens when the relation line is clicked', () => {
    renderCanvas();
    fireEvent.click(screen.getByTestId('qb-relation-hit-er_orders_customer_id_fkey'));
    expect(screen.getByTestId('qb-join-popover')).toBeTruthy();
  });

  it('calls onSetGroupType when a join-type button is clicked', () => {
    const { onSetGroupType } = renderCanvas();
    fireEvent.click(screen.getByTestId('qb-relation-hit-er_orders_customer_id_fkey'));
    fireEvent.click(screen.getByTestId('qb-join-type-LEFT'));
    expect(onSetGroupType).toHaveBeenCalledWith('er_orders_customer_id_fkey', 'LEFT');
  });

  it('calls onConfirmGroup when the confirm button is clicked and closes', () => {
    const { onConfirmGroup } = renderCanvas();
    fireEvent.click(screen.getByTestId('qb-relation-hit-er_orders_customer_id_fkey'));
    fireEvent.click(screen.getByTestId('qb-join-confirm'));
    expect(onConfirmGroup).toHaveBeenCalledWith('er_orders_customer_id_fkey');
    expect(screen.queryByTestId('qb-join-popover')).toBeNull();
  });

  it('stays open when a type button is clicked (only confirm/remove/close dismiss)', () => {
    renderCanvas();
    fireEvent.click(screen.getByTestId('qb-relation-hit-er_orders_customer_id_fkey'));
    fireEvent.click(screen.getByTestId('qb-join-type-FULL'));
    expect(screen.getByTestId('qb-join-popover')).toBeTruthy();
  });
});
