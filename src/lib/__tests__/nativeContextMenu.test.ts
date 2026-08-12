import { beforeEach, describe, expect, it, vi } from 'vitest';

const menuPopup = vi.fn().mockResolvedValue(undefined);
const menuNew = vi.fn().mockImplementation(async ({ items }: { items: unknown[] }) => ({
  popup: menuPopup,
  items,
}));
const menuItemNew = vi.fn().mockImplementation(async (opts: unknown) => opts);
const submenuNew = vi.fn().mockImplementation(async (opts: unknown) => opts);
const predefinedNew = vi.fn().mockImplementation(async (opts: unknown) => opts);

vi.mock('@tauri-apps/api/menu', () => ({
  Menu: { new: (...a: unknown[]) => menuNew(...a) },
  MenuItem: { new: (...a: unknown[]) => menuItemNew(...a) },
  Submenu: { new: (...a: unknown[]) => submenuNew(...a) },
  PredefinedMenuItem: { new: (...a: unknown[]) => predefinedNew(...a) },
}));

import {
  createNativeContextMenuHandler,
  nativeEditMenuItems,
  normalizeNativeMenuItems,
  showNativeContextMenu,
  type NativeMenuItemDef,
} from '../nativeContextMenu';

describe('normalizeNativeMenuItems', () => {
  it('removes disabled items and empty submenus', () => {
    const items: NativeMenuItemDef[] = [
      { kind: 'item', id: 'a', label: 'A', action: () => undefined },
      { kind: 'item', id: 'b', label: 'B', enabled: false, action: () => undefined },
      {
        kind: 'submenu',
        label: 'Empty',
        items: [{ kind: 'item', id: 'c', label: 'C', enabled: false, action: () => undefined }],
      },
      { kind: 'separator' },
      { kind: 'item', id: 'd', label: 'D', action: () => undefined },
    ];
    const normalized = normalizeNativeMenuItems(items);
    expect(normalized).toEqual([
      { kind: 'item', id: 'a', label: 'A', action: expect.any(Function) },
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
    menuPopup.mockClear();
    menuNew.mockClear();
    menuItemNew.mockClear();
    submenuNew.mockClear();
    predefinedNew.mockClear();
  });

  it('no-ops when all items are filtered out', async () => {
    await showNativeContextMenu([
      { kind: 'item', id: 'x', label: 'X', enabled: false, action: () => undefined },
    ]);
    expect(menuNew).not.toHaveBeenCalled();
  });

  it('builds menu and pops up', async () => {
    const action = vi.fn();
    await showNativeContextMenu([
      { kind: 'item', id: 'run', label: 'Run', action },
      { kind: 'separator' },
      { kind: 'predefined', item: 'Copy' },
      {
        kind: 'submenu',
        label: 'More',
        items: [{ kind: 'item', id: 'nested', label: 'Nested', action: () => undefined }],
      },
    ]);
    expect(menuItemNew).toHaveBeenCalled();
    expect(predefinedNew).toHaveBeenCalledWith(expect.objectContaining({ item: 'Copy' }));
    expect(submenuNew).toHaveBeenCalled();
    expect(menuNew).toHaveBeenCalled();
    expect(menuPopup).toHaveBeenCalledTimes(1);
  });

  it('invokes item action when MenuItem action runs', async () => {
    const action = vi.fn();
    await showNativeContextMenu([{ kind: 'item', id: 'run', label: 'Run', action }]);
    const opts = menuItemNew.mock.calls[0]![0] as { action: () => void };
    opts.action();
    expect(action).toHaveBeenCalledTimes(1);
  });
});

describe('createNativeContextMenuHandler', () => {
  it('prevents default, stops propagation, and shows menu', async () => {
    const handler = createNativeContextMenuHandler(() => [
      { kind: 'item', id: 'a', label: 'A', action: () => undefined },
    ]);
    const e = {
      preventDefault: vi.fn(),
      stopPropagation: vi.fn(),
    } as unknown as MouseEvent;
    handler(e);
    expect(e.preventDefault).toHaveBeenCalled();
    expect(e.stopPropagation).toHaveBeenCalled();
    await vi.waitFor(() => expect(menuPopup).toHaveBeenCalled());
  });

  it('can skip stopPropagation', async () => {
    const handler = createNativeContextMenuHandler(
      () => [{ kind: 'item', id: 'a', label: 'A', action: () => undefined }],
      { stopPropagation: false },
    );
    const e = {
      preventDefault: vi.fn(),
      stopPropagation: vi.fn(),
    } as unknown as MouseEvent;
    handler(e);
    expect(e.stopPropagation).not.toHaveBeenCalled();
    await vi.waitFor(() => expect(menuPopup).toHaveBeenCalled());
  });
});
