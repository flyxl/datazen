/**
 * Web context-menu helpers. The single implementation lives in
 * `@datazen/driver-sdk` (nativeContextMenu module + `bindContextMenuBridge`);
 * this thin host module keeps existing imports stable. The runtime binding to
 * `contextMenuStore` happens in `src/stores/contextMenuStore.ts`.
 */
export {
  showNativeContextMenu,
  hideNativeContextMenu,
  normalizeNativeMenuItems,
  nativeEditMenuItems,
  createNativeContextMenuHandler,
} from '@datazen/driver-sdk';
export type { ContextMenuPosition, ContextMenuBridge } from '@datazen/driver-sdk';
export type { NativeMenuItemDef, NativeMenuPredefined } from '@datazen/driver-sdk';
