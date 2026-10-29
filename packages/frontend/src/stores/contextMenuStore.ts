import { create } from 'zustand';
import { bindContextMenuBridge, normalizeNativeMenuItems } from '@datazen/driver-sdk';
import type { NativeMenuItemDef } from '@datazen/driver-sdk';

interface ContextMenuStore {
  open: boolean;
  x: number;
  y: number;
  items: NativeMenuItemDef[];
  show: (items: NativeMenuItemDef[], pos: { x: number; y: number }) => void;
  hide: () => void;
}

export const useContextMenuStore = create<ContextMenuStore>((set) => ({
  open: false,
  x: 0,
  y: 0,
  items: [],
  show: (items, pos) => {
    const normalized = normalizeNativeMenuItems(items);
    if (normalized.length === 0) {
      set({ open: false, items: [] });
      return;
    }
    set({ open: true, x: pos.x, y: pos.y, items: normalized });
  },
  hide: () => set({ open: false, items: [] }),
}));

export function showWebContextMenu(
  items: NativeMenuItemDef[],
  pos: { x: number; y: number },
): void {
  useContextMenuStore.getState().show(items, pos);
}

// Inject the web-menu mount point into @datazen/driver-sdk so host and
// driver code share one `showNativeContextMenu` implementation
// (schemaStoreBridge pattern).
bindContextMenuBridge({
  show: showWebContextMenu,
  hide: () => useContextMenuStore.getState().hide(),
});
