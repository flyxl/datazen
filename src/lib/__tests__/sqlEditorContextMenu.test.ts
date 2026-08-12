import { describe, expect, it, vi } from 'vitest';
import { buildSqlEditorContextMenuItems } from '../sqlEditorContextMenu';

describe('buildSqlEditorContextMenuItems', () => {
  const labels = { addFavorite: 'Add to Favorites' };

  it('includes edit predefined items and favorite', () => {
    const onAdd = vi.fn();
    const items = buildSqlEditorContextMenuItems(labels, 'SELECT 1', onAdd);
    expect(items.map((i) => ('item' in i && i.kind === 'predefined' ? i.item : i.kind))).toEqual([
      'Cut',
      'Copy',
      'Paste',
      'SelectAll',
      'separator',
      'item',
    ]);
    const fav = items[items.length - 1]!;
    expect(fav).toMatchObject({ kind: 'item', id: 'add-favorite', enabled: true });
    if (fav.kind === 'item') fav.action();
    expect(onAdd).toHaveBeenCalledWith('SELECT 1');
  });

  it('disables favorite when sql is empty/whitespace', () => {
    const onAdd = vi.fn();
    const items = buildSqlEditorContextMenuItems(labels, '   ', onAdd);
    const fav = items[items.length - 1]!;
    expect(fav).toMatchObject({ kind: 'item', enabled: false });
    if (fav.kind === 'item') fav.action();
    expect(onAdd).not.toHaveBeenCalled();
  });

  it('trims sql before passing to onAddFavorite', () => {
    const onAdd = vi.fn();
    const items = buildSqlEditorContextMenuItems(labels, '  SELECT 2  ', onAdd);
    const fav = items[items.length - 1]!;
    if (fav.kind === 'item') fav.action();
    expect(onAdd).toHaveBeenCalledWith('SELECT 2');
  });
});
