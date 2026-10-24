import { describe, expect, it, vi } from 'vitest';
import { render, fireEvent } from '@testing-library/react';
import { TableHeader } from '../TableHeader';
import type { SortCondition } from '../../../types';

const COLS = [
  { id: 'id', name: 'id', type: 'bigint' },
  { id: 'name', name: 'name', type: 'varchar' },
  { id: 'price', name: 'price', type: 'numeric' },
];

describe('TableHeader sort icon visibility', () => {
  it('hides sort icon for unsorted columns', () => {
    const { container } = render(
      <TableHeader columns={COLS} sorts={[]} onSort={() => {}} sortable />,
    );
    const sortIcons = container.querySelectorAll('svg');
    // No sort icons should be visible when nothing is sorted
    expect(sortIcons.length).toBe(0);
  });

  it('shows sort icon only for the actively sorted column', () => {
    const sorts: SortCondition[] = [{ column: 'name', descending: false }];
    const { container } = render(
      <TableHeader columns={COLS} sorts={sorts} onSort={() => {}} sortable />,
    );
    const sortIcons = container.querySelectorAll('svg');
    // Only one sort icon (ArrowUp for asc)
    expect(sortIcons.length).toBe(1);
  });

  it('triggers onSort when column header is double-clicked', () => {
    const onSort = vi.fn();
    const { container } = render(
      <TableHeader columns={COLS} sorts={[]} onSort={onSort} sortable />,
    );
    // Double-click the "name" column header area
    const headers = container.querySelectorAll('[data-col-header]');
    fireEvent.doubleClick(headers[1]); // "name" column
    expect(onSort).toHaveBeenCalledWith({ column: 'name', descending: false });
  });

  it('does not trigger sort on single click', () => {
    const onSort = vi.fn();
    const { container } = render(
      <TableHeader columns={COLS} sorts={[]} onSort={onSort} sortable />,
    );
    const headers = container.querySelectorAll('[data-col-header]');
    fireEvent.click(headers[1]);
    expect(onSort).not.toHaveBeenCalled();
  });

  it('text container fills available space when sort icon is hidden', () => {
    const { container } = render(
      <TableHeader columns={COLS} sorts={[]} onSort={() => {}} sortable />,
    );
    const headers = container.querySelectorAll('[data-col-header]');
    const textContainer = headers[0].querySelector('[data-col-label]')!;
    expect(textContainer.className).toContain('flex-1');
  });

  it('text container fills remaining space when sort icon is shown', () => {
    const sorts: SortCondition[] = [{ column: 'id', descending: false }];
    const { container } = render(
      <TableHeader columns={COLS} sorts={sorts} onSort={() => {}} sortable />,
    );
    const headers = container.querySelectorAll('[data-col-header]');
    const textContainer = headers[0].querySelector('[data-col-label]')!;
    expect(textContainer.className).toContain('flex-1');
  });

  it('single-clicking the sort icon toggles sort direction', () => {
    const onSort = vi.fn();
    const sorts: SortCondition[] = [{ column: 'name', descending: false }];
    const { container } = render(
      <TableHeader columns={COLS} sorts={sorts} onSort={onSort} sortable />,
    );
    const icon = container.querySelector('[data-sort-icon]')!;
    fireEvent.click(icon);
    expect(onSort).toHaveBeenCalledWith({ column: 'name', descending: true });
  });

  it('single-clicking desc sort icon toggles back to asc', () => {
    const onSort = vi.fn();
    const sorts: SortCondition[] = [{ column: 'price', descending: true }];
    const { container } = render(
      <TableHeader columns={COLS} sorts={sorts} onSort={onSort} sortable />,
    );
    const icon = container.querySelector('[data-sort-icon]')!;
    fireEvent.click(icon);
    expect(onSort).toHaveBeenCalledWith({ column: 'price', descending: false });
  });
});

describe('TableHeader multi-column sort badges (DB-11)', () => {
  it('shows no badge when only one column is sorted', () => {
    const { container } = render(
      <TableHeader
        columns={COLS}
        sorts={[{ column: 'name', descending: false }]}
        onSort={() => {}}
        sortable
      />,
    );
    expect(container.querySelectorAll('[data-sort-badge]').length).toBe(0);
  });

  it('badges every sorted column with its position in the sort list', () => {
    const sorts: SortCondition[] = [
      { column: 'price', descending: true },
      { column: 'name', descending: false },
    ];
    const { container } = render(
      <TableHeader columns={COLS} sorts={sorts} onSort={() => {}} sortable />,
    );
    const badges = [...container.querySelectorAll('[data-sort-badge]')];
    expect(badges.length).toBe(2);
    // The badge is the column's position in the list, 1-based — the list
    // order is the ORDER BY order.
    expect(badges[0].textContent).toBe('2'); // name, second in the list
    expect(badges[1].textContent).toBe('1'); // price, first in the list
    // Both sorted columns show their arrow, in their own direction.
    const icons = container.querySelectorAll('[data-sort-icon]');
    expect(icons.length).toBe(2);
  });

  it('shows the direction of the second sorted column, not only the first', () => {
    const sorts: SortCondition[] = [
      { column: 'name', descending: false },
      { column: 'price', descending: true },
    ];
    const { container } = render(
      <TableHeader columns={COLS} sorts={sorts} onSort={() => {}} sortable />,
    );
    // The pre-DB-11 header read only sorts[0]; the second column's arrow
    // must follow its own direction from the list.
    const headers = container.querySelectorAll('[data-col-header]');
    const priceHeader = [...headers].find(
      (header) => header.getAttribute('data-col-header') === 'price',
    )!;
    const priceIcon = priceHeader.querySelector('[data-sort-icon]')!;
    expect(priceIcon.querySelector('.lucide-arrow-down')).toBeTruthy();
    expect(priceIcon.querySelector('.lucide-arrow-up')).toBeNull();
  });

  it('double-clicking an unsorted column still offers the single-sort replacement', () => {
    const onSort = vi.fn();
    const sorts: SortCondition[] = [
      { column: 'name', descending: false },
      { column: 'price', descending: true },
    ];
    const { container } = render(
      <TableHeader columns={COLS} sorts={sorts} onSort={onSort} sortable />,
    );
    const headers = container.querySelectorAll('[data-col-header]');
    fireEvent.doubleClick(headers[0]); // "id" — unsorted
    expect(onSort).toHaveBeenCalledWith({ column: 'id', descending: false });
  });
});
