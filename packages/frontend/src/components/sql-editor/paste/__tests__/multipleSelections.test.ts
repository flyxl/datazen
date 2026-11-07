import { describe, it, expect } from 'vitest';
import { EditorState } from '@codemirror/state';
import { EditorView, keymap } from '@codemirror/view';
import type { Extension } from '@codemirror/state';
import { selectNextOccurrence, searchKeymap } from '@codemirror/search';
import { defaultKeymap, historyKeymap } from '@codemirror/commands';
import { createMultipleSelectionsExtension } from '../multipleSelections';

/**
 * Faithful replica of the REAL mount order used by the SQL editor.
 *
 * `createBaseEditorExtensions()` (src/components/sql-editor/editorExtensions.ts) registers
 * `keymap.of([...defaultKeymap, ...historyKeymap, ...searchKeymap])` FIRST, while
 * `createMultipleSelectionsExtension()` only arrives much later through
 * `compartments.paste.of(...)` (src/components/sql-editor/SqlEditor.tsx).
 *
 * CodeMirror 6 keymap semantics: within one precedence, the binding registered first wins.
 * A standalone multi-cursor extension therefore NEVER reproduces the real dispatch — every
 * test in this file mounts the extensions in that real order.
 */
function realOrderExtensions(): Extension[] {
  return [
    keymap.of([...defaultKeymap, ...historyKeymap, ...searchKeymap]),
    createMultipleSelectionsExtension(),
  ];
}

/** Builds a real EditorView whose keymap precedence matches the SQL editor. */
function mountView(doc: string, anchor: number): { view: EditorView; parent: HTMLElement } {
  const parent = document.createElement('div');
  document.body.appendChild(parent);
  const state = EditorState.create({
    doc,
    selection: { anchor, head: anchor },
    extensions: realOrderExtensions(),
  });
  return { view: new EditorView({ state, parent }), parent };
}

function destroyView(view: EditorView, parent: HTMLElement): void {
  view.destroy();
  parent.remove();
}

/** Dispatches a genuine KeyboardEvent through CodeMirror's real keymap dispatch. */
function press(view: EditorView, init: KeyboardEventInit): KeyboardEvent {
  const event = new KeyboardEvent('keydown', { bubbles: true, cancelable: true, ...init });
  view.contentDOM.dispatchEvent(event);
  return event;
}

const DOC = 'SELECT a\nSELECT b\nSELECT c';

