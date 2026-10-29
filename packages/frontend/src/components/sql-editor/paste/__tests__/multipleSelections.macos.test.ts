/**
 * macOS branch of the multi-cursor keymap.
 *
 * CodeMirror's `browser.mac` is computed from `navigator.platform` at MODULE LOAD time
 * (`@codemirror/view`: `var browser = { mac: ... /Mac/.test(nav.platform) }`), and
 * `currentPlatform` then decides whether `Mod` normalizes to `Cmd` or `Ctrl`. jsdom reports
 * an empty `navigator.platform`, so the main test file exercises the non-mac branch only.
 *
 * `vi.hoisted` is hoisted above this file's `import` statements, so the stub is in place
 * when `@codemirror/view` (and every module that re-exports its keymap machinery) is first
 * evaluated. `afterAll` restores the original descriptor.
 */
import { describe, it, expect, afterAll, vi } from 'vitest';
import type { Extension } from '@codemirror/state';
// Type-only import: erased at compile time, so it cannot trigger the
// `@codemirror/view` module evaluation that `vi.hoisted` has to precede.
import type { EditorView as EditorViewType } from '@codemirror/view';

const platformBackup = vi.hoisted(() => {
  const own = Object.getOwnPropertyDescriptor(navigator, 'platform');
  Object.defineProperty(navigator, 'platform', { value: 'MacIntel', configurable: true });
  return own;
});

afterAll(() => {
  if (platformBackup) {
    Object.defineProperty(navigator, 'platform', platformBackup);
  } else {
    Reflect.deleteProperty(navigator, 'platform');
  }
});

const { EditorState } = await import('@codemirror/state');
const { EditorView, keymap } = await import('@codemirror/view');
const { searchKeymap } = await import('@codemirror/search');
const { defaultKeymap, historyKeymap } = await import('@codemirror/commands');
const { createMultipleSelectionsExtension } = await import('../multipleSelections');

/** Same real mount order as the SQL editor: defaultKeymap first, paste compartment last. */
function realOrderExtensions(): Extension[] {
  return [
    keymap.of([...defaultKeymap, ...historyKeymap, ...searchKeymap]),
    createMultipleSelectionsExtension(),
  ];
}

const DOC = 'SELECT a\nSELECT b\nSELECT c';

function mount(doc = DOC, anchor = 12): { view: EditorViewType; parent: HTMLElement } {
  const parent = document.createElement('div');
  document.body.appendChild(parent);
  const state = EditorState.create({
    doc,
    selection: { anchor, head: anchor },
    extensions: realOrderExtensions(),
  });
  return { view: new EditorView({ state, parent }), parent };
}

function press(view: EditorViewType, init: KeyboardEventInit): void {
  view.contentDOM.dispatchEvent(
    new KeyboardEvent('keydown', { bubbles: true, cancelable: true, ...init }),
  );
}

function destroy(view: EditorViewType, parent: HTMLElement): void {
  view.destroy();
  parent.remove();
}

/** Builds a view whose ONLY binding is `key`, and reports which chord reached it. */
function probeOnlyBinding(key: string, init: KeyboardEventInit): boolean {
  const parent = document.createElement('div');
  document.body.appendChild(parent);
  const hit: string[] = [];
  const state = EditorState.create({
    doc: 'x',
    extensions: [
      keymap.of([
        {
          key,
          run: (): boolean => {
            hit.push(key);
            return true;
          },
        },
      ]),
    ],
  });
  const view = new EditorView({ state, parent });
  try {
    press(view, init);
    return hit.length > 0;
  } finally {
    destroy(view, parent);
  }
}

