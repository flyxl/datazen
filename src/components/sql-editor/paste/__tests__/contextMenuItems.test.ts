import { describe, it, expect } from 'vitest';
import { createPasteAsInContextMenuItems, PASTE_AS_IN_MENU_KEYS } from '../contextMenuItems';

describe('contextMenuItems', () => {
  describe('createPasteAsInContextMenuItems', () => {
    it('returns null in fallback mode (Pro not registered)', () => {
      const group = createPasteAsInContextMenuItems();
      expect(group).toBeNull();
    });
  });

  describe('PASTE_AS_IN_MENU_KEYS', () => {
    it('exports stable i18n keys', () => {
      expect(PASTE_AS_IN_MENU_KEYS.group).toBe('query.editor.pasteAsIn.menu');
      expect(PASTE_AS_IN_MENU_KEYS.autoType).toBe('query.editor.pasteAsIn.autoType');
      expect(PASTE_AS_IN_MENU_KEYS.allStrings).toBe('query.editor.pasteAsIn.allStrings');
      expect(PASTE_AS_IN_MENU_KEYS.shortcutMac).toBe('query.editor.pasteAsIn.shortcutMac');
      expect(PASTE_AS_IN_MENU_KEYS.shortcutWin).toBe('query.editor.pasteAsIn.shortcutWin');
    });
  });
});