describe('multipleSelections', () => {
  it('selects word on first Mod-D and adds next occurrences on consecutive Mod-D', () => {
    const parent = document.createElement('div');
    document.body.appendChild(parent);

    const doc = 'SELECT name, age, name FROM users WHERE name = 1';
    const state = EditorState.create({
      doc,
      // Place cursor in the middle of the first 'name' (position 9)
      selection: { anchor: 9, head: 9 },
      extensions: createMultipleSelectionsExtension(),
    });

    const view = new EditorView({ state, parent });

    try {
      expect(view.state.selection.ranges).toHaveLength(1);
      expect(view.state.selection.main.empty).toBe(true);

      // First Mod-D: expands empty cursor to the full word 'name'
      const handled1 = selectNextOccurrence(view);
      expect(handled1).toBe(true);
      expect(view.state.selection.ranges).toHaveLength(1);
      expect(
        view.state.sliceDoc(view.state.selection.main.from, view.state.selection.main.to),
      ).toBe('name');
      expect(view.state.selection.main.from).toBe(7);
      expect(view.state.selection.main.to).toBe(11);

      // Second Mod-D: selects second 'name' (ranges has length 2)
      const handled2 = selectNextOccurrence(view);
      expect(handled2).toBe(true);
      expect(view.state.selection.ranges).toHaveLength(2);
      expect(
        view.state.sliceDoc(
          view.state.selection.ranges[0]!.from,
          view.state.selection.ranges[0]!.to,
        ),
      ).toBe('name');
      expect(
        view.state.sliceDoc(
          view.state.selection.ranges[1]!.from,
          view.state.selection.ranges[1]!.to,
        ),
      ).toBe('name');
      expect(view.state.selection.ranges[1]!.from).toBe(18);
      expect(view.state.selection.ranges[1]!.to).toBe(22);

      // Third Mod-D: selects third 'name' (ranges has length 3)
      const handled3 = selectNextOccurrence(view);
      expect(handled3).toBe(true);
      expect(view.state.selection.ranges).toHaveLength(3);
      expect(view.state.selection.ranges[2]!.from).toBe(40);
      expect(view.state.selection.ranges[2]!.to).toBe(44);
    } finally {
      view.destroy();
      parent.remove();
    }
  });

  it('triggers via keydown event on contentDOM with the multi-cursor extension alone', () => {
    const parent = document.createElement('div');
    document.body.appendChild(parent);

    const doc = 'SELECT name, age, name FROM users WHERE name = 1';
    const state = EditorState.create({
      doc,
      selection: { anchor: 9, head: 9 },
      extensions: createMultipleSelectionsExtension(),
    });

    const view = new EditorView({ state, parent });

    try {
      // Dispatch Cmd+D (Meta+d on macOS)
      const event1 = new KeyboardEvent('keydown', {
        key: 'd',
        code: 'KeyD',
        metaKey: true,
        bubbles: true,
        cancelable: true,
      });
      view.contentDOM.dispatchEvent(event1);

      expect(view.state.selection.ranges).toHaveLength(1);
      expect(view.state.selection.main.from).toBe(7);
      expect(view.state.selection.main.to).toBe(11);

      // Dispatch Cmd+D second time
      const event2 = new KeyboardEvent('keydown', {
        key: 'd',
        code: 'KeyD',
        metaKey: true,
        bubbles: true,
        cancelable: true,
      });
      view.contentDOM.dispatchEvent(event2);

      expect(view.state.selection.ranges).toHaveLength(2);
    } finally {
      view.destroy();
      parent.remove();
    }
  });

  /* ====================================================================== */
  /*  REGRESSION: Shift-Alt-ArrowUp/Down must reach addCursorAbove/Below.   */
  /*                                                                      */
  /*  @codemirror/commands defaultKeymap owns these two chords for         */
  /*  copyLineUp/copyLineDown. The bindings carry NO `mac:` variant, so the */
  /*  conflict exists on EVERY platform, and defaultKeymap is registered    */
  /*  BEFORE the paste compartment. These two bindings were dead.          */
  /* ====================================================================== */

  it('Shift-Alt-ArrowUp runs addCursorAbove instead of copyLineUp under the real mount order', () => {
    const { view, parent } = mountView(DOC, 12);
    try {
      expect(view.state.selection.ranges).toHaveLength(1);

      // Real event combination for the `Shift-Alt-ArrowUp` chord.
      press(view, { key: 'ArrowUp', code: 'ArrowUp', shiftKey: true, altKey: true });

      // addCursorAbove: one extra range. The exact position is NOT asserted
      // because `view.moveVertically` needs layout, which jsdom does not
      // provide (it degrades to the document start/end). The doc text is
      // unchanged, which is what actually distinguishes addCursor* (selection
      // only) from copyLineUp/Down (duplicates a line).
      expect(view.state.selection.ranges).toHaveLength(2);
      expect(view.state.doc.toString()).toBe(DOC);
    } finally {
      destroyView(view, parent);
    }
  });

  it('Shift-Alt-ArrowDown runs addCursorBelow instead of copyLineDown under the real mount order', () => {
    const { view, parent } = mountView(DOC, 4);
    try {
      press(view, { key: 'ArrowDown', code: 'ArrowDown', shiftKey: true, altKey: true });

      expect(view.state.selection.ranges).toHaveLength(2);
      expect(view.state.doc.toString()).toBe(DOC);
    } finally {
      destroyView(view, parent);
    }
  });

  it('Alt-Mod-ArrowUp/Down (the non-shift add-cursor chord) still add a cursor', () => {
    const up = mountView(DOC, 12);
    try {
      press(up.view, { key: 'ArrowUp', code: 'ArrowUp', altKey: true, ctrlKey: true });
      expect(up.view.state.selection.ranges).toHaveLength(2);
      expect(up.view.state.doc.toString()).toBe(DOC);
    } finally {
      destroyView(up.view, up.parent);
    }

    const down = mountView(DOC, 4);
    try {
      press(down.view, { key: 'ArrowDown', code: 'ArrowDown', altKey: true, ctrlKey: true });
      expect(down.view.state.selection.ranges).toHaveLength(2);
      expect(down.view.state.doc.toString()).toBe(DOC);
    } finally {
      destroyView(down.view, down.parent);
    }
  });

  /* ====================================================================== */
  /*  JOURNEY (continuous-journey rule): the full state machine.            */
  /*                                                                      */
  /*  enter  : Shift-Alt-ArrowUp on a middle line                           */
  /*  inside : press it again -> 3 cursors; backspace deletes at every      */
  /*           cursor in one keystroke                                     */
  /*  exit   : keep backspacing until the document is empty, at which point */
  /*           CodeMirror merges the coincident empty ranges back into one  */
  /* ====================================================================== */

  it('journey: add cursors up and down -> backspace through the whole doc -> single cursor', () => {
    const { view, parent } = mountView(DOC, 12);
    try {
      // 1. ENTER multi-cursor: Option+Shift+Down adds a cursor below.
      expect(view.state.selection.ranges).toHaveLength(1);
      expect(view.state.selection.main.head).toBe(12);
      press(view, { key: 'ArrowDown', code: 'ArrowDown', shiftKey: true, altKey: true });
      expect(view.state.selection.ranges).toHaveLength(2);
      expect(view.state.doc.toString()).toBe(DOC);

      // 2. STAY inside: Option+Shift+Up adds a third cursor, document still
      //    untouched (addCursor* only move the selection).
      press(view, { key: 'ArrowUp', code: 'ArrowUp', shiftKey: true, altKey: true });
      expect(view.state.selection.ranges).toHaveLength(3);
      expect(view.state.doc.toString()).toBe(DOC);

      // 3. A single Backspace now applies at every cursor at once, and the
      //    three cursors survive the edit. How many characters go is NOT
      //    asserted: `addCursor*` lands on positions produced by
      //    `view.moveVertically`, which needs layout that jsdom does not
      //    provide. What must hold everywhere is that the edit is applied
      //    across the whole multi-cursor selection.
      press(view, { key: 'Backspace', code: 'Backspace' });
      expect(view.state.selection.ranges).toHaveLength(3);
      expect(view.state.doc.length).toBeLessThan(DOC.length);

      // 4. Keep editing until the document is gone.
      for (let i = 0; i < DOC.length + 4; i++) {
        press(view, { key: 'Backspace', code: 'Backspace' });
      }
      expect(view.state.doc.length).toBe(0);

      // 5. EXIT: with nothing left to select, the coincident cursors collapse
      //    back to a single empty cursor — the state the journey started in.
      expect(view.state.selection.ranges).toHaveLength(1);
      expect(view.state.selection.main.empty).toBe(true);
      expect(view.state.selection.main.head).toBe(0);
    } finally {
      destroyView(view, parent);
    }
  });

  /* ====================================================================== */
  /*  copy line survived the takeover (blast-radius self-check #2).         */
  /*  On Windows/Linux Ctrl+Shift+Up/Down was free, so copyLineUp/Down     */
  /*  moves there. It is registered at NORMAL precedence on purpose: the   */
  /*  macOS branch of that chord is `Cmd+Shift+Up` = select to document    */
  /*  start (standardKeymap), and shadowing that would break a core macOS  */
  /*  gesture. See multipleSelections.macos.test.ts for the mac entry.      */
  /* ====================================================================== */

  it('Mod-Shift-ArrowUp copies the line up on non-mac platforms', () => {
    const { view, parent } = mountView(DOC, 12);
    try {
      press(view, { key: 'ArrowUp', code: 'ArrowUp', ctrlKey: true, shiftKey: true });
      expect(view.state.doc.toString()).toBe('SELECT a\nSELECT b\nSELECT b\nSELECT c');
      expect(view.state.selection.ranges).toHaveLength(1);
    } finally {
      destroyView(view, parent);
    }
  });

  it('Mod-Shift-ArrowDown copies the line down on non-mac platforms', () => {
    const { view, parent } = mountView(DOC, 4);
    try {
      press(view, { key: 'ArrowDown', code: 'ArrowDown', ctrlKey: true, shiftKey: true });
      expect(view.state.doc.toString()).toBe('SELECT a\nSELECT a\nSELECT b\nSELECT c');
      expect(view.state.selection.ranges).toHaveLength(1);
    } finally {
      destroyView(view, parent);
    }
  });

  /* ====================================================================== */
  /*  Mod-d family: which spelling actually matches a real KeyboardEvent?   */
  /*                                                                      */
  /*  Method: a PROBE view whose keymap is a byte-for-byte replica of the   */
  /*  four Mod-d spellings, each with a spy command. The spy records which  */
  /*  binding the event reached, so the answer comes from real EditorView + */
  /*  real KeyboardEvent dispatch, not from reading source.                 */
  /*                                                                      */
  /*  PLATFORM: jsdom reports no navigator.platform, so CodeMirror's        */
  /*  `currentPlatform` is "key" here and `Mod` normalizes to Ctrl. The    */
  /*  macOS branch (Mod === Cmd) is covered by multipleSelections.macos.    */
  /*                                                                      */
  /*  `keyCode` is only supplied where w3c-keyname's `base`/`shift` keyCode  */
  /*  tables are actually consulted as fallbacks; for the plain "d"/"D"       */
  /*  events the browser-reported `event.key` is used directly. A real        */
  /*  browser reports key "d" without Shift and key "D" with Shift on a US    */
  /*  layout.                                                               */
  /* ====================================================================== */

  it('resolves each Mod-d spelling to the binding a real event reaches', () => {
    const variants = ['Mod-d', 'Mod-D', 'Shift-Mod-d', 'Shift-Mod-D'] as const;
    const probeFor = (init: KeyboardEventInit): string | null => {
      const parent = document.createElement('div');
      document.body.appendChild(parent);
      const hit: string[] = [];
      const state = EditorState.create({
        doc: 'x',
        extensions: [
          keymap.of(
            variants.map((name) => ({
              key: name,
              run: (): boolean => {
                hit.push(name);
                return true;
              },
              preventDefault: true,
            })),
          ),
        ],
      });
      const view = new EditorView({ state, parent });
      try {
        press(view, init);
        return hit[0] ?? null;
      } finally {
        destroyView(view, parent);
      }
    };

    // Ctrl+D, as a browser reports it without Shift -> `Mod-d`.
    expect(probeFor({ key: 'd', code: 'KeyD', ctrlKey: true })).toBe('Mod-d');
    // Ctrl+Shift+D, as a browser reports it WITH Shift (key "D") -> `Mod-D`.
    // `runHandlers` tries the shift-less name first for character keys, so the
    // shift-prefixed spellings are not even consulted here.
    expect(probeFor({ key: 'D', code: 'KeyD', ctrlKey: true, shiftKey: true })).toBe('Mod-D');
    // CapsLock reports "D" with Shift off -> also `Mod-D`.
    expect(probeFor({ key: 'D', code: 'KeyD', ctrlKey: true })).toBe('Mod-D');
    // Non-US layout: the physical key reports a different character, so the
    // shift-less lookup misses and the fallback resolves through
    // `base[keyCode]` (base[68] === "d"). This is the ONLY way `Shift-Mod-d`
    // is ever reached — a layout safety net, not dead weight. The event must
    // carry the keyCode a real browser always sends.
    expect(probeFor({ key: 'đ', code: 'KeyD', keyCode: 68, ctrlKey: true, shiftKey: true })).toBe(
      'Shift-Mod-d',
    );
  });

  it('keeps the Mod-d family working end to end under the real mount order', () => {
    const doc = 'SELECT name, age, name FROM users WHERE name = 1';
    const { view, parent } = mountView(doc, 9);
    try {
      press(view, { key: 'd', code: 'KeyD', ctrlKey: true });
      expect(view.state.selection.main.from).toBe(7);
      expect(view.state.selection.main.to).toBe(11);

      press(view, { key: 'd', code: 'KeyD', ctrlKey: true });
      expect(view.state.selection.ranges).toHaveLength(2);

      press(view, { key: 'D', code: 'KeyD', ctrlKey: true, shiftKey: true });
      expect(view.state.selection.ranges).toHaveLength(3);
    } finally {
      destroyView(view, parent);
    }
  });
});