describe('multipleSelections · macOS branch (navigator.platform = MacIntel)', () => {
  it('sanity: the mac branch is active, so `Mod` resolves to Cmd', () => {
    // Reaches `Mod-d` only when Mod normalized to Meta (mac).
    expect(probeOnlyBinding('Mod-d', { key: 'd', code: 'KeyD', metaKey: true })).toBe(true);
    // ...and must NOT reach it via Ctrl, which is what would happen if the
    // non-mac branch were active. Guards the vi.hoisted platform stub.
    expect(probeOnlyBinding('Mod-d', { key: 'd', code: 'KeyD', ctrlKey: true })).toBe(false);
  });

  it('Shift-Alt-ArrowUp (Option+Shift+Up) runs addCursorAbove, not copyLineUp', () => {
    const { view, parent } = mount();
    try {
      press(view, { key: 'ArrowUp', code: 'ArrowUp', shiftKey: true, altKey: true });
      expect(view.state.selection.ranges).toHaveLength(2);
      expect(view.state.doc.toString()).toBe(DOC);
    } finally {
      destroy(view, parent);
    }
  });

  it('Shift-Alt-ArrowDown (Option+Shift+Down) runs addCursorBelow, not copyLineDown', () => {
    const { view, parent } = mount(DOC, 4);
    try {
      press(view, { key: 'ArrowDown', code: 'ArrowDown', shiftKey: true, altKey: true });
      expect(view.state.selection.ranges).toHaveLength(2);
      expect(view.state.doc.toString()).toBe(DOC);
    } finally {
      destroy(view, parent);
    }
  });

  it('Alt-Cmd-ArrowUp (the mac variant of Alt-Mod-ArrowUp) still adds a cursor', () => {
    const { view, parent } = mount();
    try {
      press(view, { key: 'ArrowUp', code: 'ArrowUp', altKey: true, metaKey: true });
      expect(view.state.selection.ranges).toHaveLength(2);
      expect(view.state.doc.toString()).toBe(DOC);
    } finally {
      destroy(view, parent);
    }
  });

  it('Alt-Cmd-ArrowDown (mac variant of Alt-Mod-ArrowDown) still adds a cursor', () => {
    const { view, parent } = mount(DOC, 4);
    try {
      press(view, { key: 'ArrowDown', code: 'ArrowDown', altKey: true, metaKey: true });
      expect(view.state.selection.ranges).toHaveLength(2);
      expect(view.state.doc.toString()).toBe(DOC);
    } finally {
      destroy(view, parent);
    }
  });

  /* ------------------------------------------------------------------ */
  /*  copy line: Option+Shift+Up/Down is now multi-cursor, so copy line  */
  /*  needs a second home. Cmd+Shift+Up/Down CANNOT be used on macOS:    */
  /*  `standardKeymap` owns it as { mac: "Cmd-ArrowUp", shift:          */
  /*  selectDocStart }, i.e. Cmd+Shift+Up = select to start of document. */
  /*  Shadowing that would break a core macOS gesture, so the macOS      */
  /*  copy-line entry point is the guaranteed-free four-modifier chord  */
  /*  instead.                                                          */
  /* ------------------------------------------------------------------ */

  it('Cmd+Shift+ArrowUp keeps selecting to the start of the document (not copy line)', () => {
    const { view, parent } = mount(DOC, 20);
    try {
      press(view, { key: 'ArrowUp', code: 'ArrowUp', metaKey: true, shiftKey: true });
      // selectDocStart: a non-empty range from the document start to the old cursor.
      expect(view.state.selection.main.empty).toBe(false);
      expect(view.state.selection.main.from).toBe(0);
      expect(view.state.selection.main.to).toBe(20);
      expect(view.state.doc.toString()).toBe(DOC);
    } finally {
      destroy(view, parent);
    }
  });

  it('Alt-Cmd-Shift-ArrowUp (the free mac chord) copies the line up', () => {
    const { view, parent } = mount();
    try {
      press(view, { key: 'ArrowUp', code: 'ArrowUp', altKey: true, metaKey: true, shiftKey: true });
      // copyLineUp duplicates the current line above; the cursor stays put.
      expect(view.state.doc.toString()).toBe('SELECT a\nSELECT b\nSELECT b\nSELECT c');
      expect(view.state.selection.ranges).toHaveLength(1);
      expect(view.state.selection.main.head).toBe(12);
    } finally {
      destroy(view, parent);
    }
  });

  it('Alt-Cmd-Shift-ArrowDown (the free mac chord) copies the line down', () => {
    const { view, parent } = mount(DOC, 4);
    try {
      press(view, {
        key: 'ArrowDown',
        code: 'ArrowDown',
        altKey: true,
        metaKey: true,
        shiftKey: true,
      });
      expect(view.state.doc.toString()).toBe('SELECT a\nSELECT a\nSELECT b\nSELECT c');
      expect(view.state.selection.ranges).toHaveLength(1);
      expect(view.state.selection.main.head).toBe(13);
    } finally {
      destroy(view, parent);
    }
  });

  /* ------------------------------------------------------------------ */
  /*  Mod-d family on macOS: `Mod` normalizes to `Meta` here.            */
  /* ------------------------------------------------------------------ */

  it('resolves each Mod-d spelling to the binding a real mac event reaches', () => {
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
        destroy(view, parent);
      }
    };

    // Cmd+D: the shift-less lookup matches `Mod-d`.
    expect(probeFor({ key: 'd', code: 'KeyD', metaKey: true })).toBe('Mod-d');
    // Cmd+Shift+D: still the shift-less lookup, because for character keys
    // `runHandlers` strips Shift from the primary name.
    expect(probeFor({ key: 'D', code: 'KeyD', metaKey: true, shiftKey: true })).toBe('Mod-D');
    // w3c-keyname has a macOS-specific rule: for Cmd+Shift+<char> it IGNORES
    // `event.key` and reads `shift[event.keyCode]` instead. So even a key
    // reporting a foreign character still resolves to "Meta-D" and lands on
    // `Mod-D`. Consequence on macOS: `Shift-Mod-d` and `Shift-Mod-D` are
    // unreachable, and Cmd+Shift+D keeps working through `Mod-D`.
    // The non-mac branch is the mirror image (the rule is mac-only) and does
    // reach `Shift-Mod-d`; see multipleSelections.test.ts.
    expect(probeFor({ key: 'đ', code: 'KeyD', keyCode: 68, metaKey: true, shiftKey: true })).toBe(
      'Mod-D',
    );
  });

  it('Cmd+D still selects the next occurrence through the real mount order', () => {
    const doc = 'SELECT name, age, name FROM users WHERE name = 1';
    const { view, parent } = mount(doc, 9);
    try {
      press(view, { key: 'd', code: 'KeyD', metaKey: true });
      expect(view.state.selection.main.from).toBe(7);
      expect(view.state.selection.main.to).toBe(11);

      press(view, { key: 'd', code: 'KeyD', metaKey: true });
      expect(view.state.selection.ranges).toHaveLength(2);

      press(view, { key: 'D', code: 'KeyD', metaKey: true, shiftKey: true });
      expect(view.state.selection.ranges).toHaveLength(3);
    } finally {
      destroy(view, parent);
    }
  });
});
