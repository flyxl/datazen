import { create } from 'zustand';
import type { NativeMenuItemDef } from '../lib/nativeContextMenu';
import { normalizeNativeMenuItems } from '../lib/nativeContextMenu';

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
