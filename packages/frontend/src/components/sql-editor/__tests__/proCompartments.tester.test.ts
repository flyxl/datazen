/**
 * [tester] G3 settings-bag plumbing and the late-slot merge.
 *
 * Two gaps the track's own suites left, both measured rather than guessed:
 *
 * 1. `readProSettingsBag` / `proSettingFlag` are the seam that makes *any* key an
 *    extension declares in `settingsContributions` reach the editor. Their
 *    precedence rules and degenerate cases were never asserted directly, so a
 *    refactor could silently flip which namespace wins.
 * 2. The `overflow` merge inside `reconfigureProCompartments`
 *    (`proCompartments.ts:167-181`) is the documented "an extension announced a
 *    slot after the view was built" path. A coverage run showed its lines
 *    uncovered from the branch that runs *with* `extra` mounted.
 */
import { describe, expect, it } from 'vitest';
import { EditorState, Facet, type Extension } from '@codemirror/state';
import { EditorView } from '@codemirror/view';
import {
  ensureProCompartment,
  EXTRA_COMPARTMENT_ID,
  proSettingFlag,
  readProSettingsBag,
  reconfigureProCompartments,
  type ProCompartmentPayload,
} from '../proCompartments';

const tag = Facet.define<string, readonly string[]>({ combine: (values) => values });

/** The ordered contents of every slot visible to this view. */
function slotContents(view: EditorView): readonly string[] {
  return view.state.facet(tag);
}

const slot = (value: string): Extension => [tag.of(value)];

function makeView(payload: ProCompartmentPayload): EditorView {
  const state = EditorState.create({
    doc: 'a\nb\nc',
    extensions: [...Object.keys(payload).map((id) => ensureProCompartment(id).of(payload[id]))],
  });
  const host = document.body.appendChild(document.createElement('div'));
  return new EditorView({ state, parent: host });
}

describe('[tester] readProSettingsBag', () => {
  it('prefers the enhanced namespace, then falls back to the pro namespace', () => {
    const enhanced = { a: 1 };
    const pro = { b: 2 };
    expect(readProSettingsBag({ 'sql-editor-enhanced': enhanced, 'sql-editor-pro': pro })).toBe(
      enhanced,
    );
    expect(readProSettingsBag({ 'sql-editor-pro': pro })).toBe(pro);
  });

  it('skips a namespace whose value is not a plain object bag', () => {
    // A corrupted settings file must not become `opts.proSettings`: reading a
    // string bag would make `proSettingFlag` see `undefined` for every key and
    // silently fall back to defaults — the hardest kind of settings bug.
    const pro = { pasteAsIn: false };
    expect(readProSettingsBag({ 'sql-editor-enhanced': 'nope', 'sql-editor-pro': pro })).toBe(pro);
    expect(readProSettingsBag({ 'sql-editor-enhanced': [1, 2], 'sql-editor-pro': pro })).toBe(pro);
    expect(readProSettingsBag({ 'sql-editor-enhanced': null, 'sql-editor-pro': pro })).toBe(pro);
  });

  it('returns one stable empty bag, never a fresh object', () => {
    // A fresh `{}` per read would give `proPayload` a new identity on every
    // store read and spin the reconfigure effect forever.
    const a = readProSettingsBag(undefined);
    const b = readProSettingsBag({});
    const c = readProSettingsBag({ 'sql-editor-enhanced': null });
    expect(a).toBe(b);
    expect(b).toBe(c);
    expect(a).toEqual({});
  });

  it('an empty but present bag still wins over the fallback namespace', () => {
    // First-match-wins by *presence*, not by truthiness: a user who cleared
    // every setting must not have the other namespace's stale bag reappear.
    expect(readProSettingsBag({ 'sql-editor-enhanced': {}, 'sql-editor-pro': { a: 1 } })).toEqual(
      {},
    );
  });
});

