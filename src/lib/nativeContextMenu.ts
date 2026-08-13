import type { MouseEvent as ReactMouseEvent } from 'react';

/** Edit / separator kinds shared by web context menus (legacy name kept for item defs). */
export type NativeMenuPredefined =
  | 'Separator'
  | 'Cut'
  | 'Copy'
  | 'Paste'
  | 'SelectAll'
  | 'Undo'
  | 'Redo';

export type NativeMenuItemDef =
  | {
      kind: 'item';
      id: string;
      label: string;
      enabled?: boolean;
      action: () => void | Promise<void>;
    }
  | { kind: 'separator'; id?: string }
  | { kind: 'predefined'; item: NativeMenuPredefined; text?: string }
  | { kind: 'submenu'; id?: string; label: string; items: NativeMenuItemDef[] };

export type ContextMenuPosition = { x: number; y: number };

/** Drop empty submenus and leading/trailing/duplicate separators. Disabled items are kept. */
export function normalizeNativeMenuItems(items: NativeMenuItemDef[]): NativeMenuItemDef[] {
  const filtered = items
    .map((def) => {
      if (def.kind === 'submenu') {
        const children = normalizeNativeMenuItems(def.items);
        if (children.length === 0) return null;
        return { ...def, items: children };
      }
      return def;
    })
    .filter((d): d is NativeMenuItemDef => d != null);

  const out: NativeMenuItemDef[] = [];
  for (const def of filtered) {
    const isSep =
      def.kind === 'separator' || (def.kind === 'predefined' && def.item === 'Separator');
    if (isSep) {
      if (out.length === 0) continue;
      const last = out[out.length - 1]!;
      const lastSep =
        last.kind === 'separator' || (last.kind === 'predefined' && last.item === 'Separator');
      if (lastSep) continue;
    }
    out.push(def);
  }
  while (out.length > 0) {
    const last = out[out.length - 1]!;
    const lastSep =
      last.kind === 'separator' || (last.kind === 'predefined' && last.item === 'Separator');
    if (!lastSep) break;
    out.pop();
  }
  return out;
}

/**
 * Show a web context menu at `pos` (client coordinates).
 * Call after `e.preventDefault()` on a `contextmenu` event.
 * Dynamic import avoids a cycle with `contextMenuStore` (which imports normalize).
 */
export function showNativeContextMenu(
  items: NativeMenuItemDef[],
  pos: ContextMenuPosition,
): void {
  void import('../stores/contextMenuStore').then(({ showWebContextMenu }) => {
    showWebContextMenu(items, pos);
  });
}

/** Standard edit block for text-focused surfaces (editor, inputs). */
export function nativeEditMenuItems(): NativeMenuItemDef[] {
  return [
    { kind: 'predefined', item: 'Cut' },
    { kind: 'predefined', item: 'Copy' },
    { kind: 'predefined', item: 'Paste' },
    { kind: 'predefined', item: 'SelectAll' },
  ];
}

/**
 * Build a React contextmenu handler that shows a web menu at the cursor.
 * Always preventDefault; optionally stopPropagation (default true) to avoid nested menus.
 */
export function createNativeContextMenuHandler(
  buildItems: () => NativeMenuItemDef[] | Promise<NativeMenuItemDef[]>,
  options?: { stopPropagation?: boolean },
): (e: ReactMouseEvent | MouseEvent) => void {
  const stop = options?.stopPropagation !== false;
  return (e: ReactMouseEvent | MouseEvent) => {
    e.preventDefault();
    if (stop) e.stopPropagation();
    const pos = { x: e.clientX, y: e.clientY };
    void (async () => {
      const items = await buildItems();
      showNativeContextMenu(items, pos);
    })();
  };
}
