/**
 * Web/context-menu item definitions shared by host and driver UIs.
 *
 * Type-only contract: the runtime helpers (show/hide/normalize) stay in the
 * host `src/lib/nativeContextMenu.ts` until the cap-bridge track.
 */

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
      shortcut?: string;
      enabled?: boolean;
      action: () => void | Promise<void>;
    }
  | { kind: 'separator'; id?: string }
  | { kind: 'predefined'; item: NativeMenuPredefined; text?: string }
  | { kind: 'submenu'; id?: string; label: string; items: NativeMenuItemDef[] };