describe('[tester] proSettingFlag', () => {
  it('honours an explicit boolean in either position', () => {
    expect(proSettingFlag({ on: true, off: false }, 'on')).toBe(true);
    expect(proSettingFlag({ on: true, off: false }, 'off')).toBe(false);
  });

  it('uses the caller-supplied fallback when the key is absent', () => {
    expect(proSettingFlag({}, 'x', false)).toBe(false);
    expect(proSettingFlag({}, 'x')).toBe(true);
    expect(proSettingFlag(undefined, 'x')).toBe(true);
  });

  it('treats a non-boolean value as absent rather than truthy', () => {
    // `'false'` from a hand-edited settings file is the trap: truthy strings
    // would read as "on" forever with no way to turn the feature off.
    expect(proSettingFlag({ x: 'false' }, 'x', true)).toBe(true);
    expect(proSettingFlag({ x: 'false' }, 'x', false)).toBe(false);
    expect(proSettingFlag({ x: 0 }, 'x', true)).toBe(true);
    expect(proSettingFlag({ x: null }, 'x', true)).toBe(true);
  });
});

// [tester] Coverage caveat, not an assertion: every case in this block puts
// `extra` in the payload, so none of them would catch the removal of the
// `payload[EXTRA_COMPARTMENT_ID] ?? []` fallback in `reconfigureProCompartments`.
// That fallback's only guard today is the single case
// `'merges overflow into a mounted \`extra\` the payload never names'` in
// `proCompartments.test.ts`. If you rework these cases, keep at least one that
// omits `extra` from the payload — otherwise that fallback silently loses cover.
describe('[tester] late-announced slots merge into `extra`', () => {
  it('folds an unregistered id into the mounted `extra` slot, preserving order', () => {
    const view = makeView({ [EXTRA_COMPARTMENT_ID]: [slot('base')] });
    try {
      reconfigureProCompartments(view, {
        [EXTRA_COMPARTMENT_ID]: [slot('extra')],
        'late-a': [slot('late-a')],
        'late-b': [slot('late-b')],
      });
      // `extra`'s own content comes first, then the unmounted ids in payload
      // order — the order an extension reads its own slots back in.
      expect(slotContents(view)).toEqual(['extra', 'late-a', 'late-b']);
    } finally {
      view.destroy();
    }
  });

  it('does not duplicate `extra` when it is also present in the payload', () => {
    const view = makeView({ [EXTRA_COMPARTMENT_ID]: [slot('base')] });
    try {
      // `extra` is mounted, so it is applied through its own compartment and
      // cannot also land in the overflow list — otherwise its content would be
      // merged into itself.
      reconfigureProCompartments(view, {
        [EXTRA_COMPARTMENT_ID]: [slot('only-once')],
        late: [slot('late')],
      });
      expect(slotContents(view)).toEqual(['only-once', 'late']);
    } finally {
      view.destroy();
    }
  });

  it('a late id with no content contributes nothing instead of failing the batch', () => {
    const view = makeView({ [EXTRA_COMPARTMENT_ID]: [slot('base')] });
    try {
      expect(() =>
        reconfigureProCompartments(view, {
          [EXTRA_COMPARTMENT_ID]: [slot('kept')],
          late: [],
        }),
      ).not.toThrow();
      expect(slotContents(view)).toEqual(['kept']);
    } finally {
      view.destroy();
    }
  });

  it('an id that is registered but not in this view is still treated as late', () => {
    // `ensureProCompartment` alone is not enough — only a compartment present
    // in the state can be reconfigured. This is the distinction the code
    // comment claims and the one a reader is most likely to get wrong.
    ensureProCompartment('registered-but-unmounted');
    const view = makeView({ [EXTRA_COMPARTMENT_ID]: [slot('base')] });
    try {
      reconfigureProCompartments(view, {
        [EXTRA_COMPARTMENT_ID]: [slot('still-here')],
        'registered-but-unmounted': [slot('late-anyway')],
      });
      expect(slotContents(view)).toEqual(['still-here', 'late-anyway']);
    } finally {
      view.destroy();
    }
  });
});
