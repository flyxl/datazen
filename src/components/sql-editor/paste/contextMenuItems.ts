/**
 * Context menu item factory for Paste-as-IN (Enhanced extension).
 *
 * Returns null in fallback mode when sqlEditorEnhancedEP is not enhanced.
 */
import type { EditorView } from '@codemirror/view';
import { extensionRegistry, sqlEditorEnhancedEP } from '@datazen/extension-points';

export interface ContextMenuItem {
  label: string;
  accelerator?: string;
  action: (view: EditorView) => void;
}

export interface PasteAsInContextMenuGroup {
  label: string;
  items: ContextMenuItem[];
}

/**
 * Create the context menu items for Paste-as-IN when enhanced extension is active.
 */
export function createPasteAsInContextMenuItems(): PasteAsInContextMenuGroup | null {
  const enhanced = extensionRegistry.get(sqlEditorEnhancedEP);
  const group = enhanced.createPasteAsInContextMenuItems?.();
  return (group as PasteAsInContextMenuGroup | null) ?? null;
}

/**
 * Get the i18n keys used by context menu items.
 * External consumers can use these with t() for localization.
 */
export const PASTE_AS_IN_MENU_KEYS = {
  group: 'query.editor.pasteAsIn.menu',
  autoType: 'query.editor.pasteAsIn.autoType',
  allStrings: 'query.editor.pasteAsIn.allStrings',
  shortcutMac: 'query.editor.pasteAsIn.shortcutMac',
  shortcutWin: 'query.editor.pasteAsIn.shortcutWin',
} as const;
