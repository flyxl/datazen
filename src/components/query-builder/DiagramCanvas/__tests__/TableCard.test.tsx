/**
 * Table card interactions: drag-to-reposition, remove, and select-all.
 *
 * All three were reported as missing/broken from the canvas: cards could not be
 * moved, a wrongly chosen table could only be removed by closing the whole
 * builder, and there was no way to select every column at once.
 */
import { afterEach, describe, expect, it, vi } from 'vitest';
import { cleanup, fireEvent, render, screen } from '@testing-library/react';
import { TableCard } from '../TableCard';

vi.mock('../../../../hooks/useI18n', () => ({
  useI18n: () => ({ t: (key: string) => key }),
}));

const COLUMNS = [
  { name: 'id', dataType: 'int', nullable: false },
  { name: 'name', dataType: 'text', nullable: true },
];

function renderCard(overrides: Partial<Parameters<typeof TableCard>[0]> = {}) {
  const handlers = {
    onToggleColumn: vi.fn(),
    onToggleAllColumns: vi.fn(),
    onRemove: vi.fn(),
    onDragEnd: vi.fn(),
    onSetAlias: vi.fn(),
  };
  render(
    <TableCard
      tableName="users"
      alias="u"
      columns={COLUMNS}
      selectedColumns={[]}
      position={{ x: 48, y: 24 }}
      {...handlers}
      {...overrides}
    />,
  );
  return handlers;
}

afterEach(cleanup);

describe('TableCard drag', () => {
  it('reports a grid-snapped position while dragging the card body', () => {
    const { onDragEnd } = renderCard();
    const card = screen.getByTestId('qb-drag-users');

    fireEvent.pointerDown(card, { pointerId: 1, button: 0, clientX: 100, clientY: 100 });
    fireEvent.pointerMove(card, { pointerId: 1, clientX: 180, clientY: 150 });

    expect(onDragEnd).toHaveBeenCalled();
    const pos = onDragEnd.mock.calls.at(-1)![0] as { x: number; y: number };
    // +80 / +50 from (48, 24) → snapped to the 24px grid.
    expect(pos.x % 24).toBe(0);
    expect(pos.y % 24).toBe(0);
    expect(pos.x).toBeGreaterThan(48);
    expect(pos.y).toBeGreaterThan(24);
  });

  it('does not start a drag from an interactive control', () => {
    const { onDragEnd } = renderCard();
    const aliasInput = screen.getByTestId('qb-alias-users');

    fireEvent.pointerDown(aliasInput, { pointerId: 1, button: 0, clientX: 10, clientY: 10 });
    fireEvent.pointerMove(aliasInput, { pointerId: 1, clientX: 200, clientY: 200 });

    expect(onDragEnd).not.toHaveBeenCalled();
  });

  it('ignores a right-click drag', () => {
    const { onDragEnd } = renderCard();
    const card = screen.getByTestId('qb-drag-users');

    fireEvent.pointerDown(card, { pointerId: 1, button: 2, clientX: 10, clientY: 10 });
    fireEvent.pointerMove(card, { pointerId: 1, clientX: 200, clientY: 200 });

    expect(onDragEnd).not.toHaveBeenCalled();
  });
});

describe('TableCard remove', () => {
  it('exposes a remove control that reports the table', () => {
    const { onRemove } = renderCard();
    fireEvent.click(screen.getByTestId('qb-remove-users'));
    expect(onRemove).toHaveBeenCalledTimes(1);
  });

  it('removing does not start a card drag', () => {
    const { onDragEnd } = renderCard();
    const remove = screen.getByTestId('qb-remove-users');
    fireEvent.pointerDown(remove, { pointerId: 1, button: 0, clientX: 5, clientY: 5 });
    fireEvent.pointerMove(remove, { pointerId: 1, clientX: 300, clientY: 300 });
    expect(onDragEnd).not.toHaveBeenCalled();
  });
});

describe('TableCard select-all', () => {
  it('selects every column at once', () => {
    const { onToggleAllColumns } = renderCard({ selectedColumns: [] });
    fireEvent.click(screen.getByTestId('qb-selectall-users'));
    expect(onToggleAllColumns).toHaveBeenCalledWith(true);
  });

  it('clears every column when they are all selected', () => {
    const { onToggleAllColumns } = renderCard({ selectedColumns: ['id', 'name'] });
    const all = screen.getByTestId('qb-selectall-users') as HTMLInputElement;
    expect(all.checked).toBe(true);

    fireEvent.click(all);
    expect(onToggleAllColumns).toHaveBeenCalledWith(false);
  });

  it('shows an indeterminate state for a partial selection', () => {
    renderCard({ selectedColumns: ['id'] });
    const all = screen.getByTestId('qb-selectall-users') as HTMLInputElement;
    expect(all.checked).toBe(false);
    expect(all.indeterminate).toBe(true);
  });

  it('is not checked when the card has no columns loaded yet', () => {
    renderCard({ columns: [], selectedColumns: [] });
    const all = screen.getByTestId('qb-selectall-users') as HTMLInputElement;
    expect(all.checked).toBe(false);
  });
});
