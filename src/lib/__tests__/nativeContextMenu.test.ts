import { beforeEach, describe, expect, it, vi } from 'vitest';
import { useContextMenuStore } from '../../stores/contextMenuStore';
import {
  createNativeContextMenuHandler,
  nativeEditMenuItems,
  normalizeNativeMenuItems,
  showNativeContextMenu,
  type NativeMenuItemDef,
} from '../nativeContextMenu';

describe('normalizeNativeMenuItems', () => {
  it('keeps disabled items and removes empty submenus', () => {
    const items: NativeMenuItemDef[] = [
      { kind: 'item', id: 'a', label: 'A', action: () => undefined },
      { kind: 'item', id: 'b', label: 'B', enabled: false, action: () => undefined },
      {
        kind: 'submenu',
        label: 'Empty',
        items: [],
      },
      {
        kind: 'submenu',
        label: 'WithDisabled',
        items: [{ kind: 'item', id: 'c', label: 'C', enabled: false, action: () => undefined }],
      },
      { kind: 'separator' },
      { kind: 'item', id: 'd', label: 'D', action: () => undefined },
    ];
    const normalized = normalizeNativeMenuItems(items);
    expect(normalized).toEqual([
      { kind: 'item', id: 'a', label: 'A', action: expect.any(Function) },
      { kind: 'item', id: 'b', label: 'B', enabled: false, action: expect.any(Function) },
      {
        kind: 'submenu',
        label: 'WithDisabled',
        items: [
          { kind: 'item', id: 'c', label: 'C', enabled: false, action: expect.any(Function) },
        ],
      },
      { kind: 'separator' },
      { kind: 'item', id: 'd', label: 'D', action: expect.any(Function) },
    ]);
  });

  it('collapses duplicate and edge separators', () => {
    const items: NativeMenuItemDef[] = [
      { kind: 'separator' },
      { kind: 'predefined', item: 'Separator' },
      { kind: 'item', id: 'a', label: 'A', action: () => undefined },
      { kind: 'separator' },
      { kind: 'separator' },
      { kind: 'item', id: 'b', label: 'B', action: () => undefined },
      { kind: 'separator' },
    ];
    const normalized = normalizeNativeMenuItems(items);
    expect(normalized.map((i) => i.kind)).toEqual(['item', 'separator', 'item']);
  });
});

describe('nativeEditMenuItems', () => {
  it('returns cut/copy/paste/selectAll predefined items', () => {
    expect(nativeEditMenuItems()).toEqual([
      { kind: 'predefined', item: 'Cut' },
      { kind: 'predefined', item: 'Copy' },
      { kind: 'predefined', item: 'Paste' },
      { kind: 'predefined', item: 'SelectAll' },
    ]);
  });
});

describe('showNativeContextMenu', () => {
  beforeEach(() => {
    useContextMenuStore.getState().hide();
  });

  it('no-ops when only separators remain after normalize', async () => {
    useContextMenuStore.getState().show(
      [{ kind: 'item', id: 'keep', label: 'Keep', action: () => undefined }],
      { x: 1, y: 1 },
    );
    expect(useContextMenuStore.getState().open).toBe(true);
    showNativeContextMenu([{ kind: 'separator' }, { kind: 'predefined', item: 'Separator' }], {
      x: 8,
      y: 12,
    });
    await vi.waitFor(() => {
      expect(useContextMenuStore.getState().open).toBe(false);
    });
  });

  it('opens the web menu store at the given client position', async () => {
    const action = vi.fn();
    showNativeContextMenu(
      [
        { kind: 'item', id: 'run', label: 'Run', action },
        { kind: 'separator' },
        { kind: 'predefined', item: 'Copy' },
        {
          kind: 'submenu',
          label: 'More',
          items: [{ kind: 'item', id: 'nested', label: 'Nested', action: () => undefined }],
        },
      ],
      { x: 40, y: 80 },
    );
    await vi.waitFor(() => {
      const s = useContextMenuStore.getState();
      expect(s.open).toBe(true);
      expect(s.x).toBe(40);
      expect(s.y).toBe(80);
      expect(s.items.some((i) => i.kind === 'item' && i.id === 'run')).toBe(true);
      expect(s.items.some((i) => i.kind === 'submenu')).toBe(true);
    });
  });

  it('keeps disabled items so the web menu can render them inert', async () => {
    showNativeContextMenu(
      [{ kind: 'item', id: 'x', label: 'X', enabled: false, action: () => undefined }],
      { x: 1, y: 2 },
    );
    await vi.waitFor(() => {
      const item = useContextMenuStore.getState().items[0];
      expect(item).toMatchObject({ kind: 'item', id: 'x', enabled: false });
    });
  });
});

describe('createNativeContextMenuHandler', () => {
  beforeEach(() => {
    useContextMenuStore.getState().hide();
  });

  it('prevents default, stops propagation, and shows menu at client coords', async () => {
    const handler = createNativeContextMenuHandler(() => [
      { kind: 'item', id: 'a', label: 'A', action: () => undefined },
    ]);
    const e = {
      preventDefault: vi.fn(),
      stopPropagation: vi.fn(),
      clientX: 15,
      clientY: 25,
    } as unknown as MouseEvent;
    handler(e);
    expect(e.preventDefault).toHaveBeenCalled();
    expect(e.stopPropagation).toHaveBeenCalled();
    await vi.waitFor(() => {
      const s = useContextMenuStore.getState();
      expect(s.open).toBe(true);
      expect(s.x).toBe(15);
      expect(s.y).toBe(25);
    });
  });

  it('can skip stopPropagation', async () => {
    const handler = createNativeContextMenuHandler(
      () => [{ kind: 'item', id: 'a', label: 'A', action: () => undefined }],
      { stopPropagation: false },
    );
    const e = {
      preventDefault: vi.fn(),
      stopPropagation: vi.fn(),
      clientX: 3,
      clientY: 4,
    } as unknown as MouseEvent;
    handler(e);
    expect(e.stopPropagation).not.toHaveBeenCalled();
    await vi.waitFor(() => expect(useContextMenuStore.getState().open).toBe(true));
  });
});
