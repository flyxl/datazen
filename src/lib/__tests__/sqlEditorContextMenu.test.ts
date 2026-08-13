import { describe, expect, it, vi } from 'vitest';
import { buildSqlEditorContextMenuItems, toggleSqlLineComments } from '../sqlEditorContextMenu';

describe('toggleSqlLineComments', () => {
  it('prefixes non-empty lines with -- ', () => {
    expect(toggleSqlLineComments('SELECT 1\n\nSELECT 2')).toBe('-- SELECT 1\n\n-- SELECT 2');
  });

  it('strips -- when every non-empty line is commented', () => {
    expect(toggleSqlLineComments('-- SELECT 1\n\n-- SELECT 2')).toBe('SELECT 1\n\nSELECT 2');
  });
});

describe('buildSqlEditorContextMenuItems', () => {
  const labels = {
    run: 'Run',
    runSelection: 'Run Selection',
    format: 'Format',
    comment: 'Comment/Uncomment',
    addFavorite: 'Add to Favorites',
  };

  function kinds(items: ReturnType<typeof buildSqlEditorContextMenuItems>) {
    return items.map((i) => (i.kind === 'predefined' ? i.item : i.kind === 'item' ? i.id : i.kind));
  }

  it('includes edit actions then run / format / comment / favorite', () => {
    const onAdd = vi.fn();
    const onRun = vi.fn();
    const onRunSelection = vi.fn();
    const onFormat = vi.fn();
    const onComment = vi.fn();
    const items = buildSqlEditorContextMenuItems({
      labels,
      handlers: { onRun, onRunSelection, onFormat, onComment, onAddFavorite: onAdd },
      sqlText: 'SELECT 1',
      hasSelection: true,
    });
    expect(kinds(items)).toEqual([
      'Cut',
      'Copy',
      'Paste',
      'SelectAll',
      'separator',
      'run',
      'run-selection',
      'format',
      'comment',
      'separator',
      'add-favorite',
    ]);
    const fav = items[items.length - 1]!;
    expect(fav).toMatchObject({ kind: 'item', id: 'add-favorite', enabled: true });
    if (fav.kind === 'item') fav.action();
    expect(onAdd).toHaveBeenCalledWith('SELECT 1');

    const runSel = items.find((i) => i.kind === 'item' && i.id === 'run-selection');
    expect(runSel).toMatchObject({ kind: 'item', enabled: true });
  });

  it('disables run-selection without selection and favorite when empty', () => {
    const onAdd = vi.fn();
    const items = buildSqlEditorContextMenuItems({
      labels,
      handlers: {
        onRun: vi.fn(),
        onRunSelection: vi.fn(),
        onFormat: vi.fn(),
        onComment: vi.fn(),
        onAddFavorite: onAdd,
      },
      sqlText: '   ',
      hasSelection: false,
    });
    const runSel = items.find((i) => i.kind === 'item' && i.id === 'run-selection');
    expect(runSel).toMatchObject({ kind: 'item', enabled: false });
    const fav = items[items.length - 1]!;
    expect(fav).toMatchObject({ kind: 'item', enabled: false });
    if (fav.kind === 'item') fav.action();
    expect(onAdd).not.toHaveBeenCalled();
  });

  it('trims sql before passing to onAddFavorite', () => {
    const onAdd = vi.fn();
    const items = buildSqlEditorContextMenuItems({
      labels,
      handlers: { onAddFavorite: onAdd },
      sqlText: '  SELECT 2  ',
    });
    const fav = items[items.length - 1]!;
    if (fav.kind === 'item') fav.action();
    expect(onAdd).toHaveBeenCalledWith('SELECT 2');
  });
});
