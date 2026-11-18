import { describe, expect, it, vi } from 'vitest';
import { render, fireEvent } from '@testing-library/react';
import { TableHeader } from '../TableHeader';
import { showNativeContextMenu } from '../../../lib/nativeContextMenu';
import type { NativeMenuItemDef } from '../../../lib/nativeContextMenu';
import type { SortCondition } from '../../../types';

vi.mock('../../../lib/nativeContextMenu', () => ({
  showNativeContextMenu: vi.fn(),
}));

/** The `kind: 'item'` variant of a menu entry — the one that carries an action. */
type MenuItem = Extract<NativeMenuItemDef, { kind: 'item' }>;

/** The items of the last opened menu, asserted entry by entry. */
function lastMenuItems(): MenuItem[] {
  const calls = vi.mocked(showNativeContextMenu).mock.calls;
  expect(calls.length).toBeGreaterThan(0);
  return calls[calls.length - 1][0] as MenuItem[];
}

const COLS = [
  { id: 'id', name: 'id', type: 'bigint' },
  { id: 'name', name: 'name', type: 'varchar' },
  { id: 'price', name: 'price', type: 'numeric' },
];

describe('TableHeader sort interaction (DBR-SORT, Navicat-aligned)', () => {
  it('shows a default sort icon for every unsorted column', () => {
    const { container } = render(
      <TableHeader columns={COLS} sorts={[]} onSort={() => {}} sortable />,
    );
    // Every column always shows one of the three sort-state icons.
    expect(container.querySelectorAll('[data-sort-indicator]').length).toBe(COLS.length);
    expect(container.querySelectorAll('[data-sort-menu]').length).toBe(COLS.length);
    expect(container.querySelectorAll('[data-sort-menu][data-sort-state="none"]').length).toBe(
      COLS.length,
    );
    expect(container.querySelectorAll('[data-sort-menu] .lucide-arrow-up-down').length).toBe(
      COLS.length,
    );
  });

  it('shows the ascending sort icon on the sorted column', () => {
    const sorts: SortCondition[] = [{ column: 'name', descending: false }];
    const { container } = render(
      <TableHeader columns={COLS} sorts={sorts} onSort={() => {}} sortable />,
    );
    expect(container.querySelectorAll('[data-sort-indicator]').length).toBe(COLS.length);
    const nameHeader = container.querySelector('[data-col-header="name"]')!;
    const sortControl = nameHeader.querySelector('[data-sort-menu]')!;
    expect(sortControl.getAttribute('data-sort-state')).toBe('asc');
    expect(sortControl.querySelector('.lucide-arrow-up-narrow-wide')).toBeTruthy();
    expect(sortControl.querySelector('.lucide-arrow-up-down')).toBeNull();
  });

  it('does not render per-column filter buttons', () => {
    const { container } = render(
      <TableHeader columns={COLS} sorts={[]} onSort={() => {}} sortable />,
    );
    expect(container.querySelectorAll('[data-col-header] button[aria-label="Filter"]').length).toBe(
      0,
    );
  });

  it('a single click sorts an unsorted column ascending', () => {
    const onSort = vi.fn();
    const { container } = render(
      <TableHeader columns={COLS} sorts={[]} onSort={onSort} sortable />,
    );
    const headers = container.querySelectorAll('[data-col-header]');
    fireEvent.click(headers[1]); // "name" column
    expect(onSort).toHaveBeenCalledWith({ column: 'name', descending: false });
  });

  it('a second click turns the ascending sort descending', () => {
    const onSort = vi.fn();
    const sorts: SortCondition[] = [{ column: 'name', descending: false }];
    const { container } = render(
      <TableHeader columns={COLS} sorts={sorts} onSort={onSort} sortable />,
    );
    const headers = container.querySelectorAll('[data-col-header]');
    fireEvent.click(headers[1]);
    expect(onSort).toHaveBeenCalledWith({ column: 'name', descending: true });
  });

  it('the third click removes the column from the sort', () => {
    const onSort = vi.fn();
    const onRemoveSort = vi.fn();
    const sorts: SortCondition[] = [{ column: 'name', descending: true }];
    const { container } = render(
      <TableHeader
        columns={COLS}
        sorts={sorts}
        onSort={onSort}
        onRemoveSort={onRemoveSort}
        sortable
      />,
    );
    const headers = container.querySelectorAll('[data-col-header]');
    fireEvent.click(headers[1]);
    expect(onRemoveSort).toHaveBeenCalledWith('name');
    // The removal is the remove channel's business: the click never *sets*
    // another sort on its way out.
    expect(onSort).not.toHaveBeenCalled();
  });

  it('without a remove channel the third click wraps back to ascending', () => {
    const onSort = vi.fn();
    const sorts: SortCondition[] = [{ column: 'name', descending: true }];
    const { container } = render(
      <TableHeader columns={COLS} sorts={sorts} onSort={onSort} sortable />,
    );
    const headers = container.querySelectorAll('[data-col-header]');
    fireEvent.click(headers[1]);
    expect(onSort).toHaveBeenCalledWith({ column: 'name', descending: false });
  });

  it('single-clicking an unsorted column replaces the multi-sort with it', () => {
    const onSort = vi.fn();
    const sorts: SortCondition[] = [
      { column: 'name', descending: false },
      { column: 'price', descending: true },
    ];
    const { container } = render(
      <TableHeader columns={COLS} sorts={sorts} onSort={onSort} sortable />,
    );
    const headers = container.querySelectorAll('[data-col-header]');
    fireEvent.click(headers[0]); // "id" — unsorted until now
    expect(onSort).toHaveBeenCalledWith({ column: 'id', descending: false });
  });

  it('the adaptive sort control opens the three single-column entries without sorting', () => {
    const onSort = vi.fn();
    vi.mocked(showNativeContextMenu).mockClear();
    const { container } = render(
      <TableHeader columns={COLS} sorts={[]} onSort={onSort} sortable />,
    );
    const menus = container.querySelectorAll('[data-sort-menu]');
    fireEvent.click(menus[1]); // "name" column's sort control

    // Opening the menu sorts nothing on its own.
    expect(onSort).not.toHaveBeenCalled();
    const items = lastMenuItems();
    expect(items.map((item) => item.id)).toEqual([
      'sort-ascending',
      'sort-descending',
      'remove-sort',
    ]);
    // An unsorted column has nothing to remove, so the entry is greyed.
    expect(items[2].enabled).toBe(false);
  });

  it('the menu actions set the single sort and remove the column', () => {
    const onSort = vi.fn();
    const onRemoveSort = vi.fn();
    const sorts: SortCondition[] = [{ column: 'name', descending: false }];
    vi.mocked(showNativeContextMenu).mockClear();
    const { container } = render(
      <TableHeader
        columns={COLS}
        sorts={sorts}
        onSort={onSort}
        onRemoveSort={onRemoveSort}
        sortable
      />,
    );
    const headers = container.querySelectorAll('[data-col-header]');
    const nameHeader = [...headers].find(
      (header) => header.getAttribute('data-col-header') === 'name',
    )!;
    fireEvent.click(nameHeader.querySelector('[data-sort-menu]')!);

    const items = lastMenuItems();
    // The sorted column can be removed from the menu.
    expect(items[2].enabled).toBe(true);
    items[0].action();
    expect(onSort).toHaveBeenCalledWith({ column: 'name', descending: false });
    items[1].action();
    expect(onSort).toHaveBeenCalledWith({ column: 'name', descending: true });
    items[2].action();
    expect(onRemoveSort).toHaveBeenCalledWith('name');
  });

  it('a click on the resize strip does not sort the column', () => {
    const onSort = vi.fn();
    const { container } = render(
      <TableHeader
        columns={COLS}
        sorts={[]}
        onSort={onSort}
        sortable
        columnWidths={[100, 120, 140]}
        onResizeStart={() => {}}
      />,
    );
    const headers = container.querySelectorAll('[data-col-header]');
    // The resize strip is the last child of the header cell.
    const strip = headers[1].lastElementChild!;
    fireEvent.click(strip);
    expect(onSort).not.toHaveBeenCalled();
  });

  it('carries aria-sort for the ARIA grid semantics', () => {
    const sorts: SortCondition[] = [{ column: 'name', descending: false }];
    const { container } = render(
      <TableHeader columns={COLS} sorts={sorts} onSort={() => {}} sortable gridSemantics />,
    );
    const headers = [...container.querySelectorAll('[data-col-header]')];
    expect(
      headers.find((h) => h.getAttribute('data-col-header') === 'id')!.getAttribute('aria-sort'),
    ).toBeNull();
    expect(
      headers.find((h) => h.getAttribute('data-col-header') === 'name')!.getAttribute('aria-sort'),
    ).toBe('ascending');
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
    // Every column shows an icon; sorted ones carry direction-specific state.
    expect(container.querySelectorAll('[data-sort-indicator]').length).toBe(COLS.length);
    expect(
      container.querySelectorAll(
        '[data-sort-menu][data-sort-state="asc"], [data-sort-menu][data-sort-state="desc"]',
      ).length,
    ).toBe(2);
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
    const sortControl = priceHeader.querySelector('[data-sort-menu]')!;
    expect(sortControl.getAttribute('data-sort-state')).toBe('desc');
    expect(sortControl.querySelector('.lucide-arrow-down-wide-narrow')).toBeTruthy();
    expect(sortControl.querySelector('.lucide-arrow-up-narrow-wide')).toBeNull();
  });

  it('keeps sort icons visible on every column in a multi-column sort', () => {
    const sorts: SortCondition[] = [
      { column: 'price', descending: true },
      { column: 'name', descending: false },
    ];
    const { container } = render(
      <TableHeader columns={COLS} sorts={sorts} onSort={() => {}} sortable />,
    );
    expect(container.querySelectorAll('[data-sort-indicator]').length).toBe(COLS.length);
    expect(container.querySelectorAll('[data-sort-menu][data-sort-state="none"]').length).toBe(1);
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
