import { describe, it, expect, vi, beforeEach } from 'vitest';
import { EditorState, EditorSelection } from '@codemirror/state';
import {
  buildInClauseFromClipboard,
  pasteAsInCondition,
  createHostPasteAsInExtension,
} from '../hostPasteAsIn';

describe('hostPasteAsIn', () => {
  beforeEach(() => {
    vi.restoreAllMocks();
  });

  function mockClipboard(text: string | null) {
    Object.defineProperty(navigator, 'clipboard', {
      value: {
        readText: vi.fn().mockResolvedValue(text),
      },
      configurable: true,
      writable: true,
    });
  }

  function createHarness(doc: string, selection?: { anchor: number; head: number }) {
    let state = EditorState.create({
      doc,
      selection: selection ? EditorSelection.single(selection.anchor, selection.head) : undefined,
    });
    const view = {
      get state() {
        return state;
      },
      dispatch: (tr: any) => {
        state = tr.state ?? state.update(tr).state;
      },
    };
    return {
      view: view as any,
      get doc() {
        return state.doc.toString();
      },
      get selection() {
        return state.selection.main;
      },
    };
  }

  describe('buildInClauseFromClipboard', () => {
    it('returns null when clipboard is empty or whitespace', async () => {
      mockClipboard('');
      expect(await buildInClauseFromClipboard()).toBeNull();

      mockClipboard('   \n  ');
      expect(await buildInClauseFromClipboard()).toBeNull();
    });

    it('formats tab-delimited values into an IN clause', async () => {
      mockClipboard('apple\tbanana\tcherry');
      const clause = await buildInClauseFromClipboard();
      expect(clause).toBe("('apple', 'banana', 'cherry')");
    });

    it('formats newline-delimited values with numbers into an IN clause', async () => {
      mockClipboard('101\n202\n303');
      const clause = await buildInClauseFromClipboard();
      expect(clause).toBe('(101, 202, 303)');
    });

    it('supports all-strings value mode', async () => {
      mockClipboard('101\n202\nNULL');
      const clause = await buildInClauseFromClipboard('all-strings');
      expect(clause).toBe("('101', '202', 'NULL')");
    });
  });

  describe('pasteAsInCondition', () => {
    it('inserts IN clause at cursor position', async () => {
      mockClipboard('foo,bar');
      const harness = createHarness('SELECT * FROM t WHERE id IN ', { anchor: 28, head: 28 });

      const handled = await pasteAsInCondition(harness.view);
      expect(handled).toBe(true);
      expect(harness.doc).toBe("SELECT * FROM t WHERE id IN ('foo', 'bar')");
      expect(harness.selection.anchor).toBe(42);
    });

    it('replaces active selection with IN clause', async () => {
      mockClipboard('1\n2\n3');
      const harness = createHarness('SELECT * FROM t WHERE id IN (placeholder);', {
        anchor: 28,
        head: 41,
      });

      const handled = await pasteAsInCondition(harness.view);
      expect(handled).toBe(true);
      expect(harness.doc).toBe('SELECT * FROM t WHERE id IN (1, 2, 3);');
    });

    it('returns false and does not mutate document when clipboard has no values', async () => {
      mockClipboard('');
      const harness = createHarness('SELECT 1;');
      const handled = await pasteAsInCondition(harness.view);
      expect(handled).toBe(false);
      expect(harness.doc).toBe('SELECT 1;');
    });
  });

  describe('createHostPasteAsInExtension', () => {
    it('creates keymap extension for Mod-Shift-v', () => {
      const exts = createHostPasteAsInExtension();
      expect(exts).toBeDefined();
      expect(exts.length).toBeGreaterThan(0);
    });
  });
});
