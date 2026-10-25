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

describe('TableHeader drag reorder (DB-12)', () => {
  it('fires onReorderColumns when a header is dragged onto another one', () => {
    const onReorderColumns = vi.fn();
    const { container } = render(
      <TableHeader
        columns={COLS}
        sorts={[]}
        onSort={() => {}}
        sortable
        onReorderColumns={onReorderColumns}
      />,
    );
    const headers = container.querySelectorAll('[data-col-header]');
    fireEvent.dragStart(headers[2]);
    fireEvent.dragOver(headers[0]);
    fireEvent.drop(headers[0]);
    expect(onReorderColumns).toHaveBeenCalledWith(2, 0);
  });

  it('headers are not draggable without the reorder handler', () => {
    const { container } = render(
      <TableHeader columns={COLS} sorts={[]} onSort={() => {}} sortable />,
    );
    const headers = container.querySelectorAll('[data-col-header]');
    // `draggable` is an enumerated attribute: false renders as "false".
    expect(headers[0].getAttribute('draggable')).toBe('false');
  });
});

describe('TableHeader frozen columns (DB-12)', () => {
  it('pins the frozen headers at cumulative offsets', () => {
    const { container } = render(
      <TableHeader
        columns={COLS}
        sorts={[]}
        onSort={() => {}}
        sortable
        frozenColumns={2}
        columnWidths={[100, 120, 140]}
      />,
    );
    const headers = [...container.querySelectorAll<HTMLElement>('[data-col-header]')];
    const idHeader = headers.find((h) => h.getAttribute('data-col-header') === 'id')!;
    const nameHeader = headers.find((h) => h.getAttribute('data-col-header') === 'name')!;
    const priceHeader = headers.find((h) => h.getAttribute('data-col-header') === 'price')!;
    // The gutter is 40px wide: id pins at 40, name right after it.
    expect(idHeader.style.position).toBe('sticky');
    expect(idHeader.style.left).toBe('40px');
    expect(nameHeader.style.position).toBe('sticky');
    expect(nameHeader.style.left).toBe('140px');
    // The unfrozen column renders exactly as before.
    expect(priceHeader.style.position).toBe('');
    // The frozen edge carries the visible seam.
    expect(nameHeader.className).toContain('border-r-2');
  });

  it('freezes nothing by default', () => {
    const { container } = render(
      <TableHeader columns={COLS} sorts={[]} onSort={() => {}} sortable />,
    );
    const headers = [...container.querySelectorAll<HTMLElement>('[data-col-header]')];
    expect(headers[0].style.position).toBe('');
  });
});

describe('TableHeader row-number gutter (DB-17)', () => {
  it('renders the # gutter by default', () => {
    const { container } = render(
      <TableHeader columns={COLS} sorts={[]} onSort={() => {}} sortable />,
    );
    // The gutter is the only child that is not a column header.
    expect(container.textContent).toContain('#');
  });

  it('drops the gutter when showRowNumbers is false', () => {
    const { container } = render(
      <TableHeader columns={COLS} sorts={[]} onSort={() => {}} sortable showRowNumbers={false} />,
    );
    expect(container.textContent).not.toContain('#');
    // ...and the columns are all still there.
    expect(container.querySelectorAll('[data-col-header]')).toHaveLength(COLS.length);
  });

  it('pins the frozen columns from the grid edge when the gutter is off', () => {
    const { container } = render(
      <TableHeader
        columns={COLS}
        sorts={[]}
        onSort={() => {}}
        sortable
        frozenColumns={1}
        columnWidths={[100, 120, 140]}
        showRowNumbers={false}
      />,
    );
    const headers = [...container.querySelectorAll<HTMLElement>('[data-col-header]')];
    const idHeader = headers.find((h) => h.getAttribute('data-col-header') === 'id')!;
    // Behind the 40px gutter the first column pins at 40; without it, at 0.
    expect(idHeader.style.position).toBe('sticky');
    expect(idHeader.style.left).toBe('0px');
  });
});
