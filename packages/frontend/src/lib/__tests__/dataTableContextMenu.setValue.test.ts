import { describe, expect, it, vi } from 'vitest';
import {
  buildDataTableContextMenuItems,
  type DataTableContextMenuLabels,
} from '../dataTableContextMenu';

/**
 * The "Set Value" submenu (DB-05).
 *
 * Two shapes have to hold at the same time:
 *
 * - a grid that offers the write entries gets a **submenu** of them, and
 *   `Set NULL` does not appear a second time inside `more-actions`;
 * - a grid that does not offer them keeps the menu it had before DB-05 —
 *   `Set NULL` inside `more-actions`, and no submenu.
 */
const labels: DataTableContextMenuLabels = {
  copy: 'Copy',
  copyRow: 'Copy Row',
  moreActions: 'More actions',
  copyAsJson: 'Copy as JSON',
  copyAsSqlInsert: 'Copy as SQL INSERT',
  copyAsUpdate: 'Copy as UPDATE',
  copyAsCsv: 'Copy as CSV',
  copyColumnName: 'Copy Column Name',
  copyColumnData: 'Copy Column Data',
  setNull: 'Set NULL',
  filterByValue: 'Filter by This Value',
  copySelectedRows: 'Copy Selected Rows',
  deleteRow: 'Delete Row',
  export: 'Export',
  restoreRow: 'Restore This Row',
  setValue: 'Set Value',
  setEmpty: 'Set empty string',
  setDefault: 'Use column default',
  revertCell: 'Revert this cell',
  loadCandidates: 'Load candidate values',
};

function submenuItems(
  items: ReturnType<typeof buildDataTableContextMenuItems>,
  id: string,
): string[] {
  const found = items.find((item) => item.kind === 'submenu' && item.id === id);
  if (!found || found.kind !== 'submenu') return [];
  return found.items
    .filter((item) => item.kind === 'item')
    .map((item) => (item.kind === 'item' ? item.id : ''));
}

describe('the Set Value submenu', () => {
  it('routes the new Set Value submenu when onSetValue is provided', () => {
    const items = buildDataTableContextMenuItems({
      labels,
      handlers: { onSetValue: vi.fn() },
      hasCellContext: true,
      canSetNull: true,
      setValue: { canNull: true, canEmpty: true, rowKind: 'update', hasPending: true },
    });

    expect(submenuItems(items, 'set-value')).toEqual([
      'set-value-setNull',
      'set-value-setEmpty',
      'set-value-revertCell',
    ]);
    // `Set NULL` is not offered twice.
    expect(submenuItems(items, 'more-actions')).not.toContain('set-null');
  });

  it('keeps the legacy more-actions menu when it is not', () => {
    const items = buildDataTableContextMenuItems({
      labels,
      handlers: { onSetNull: vi.fn() },
      hasCellContext: true,
      canSetNull: true,
    });

    expect(submenuItems(items, 'set-value')).toEqual([]);
    expect(submenuItems(items, 'more-actions')).toContain('set-null');
  });

  it('keeps the legacy menu even when the cell state is known but no handler is', () => {
    const items = buildDataTableContextMenuItems({
      labels,
      handlers: { onSetNull: vi.fn() },
      hasCellContext: true,
      canSetNull: true,
      setValue: { canNull: true, canEmpty: true, rowKind: 'update', hasPending: false },
    });

    expect(submenuItems(items, 'set-value')).toEqual([]);
    expect(submenuItems(items, 'more-actions')).toContain('set-null');
  });

  it('offers "use column default" for a new row and "revert" for an existing one', () => {
    const insert = buildDataTableContextMenuItems({
      labels,
      handlers: { onSetValue: vi.fn() },
      hasCellContext: true,
      setValue: { canNull: false, canEmpty: true, rowKind: 'insert', hasPending: false },
    });
    expect(submenuItems(insert, 'set-value')).toEqual([
      'set-value-setEmpty',
      'set-value-setDefault',
    ]);

    const update = buildDataTableContextMenuItems({
      labels,
      handlers: { onSetValue: vi.fn() },
      hasCellContext: true,
      setValue: { canNull: true, canEmpty: true, rowKind: 'update', hasPending: false },
    });
    expect(submenuItems(update, 'set-value')).toEqual(['set-value-setNull', 'set-value-setEmpty']);
  });

  it('offers the candidate values of an enumeration column', () => {
    const items = buildDataTableContextMenuItems({
      labels,
      handlers: { onSetValue: vi.fn(), onLoadCandidates: vi.fn() },
      hasCellContext: true,
      setValue: {
        canNull: true,
        canEmpty: true,
        rowKind: 'update',
        hasPending: false,
        canLoadCandidates: true,
      },
    });
    expect(submenuItems(items, 'set-value')).toContain('set-value-load-candidates');
  });

  it('writes the cell when an entry is picked', () => {
    const onSetValue = vi.fn();
    buildDataTableContextMenuItems({
      labels,
      handlers: { onSetValue },
      hasCellContext: true,
      setValue: { canNull: true, canEmpty: true, rowKind: 'update', hasPending: false },
    });
    const items = buildDataTableContextMenuItems({
      labels,
      handlers: { onSetValue },
      hasCellContext: true,
      setValue: { canNull: true, canEmpty: true, rowKind: 'update', hasPending: false },
    });
    const submenu = items.find((item) => item.kind === 'submenu' && item.id === 'set-value');
    if (!submenu || submenu.kind !== 'submenu') throw new Error('no submenu');
    const entry = submenu.items.find(
      (item) => item.kind === 'item' && item.id === 'set-value-setEmpty',
    );
    if (!entry || entry.kind !== 'item') throw new Error('no entry');
    entry.action?.();
    expect(onSetValue).toHaveBeenCalledWith('setEmpty');
  });
});
