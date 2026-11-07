/**
 * The `createExtraKeymap` hook must not be preempted by `defaultKeymap`.
 *
 * This is the sharpest edge in the track. `createBaseEditorExtensions`
 * registers `keymap.of([...defaultKeymap, ...])` *before* any Pro compartment,
 * and these `defaultKeymap` commands return `true` whenever they do anything at
 * all:
 *
 *   Shift-Alt-ArrowUp / Down  → copyLineUp / copyLineDown
 *   Alt-ArrowUp / Down        → moveLineUp / moveLineDown
 *   Mod-Alt-ArrowUp / Down    → addCursorAbove / addCursorBelow  (multi-cursor)
 *
 * CodeMirror stops at the first keymap handler that returns `true`, so an
 * extension binding one of those chords at default precedence is never even
 * consulted: the keystroke is consumed by the host command and the extension
 * looks broken. The host therefore installs the hook's output under
 * `Prec.highest`.
 *
 * Each case is paired with a control that installs the *same* bindings at
 * default precedence. Without the control the positive assertion could not
 * distinguish "our keymap wins" from "this chord was never contested".
 */
import { afterEach, describe, expect, it } from 'vitest';
import { EditorState, Prec, type Extension } from '@codemirror/state';
import { EditorView, keymap, type KeyBinding } from '@codemirror/view';
import { defaultKeymap, history, historyKeymap } from '@codemirror/commands';
import { extensionRegistry, sqlEditorEnhancedEP } from '@datazen/extension-points';
import {
  createProKeymapExtension,
  mountProCompartments,
  reconfigureProCompartments,
  type ProCompartmentPayload,
} from '../proCompartments';

const DOC = 'a\nb\nc';
/** Cursor on line 2, so `moveLineUp` has somewhere to go and answers `true`. */
const CURSOR = 3;

const ARROW_UP = { key: 'ArrowUp', code: 'ArrowUp', keyCode: 38 } as const;
const ARROW_DOWN = { key: 'ArrowDown', code: 'ArrowDown', keyCode: 40 } as const;

/**
 * Chords `defaultKeymap` already owns, with the event spellings that could reach
 * them. `Mod` resolves to Ctrl or Meta depending on the platform, so both are
 * offered and the first one `defaultKeymap` actually answers is used.
 */
const CONTESTED: ReadonlyArray<{ label: string; candidates: ReadonlyArray<KeyboardEventInit> }> = [
  { label: 'Shift-Alt-ArrowUp', candidates: [{ ...ARROW_UP, shiftKey: true, altKey: true }] },
  { label: 'Alt-ArrowUp', candidates: [{ ...ARROW_UP, altKey: true }] },
  {
    // `addCursorAbove`. No `shift:` variant is declared for it, so a
    // Shift-modified event would not resolve to this binding at all.
    label: 'Mod-Alt-ArrowUp',
    candidates: [
      { ...ARROW_UP, altKey: true, ctrlKey: true },
      { ...ARROW_UP, altKey: true, metaKey: true },
    ],
  },
];

const openViews: EditorView[] = [];

afterEach(() => {
  while (openViews.length) openViews.pop()?.destroy();
});

/**
 * Editor mirroring the host's keymap surface: `defaultKeymap` + history
 * keymaps at default precedence, registered first, then the Pro slot — the same
 * order `SqlEditor` assembles them in.
 */
function createEditor(proSlot: Extension[]): EditorView {
  const payload: ProCompartmentPayload = { keymap: proSlot };
  const state = EditorState.create({
    doc: DOC,
    selection: { anchor: CURSOR },
    extensions: [
      history(),
      EditorState.allowMultipleSelections.of(true),
      keymap.of([...defaultKeymap, ...historyKeymap]),
      ...mountProCompartments(payload),
    ],
  });
  const host = document.createElement('div');
  document.body.appendChild(host);
  const view = new EditorView({ state, parent: host });
  openViews.push(view);
  return view;
}

/** Builds the Pro keymap through the real hook + registry, not a stand-in. */
function proKeymapFromHook(bindings: KeyBinding[]): Extension {
  const unregister = extensionRegistry.register(sqlEditorEnhancedEP, {
    createExtraKeymap: () => bindings,
  });
  try {
    return createProKeymapExtension();
  } finally {
    unregister();
  }
}

/**
 * Press a chord and report whether any `defaultKeymap` command answered.
 *
 * "Answered" is tracked as a doc + selection signature rather than a doc diff,
 * because not every contested command edits the document: `addCursorAbove`
 * only adds a cursor.
 */
function defaultKeymapAnswered(view: EditorView, init: KeyboardEventInit): boolean {
  const before = viewSignature(view);
  view.contentDOM.dispatchEvent(
    new KeyboardEvent('keydown', { bubbles: true, cancelable: true, ...init }),
  );
  return viewSignature(view) !== before;
}

function viewSignature(view: EditorView): string {
  return `${view.state.doc.toString()}|${view.state.selection.ranges
    .map((range) => `${range.anchor}:${range.head}`)
    .join(',')}`;
}

/** The event spelling that `defaultKeymap` really claims for this chord. */
function claimedEventFor(chord: { candidates: ReadonlyArray<KeyboardEventInit> }) {
  const view = createEditor([]);
  const claimed = chord.candidates.find((init) => defaultKeymapAnswered(view, init));
  if (!claimed) {
    throw new Error(
      `defaultKeymap does not claim this chord in this environment — the test would be vacuous`,
    );
  }
  return claimed;
}

