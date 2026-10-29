import { describe, expect, it, vi } from 'vitest';
import {
  buildDataTableContextMenuItems,
  type DataTableContextMenuLabels,
} from '../dataTableContextMenu';

/**
 * The reverse foreign-key entry of the row menu (DB-20).
 *
 * A grid whose owner offers the reverse jump gets a "Rows referencing this
 * row" entry next to the row's own actions; a grid whose owner does not keeps
 * the menu it had before DB-20 — the entry is optional on purpose, the same
 * way every other chapter's entries are.
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
  referencedBy: 'Rows referencing this row',
};

function itemIds(items: ReturnType<typeof buildDataTableContextMenuItems>): string[] {
  return items
    .filter((item) => item.kind === 'item')
    .map((item) => (item.kind === 'item' ? item.id : ''));
}

describe('the referencing-rows entry', () => {
  it('is offered on the row under the pointer when the handler exists', () => {
    const items = buildDataTableContextMenuItems({
      labels,
      handlers: { onShowReferencingRows: vi.fn() },
      hasCellContext: true,
    });

    expect(itemIds(items)).toContain('show-referencing-rows');
  });

  it('keeps the menu it had before DB-20 when the handler does not exist', () => {
    const items = buildDataTableContextMenuItems({
      labels,
      handlers: {},
      hasCellContext: true,
    });

    expect(itemIds(items)).not.toContain('show-referencing-rows');
  });

  it('is offered on a selection-scoped menu too', () => {
    const items = buildDataTableContextMenuItems({
      labels,
      handlers: { onShowReferencingRows: vi.fn() },
      hasCellContext: false,
    });

    expect(itemIds(items)).toContain('show-referencing-rows');
  });
});
