import { afterEach, describe, expect, it } from 'vitest';
import { showWebContextMenu, useContextMenuStore } from '../contextMenuStore';

describe('contextMenuStore', () => {
  afterEach(() => {
    useContextMenuStore.getState().hide();
  });

  it('ignores menus that normalize to empty', () => {
    showWebContextMenu([{ kind: 'separator' }], { x: 10, y: 20 });
    expect(useContextMenuStore.getState().open).toBe(false);
  });

  it('opens with normalized items and position', () => {
    const action = () => undefined;
    showWebContextMenu(
      [
        { kind: 'separator' },
        { kind: 'item', id: 'a', label: 'A', action },
        { kind: 'separator' },
      ],
      { x: 12, y: 34 },
    );
    const state = useContextMenuStore.getState();
    expect(state.open).toBe(true);
    expect(state.x).toBe(12);
    expect(state.y).toBe(34);
    expect(state.items).toHaveLength(1);
    expect(state.items[0]).toMatchObject({ kind: 'item', id: 'a' });
  });

  it('hides the menu', () => {
    showWebContextMenu([{ kind: 'item', id: 'a', label: 'A', action: () => undefined }], {
      x: 1,
      y: 1,
    });
    useContextMenuStore.getState().hide();
    expect(useContextMenuStore.getState().open).toBe(false);
    expect(useContextMenuStore.getState().items).toEqual([]);
  });
});
