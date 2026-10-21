/**
 * Round-2 independent re-test (retest agent) — the `enableSetNull` guard of the context menu.
 *
 * The menu-equivalence suite shipped with the extraction has a case named "drops set-null on a
 * read-only grid", but it renders that grid **without** `onCellEdit`, so `set-null` disappears
 * because the cell cannot be edited at all — the `enableSetNull` flag itself is never exercised.
 * A mutation that ignores `enableSetNull` therefore leaves every menu test green.
 *
 * That flag is load-bearing: `ResultTableView` renders an **editable** grid with
 * `enableSetNull={false}` (`src/windows/connection/result-workspace/ResultTableView.tsx:150`),
 * i.e. "you may change values here, but the menu must not offer Set NULL". This file pins that
 * combination: editable grid + `enableSetNull={false}` ⇒ no `set-null` item.
 */
import { describe, expect, it, vi, afterEach, beforeEach } from 'vitest';
import { render, fireEvent, cleanup, waitFor } from '@testing-library/react';
import { DataTable } from '../DataTable';

const { showNativeContextMenu } = vi.hoisted(() => ({
  showNativeContextMenu: vi.fn().mockResolvedValue(undefined),
}));

vi.mock('../../../hooks/useI18n', () => ({
  useI18n: () => ({ t: (key: string) => key }),
}));

vi.mock('../../../hooks/useColumnResize', () => ({
  useColumnResize: () => ({
    columnWidths: [160, 200],
    onResizeStart: vi.fn(),
  }),
  adjustWidthsForSort: (widths: number[]) => widths,
}));

vi.mock('../../../hooks/useVirtualTable', () => ({
  useVirtualTable: ({ rows }: { rows: unknown[][] }) => ({
    virtualRows: rows.map((_, index) => ({ index, key: String(index), start: index * 40 })),
    totalHeight: rows.length * 40,
  }),
}));

vi.mock('../../../lib/nativeContextMenu', () => ({
  showNativeContextMenu: (...args: unknown[]) => showNativeContextMenu(...args),
}));

type MenuItem = { kind: string; id?: string; items?: MenuItem[] };

const COLS = [
  { id: 'id', name: 'id', type: 'integer' },
  { id: 'name', name: 'name', type: 'varchar' },
];

const ROWS = [
  [1, 'Alice'],
  [2, 'Bob'],
];

async function openMenu(enableSetNull: boolean): Promise<string[]> {
  const { container } = render(
    <DataTable
      columns={COLS}
      rows={ROWS}
      selectedRows={new Set([0])}
      onRowSelect={vi.fn()}
      onSelectAll={vi.fn()}
      // The grid IS editable — only the menu entry is switched off.
      onCellEdit={vi.fn()}
      enableSetNull={enableSetNull}
      onDeleteRows={vi.fn()}
    />,
  );
  const cell = container.querySelector('[data-dt-row="0"][data-dt-col="name"]') as HTMLElement;
  fireEvent.contextMenu(cell, { clientX: 10, clientY: 10 });

  await waitFor(() => expect(showNativeContextMenu).toHaveBeenCalled());
  const items = showNativeContextMenu.mock.calls[0]![0] as MenuItem[];
  const more = items.find((item) => item.kind === 'submenu');
  return (more?.items ?? []).filter((item) => item.kind === 'item').map((item) => item.id ?? '');
}

beforeEach(() => {
  vi.clearAllMocks();
});

afterEach(cleanup);

describe('the enableSetNull flag alone decides whether Set NULL is offered', () => {
  it('offers set-null on an editable grid', async () => {
    expect(await openMenu(true)).toContain('set-null');
  });

  it('hides set-null on an editable grid that opts out with enableSetNull={false}', async () => {
    const ids = await openMenu(false);
    expect(ids).not.toContain('set-null');
    // The rest of the menu is untouched — the flag only removes this one item.
    expect(ids).toContain('copy-as-json');
  });
});