describe('createExtraKeymap precedence', () => {
  describe.each(CONTESTED)('$label', ({ label, candidates }) => {
    it('is genuinely owned by defaultKeymap here (guards the cases below)', () => {
      const view = createEditor([]);
      expect(candidates.some((init) => defaultKeymapAnswered(view, init))).toBe(true);
    });

    it('is won by the hook when the host installs it (Prec.highest)', () => {
      const init = claimedEventFor({ candidates });
      const calls: string[] = [];
      const view = createEditor([
        proKeymapFromHook([{ key: label, run: () => (calls.push('pro'), true) }]),
      ]);

      const hostAnswered = defaultKeymapAnswered(view, init);

      expect(calls).toEqual(['pro']);
      // The Pro command handled the keystroke, so the host command never got it
      // and the document is untouched.
      expect(hostAnswered).toBe(false);
      expect(view.state.doc.toString()).toBe(DOC);
    });

    it('control: the same binding at default precedence is swallowed', () => {
      // Exactly the shape `createProKeymapExtension` would produce without its
      // `Prec.highest` wrapper, installed in the same position.
      const init = claimedEventFor({ candidates });
      const calls: string[] = [];
      const view = createEditor([
        keymap.of([{ key: label, run: () => (calls.push('pro'), true) }]),
      ]);
      const before = viewSignature(view);

      const hostAnswered = defaultKeymapAnswered(view, init);

      // defaultKeymap won by registration order and its handler returned
      // `true`, so the Pro handler is unreachable and the host command ran.
      expect(calls).toEqual([]);
      expect(hostAnswered).toBe(true);
      expect(viewSignature(view)).not.toBe(before);
    });
  });

  it('a hook handler that declines hands the chord back to the host binding', () => {
    // Prec.highest widens reach; it must not make the hook greedy.
    const claims = CONTESTED.map((chord) => ({
      label: chord.label,
      init: claimedEventFor(chord),
    }));
    const calls: string[] = [];
    const view = createEditor([
      proKeymapFromHook(
        claims.map(({ label }) => ({ key: label, run: () => (calls.push(label), false) })),
      ),
    ]);

    // Every contested chord reached the hook, every hook declined, and the host
    // commands that CM had been hiding still ran. (Not every press changes the
    // view: `moveLineUp` declines at the top of the document and
    // `addCursorAbove` with no line above, so only "at least one" is claimed.)
    const answered = claims.filter(({ init }) => defaultKeymapAnswered(view, init));

    expect(calls).toEqual(claims.map(({ label }) => label));
    expect(answered.length).toBeGreaterThan(0);
  });

  it('an EP that does not implement the hook contributes an empty keymap', () => {
    // Absent must behave exactly like the `() => []` fallback: inert, not broken.
    const unregister = extensionRegistry.register(sqlEditorEnhancedEP, {});
    let view: EditorView;
    try {
      view = createEditor([createProKeymapExtension()]);
    } finally {
      unregister();
    }
    expect(defaultKeymapAnswered(view, claimedEventFor(CONTESTED[0]))).toBe(true);
    expect(view.state.doc.toString()).not.toBe(DOC);
  });

  it('a hook that throws degrades to the fallback instead of breaking the keymap', () => {
    const unregister = extensionRegistry.register(sqlEditorEnhancedEP, {
      createExtraKeymap: () => {
        throw new Error('boom');
      },
    });
    let view: EditorView;
    try {
      view = createEditor([createProKeymapExtension()]);
    } finally {
      unregister();
    }
    // The keymap slot is empty, so the host keymap is still fully functional.
    expect(defaultKeymapAnswered(view, claimedEventFor(CONTESTED[0]))).toBe(true);
  });

  it('reconfigures the keymap slot together with the rest of the batch', () => {
    const calls: string[] = [];
    const slotFor = (tag: string): Extension[] => [
      proKeymapFromHook([{ key: 'Shift-Alt-ArrowUp', run: () => (calls.push(tag), true) }]),
    ];
    const payload: ProCompartmentPayload = { keymap: slotFor('first') };
    const state = EditorState.create({
      doc: DOC,
      selection: { anchor: CURSOR },
      extensions: [
        history(),
        EditorState.allowMultipleSelections.of(true),
        keymap.of([...defaultKeymap, ...historyKeymap]),
        ...mountProCompartments(payload),
      ],
    });
    const host = document.createElement('div');
    document.body.appendChild(host);
    const view = new EditorView({ state, parent: host });
    openViews.push(view);
    calls.length = 0;

    reconfigureProCompartments(view, { keymap: slotFor('second') });
    defaultKeymapAnswered(view, claimedEventFor(CONTESTED[0]));

    expect(calls).toEqual(['second']);
  });

  it('wins over an explicit equal-precedence competitor, not just over defaultKeymap', () => {
    // Precedence, not registration order, is what is being asserted.
    const calls: string[] = [];
    const view = createEditor([
      proKeymapFromHook([{ key: 'Alt-ArrowDown', run: () => (calls.push('pro'), true) }]),
      Prec.default(keymap.of([{ key: 'Alt-ArrowDown', run: () => (calls.push('peer'), true) }])),
    ]);

    view.contentDOM.dispatchEvent(
      new KeyboardEvent('keydown', {
        bubbles: true,
        cancelable: true,
        ...ARROW_DOWN,
        altKey: true,
      }),
    );

    expect(calls).toEqual(['pro']);
  });
});
