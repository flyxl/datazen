import { Menu, MenuItem, PredefinedMenuItem, Submenu } from '@tauri-apps/api/menu';
import type { MenuItem as TauriMenuItem } from '@tauri-apps/api/menu';
import type { PredefinedMenuItem as TauriPredefined } from '@tauri-apps/api/menu';
import type { Submenu as TauriSubmenu } from '@tauri-apps/api/menu';
import type { MouseEvent as ReactMouseEvent } from 'react';

/** OS / Tauri predefined menu item kinds we expose in context menus. */
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

type BuiltItem = TauriMenuItem | TauriPredefined | TauriSubmenu;

/** Drop leading/trailing/duplicate separators after filtering disabled items. */
export function normalizeNativeMenuItems(items: NativeMenuItemDef[]): NativeMenuItemDef[] {
  const filtered = items
    .map((def) => {
      if (def.kind === 'submenu') {
        const children = normalizeNativeMenuItems(def.items);
        if (children.length === 0) return null;
        return { ...def, items: children };
      }
      if (def.kind === 'item' && def.enabled === false) return null;
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

async function buildItem(def: NativeMenuItemDef): Promise<BuiltItem> {
  switch (def.kind) {
    case 'separator':
      return PredefinedMenuItem.new({ item: 'Separator' });
    case 'predefined':
      return PredefinedMenuItem.new({ item: def.item, text: def.text });
    case 'submenu': {
      const children = await Promise.all(normalizeNativeMenuItems(def.items).map(buildItem));
      return Submenu.new({ text: def.label, items: children });
    }
    case 'item':
      return MenuItem.new({
        id: def.id,
        text: def.label,
        enabled: def.enabled !== false,
        action: () => {
          void def.action();
        },
      });
  }
}

/**
 * Show a native (OS) context menu at the cursor.
 * Call after `e.preventDefault()` on a `contextmenu` event.
 */
export async function showNativeContextMenu(items: NativeMenuItemDef[]): Promise<void> {
  const normalized = normalizeNativeMenuItems(items);
  if (normalized.length === 0) return;
  const built = await Promise.all(normalized.map(buildItem));
  const menu = await Menu.new({ items: built });
  await menu.popup();
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
 * Build a React contextmenu handler that shows a native menu.
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
    void (async () => {
      const items = await buildItems();
      await showNativeContextMenu(items);
    })();
  };
}
