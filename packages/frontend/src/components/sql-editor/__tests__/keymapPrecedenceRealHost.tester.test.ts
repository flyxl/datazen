/**
 * [tester] `createExtraKeymap` precedence against the *real* host composition.
 *
 * `proKeymapPrecedence.test.ts` builds its editor by hand:
 *
 *   keymap.of([...defaultKeymap, ...historyKeymap])
 *
 * which mirrors `createBaseEditorExtensions` at `editorExtensions.ts:286` but
 * drops the two other keymaps that factory registers (lines 259 and 333) and
 * `searchKeymap`. That is a fair simplification for the direction being
 * asserted — removing host keymaps can only make the Pro slot's win *harder*,
 * so it cannot manufacture a false green for `Prec.highest` beating
 * `defaultKeymap`.
 *
 * What it cannot cover is the opposite direction: a future host change that
 * raises one of its own keymaps above `Prec.highest` (or to `Prec.high`, which
 * ties) would put the host first on equal precedence, production would regress
 * to "the extension keymap silently never fires", and the hand-rolled fixture
 * would stay green because it never loads the real factory.
 *
 * So this file re-runs the contested chords through `createBaseEditorExtensions`
 * itself. It is the control for the control.
 */
import { afterEach, describe, expect, it } from 'vitest';
import { EditorState, type Extension } from '@codemirror/state';
import { EditorView, keymap, type KeyBinding } from '@codemirror/view';
import { extensionRegistry, sqlEditorEnhancedEP } from '@datazen/extension-points';
import { createBaseEditorExtensions } from '../editorExtensions';
import {
  createProKeymapExtension,
  mountProCompartments,
  type ProCompartmentPayload,
} from '../proCompartments';

const DOC = 'a\nb\nc';
/** Line 2, so `moveLineUp` has somewhere to go and its handler answers `true`. */
const CURSOR = 3;

const openViews: EditorView[] = [];

afterEach(() => {
  while (openViews.length) openViews.pop()?.destroy();
});

/**
 * The production order from `SqlEditor.tsx:442-467`: base extensions first,
 * then the Pro compartments.
 */
function createRealEditor(proSlot: Extension[]): EditorView {
  const payload: ProCompartmentPayload = { keymap: proSlot };
  const state = EditorState.create({
    doc: DOC,
    selection: { anchor: CURSOR },
    extensions: [
      ...createBaseEditorExtensions({
        onExecute: { current: undefined },
        onExecuteSelection: { current: undefined },
        onExecuteAll: { current: undefined },
        onSaveQuery: { current: undefined },
      }),
      ...mountProCompartments(payload),
    ],
  });
  const host = document.body.appendChild(document.createElement('div'));
  const view = new EditorView({ state, parent: host });
  openViews.push(view);
  return view;
}

function press(view: EditorView, init: KeyboardEventInit): void {
  view.contentDOM.dispatchEvent(
    new KeyboardEvent('keydown', { bubbles: true, cancelable: true, ...init }),
  );
}

function signature(view: EditorView): string {
  return `${view.state.doc.toString()}|${view.state.selection.ranges
    .map((r) => `${r.anchor}:${r.head}`)
    .join(',')}`;
}

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

const CONTESTED: ReadonlyArray<{ label: string; candidates: ReadonlyArray<KeyboardEventInit> }> = [
  {
    label: 'Shift-Alt-ArrowUp',
    candidates: [{ key: 'ArrowUp', code: 'ArrowUp', keyCode: 38, shiftKey: true, altKey: true }],
  },
  {
    label: 'Alt-ArrowUp',
    candidates: [{ key: 'ArrowUp', code: 'ArrowUp', keyCode: 38, altKey: true }],
  },
  {
    label: 'Mod-Alt-ArrowUp',
    candidates: [
      { key: 'ArrowUp', code: 'ArrowUp', keyCode: 38, altKey: true, ctrlKey: true },
      { key: 'ArrowUp', code: 'ArrowUp', keyCode: 38, altKey: true, metaKey: true },
    ],
  },
];

describe('[tester] createExtraKeymap vs the real createBaseEditorExtensions', () => {
  it.each(CONTESTED)('$label is owned by the real host keymap surface', ({ candidates }) => {
    // Probing happens on a bare editor: a view that already carries the Pro
    // slot would let the hook swallow the probe press and look unclaimed.
    expect(() => claimedByRealHost(candidates)).not.toThrow();
  });

  it.each(CONTESTED)('$label is won by the hook against the real host', ({ label, candidates }) => {
    const init = claimedByRealHost(candidates);
    const calls: string[] = [];
    const view = createRealEditor([
      proKeymapFromHook([{ key: label, run: () => (calls.push('pro'), true) }]),
    ]);
    const before = signature(view);

    press(view, init);

    expect(calls).toEqual(['pro']);
    // The Pro command claimed the keystroke, so the host command never ran and
    // nothing moved. This is the assertion the track exists to guarantee.
    expect(signature(view)).toBe(before);
  });

  it.each(CONTESTED)(
    'control: $label at default precedence is swallowed by the real host',
    ({ label, candidates }) => {
      // Proves the chords above are genuinely contested in the real
      // composition — without it, "the hook won" would be vacuous.
      const init = claimedByRealHost(candidates);
      const calls: string[] = [];
      const view = createRealEditor([
        keymap.of([{ key: label, run: () => (calls.push('pro'), true) }]),
      ]);
      const before = signature(view);

      press(view, init);

      expect(calls).toEqual([]);
      expect(signature(view)).not.toBe(before);
    },
  );

  it('the real host keymap still wins a chord the hook declines', () => {
    const init = claimedByRealHost(CONTESTED[1].candidates);
    const calls: string[] = [];
    const view = createRealEditor([
      proKeymapFromHook([{ key: 'Alt-ArrowUp', run: () => (calls.push('pro'), false) }]),
    ]);
    const before = signature(view);

    press(view, init);

    expect(calls).toEqual(['pro']);
    expect(signature(view)).not.toBe(before);
  });
});

/**
 * The event spelling the real host composition really claims for this chord,
 * measured on a throwaway editor that carries no Pro slot.
 */
function claimedByRealHost(candidates: ReadonlyArray<KeyboardEventInit>): KeyboardEventInit {
  const view = createRealEditor([]);
  try {
    for (const init of candidates) {
      const before = signature(view);
      press(view, init);
      if (signature(view) !== before) return init;
    }
  } finally {
    view.destroy();
    const index = openViews.indexOf(view);
    if (index >= 0) openViews.splice(index, 1);
  }
  throw new Error(
    'the real createBaseEditorExtensions does not claim this chord — the test would be vacuous',
  );
}
