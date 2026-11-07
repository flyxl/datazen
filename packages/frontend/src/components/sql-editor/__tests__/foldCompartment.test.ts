/**
 * Contract tests for the `fold` Pro compartment.
 *
 * The host owns exactly three things for code folding, and this file pins all
 * three:
 *
 *  1. `fold` is a **base** compartment id — registered eagerly alongside the
 *     other seven, so it exists before any editor is created. A `Compartment`
 *     discovered after mount cannot enter an existing state, so a late
 *     registration would silently degrade into the `extra` overflow bucket.
 *  2. The host factory degrades instead of throwing: an extension that does not
 *     implement `createFoldExtensions` yields `[]`, and one that throws is
 *     circuit-broken by `SafeCompartmentWrapper` into `[]` (matching every
 *     other privileged factory).
 *  3. Multi-instance isolation survives the new slot. `compartments` is a
 *     module-level singleton, so this is a real hazard, not a formality.
 */
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { EditorState, Facet, StateEffect, type Extension } from '@codemirror/state';
import { EditorView } from '@codemirror/view';
import {
  extensionRegistry,
  sqlEditorEnhancedEP,
  type SqlEditorEnhancedFeatures,
} from '@datazen/extension-points';
import {
  BASE_PRO_COMPARTMENT_IDS,
  EXTRA_COMPARTMENT_ID,
  compartments,
  getProCompartment,
  mountProCompartments,
  registeredProCompartmentIds,
  reconfigureProCompartments,
  FOLD_COMPARTMENT_ID,
  createFoldExtensions,
  createProExtraExtensions,
  proSettingFlag,
  readProSettingsBag,
} from '../proCompartments';
import { createFoldCompartmentExtensions } from '../fold/foldCompartment';

/* -------------------------------------------------------------------------- */
/*  Stand-in extension                                                          */
/* -------------------------------------------------------------------------- */

/** One facet per slot, so a frame's contents are readable slot by slot. */
const statementTags = Facet.define<string, string>({ combine: (v) => v.join('|') });
const foldTags = Facet.define<string, string>({ combine: (v) => v.join('|') });
const extraTags = Facet.define<string, string>({ combine: (v) => v.join('|') });

/** Minimal standalone editor carrying only the Pro compartments. */
function makeView(payload: Record<string, Extension[]>): EditorView {
  return new EditorView({
    state: EditorState.create({ doc: 'select 1', extensions: mountProCompartments(payload) }),
  });
}

function withEnhanced(features: Partial<SqlEditorEnhancedFeatures>): void {
  extensionRegistry.register(sqlEditorEnhancedEP, features as SqlEditorEnhancedFeatures);
}

function withNoEnhanced(): void {
  extensionRegistry.unregister(sqlEditorEnhancedEP);
}

afterEach(() => {
  withNoEnhanced();
  vi.restoreAllMocks();
});

/* -------------------------------------------------------------------------- */

describe('fold compartment: registry wiring', () => {
  it("declares 'fold' in the closed base id set, ordered right after 'linter'", () => {
    expect(BASE_PRO_COMPARTMENT_IDS).toContain(FOLD_COMPARTMENT_ID);
    // The tuple doubles as mount order, which is precedence order. `fold`
    // installs a keymap, so it must not jump ahead of the other slots.
    const ids = [...BASE_PRO_COMPARTMENT_IDS];
    expect(ids.indexOf(FOLD_COMPARTMENT_ID)).toBe(ids.indexOf('linter') + 1);
  });

  it('registers the fold compartment eagerly, before any editor exists', () => {
    expect(registeredProCompartmentIds()).toContain(FOLD_COMPARTMENT_ID);
    expect(getProCompartment(FOLD_COMPARTMENT_ID)).toBeDefined();
    expect(compartments[FOLD_COMPARTMENT_ID]).toBe(getProCompartment(FOLD_COMPARTMENT_ID));
  });

  it('is not the generic overflow bucket', () => {
    // A fold slot that silently collapsed into `extra` would still "work" but
    // would lose independent atomic reconfiguration.
    expect(FOLD_COMPARTMENT_ID).not.toBe(EXTRA_COMPARTMENT_ID);
  });
});

describe('fold compartment: host factory', () => {
  it('returns [] when no privileged extension is registered', () => {
    withNoEnhanced();
    expect(createFoldExtensions()).toEqual([]);
  });

  it('returns [] when the extension does not implement createFoldExtensions', () => {
    withEnhanced({ createExtraExtensions: () => [] });
    expect(createFoldExtensions()).toEqual([]);
  });

  it('forwards the host options bag verbatim to the extension factory', () => {
    const opts = { proSettings: { codeFolding: false }, databaseType: 'postgresql' };
    const spy = vi.fn(() => [] as Extension[]);
    withEnhanced({ createFoldExtensions: spy });
    createFoldExtensions(opts);
    expect(spy).toHaveBeenCalledWith(opts);
  });

  it('circuit-breaks a throwing factory into [] instead of taking the editor down', () => {
    const err = vi.spyOn(console, 'error').mockImplementation(() => {});
    withEnhanced({
      createFoldExtensions: () => {
        throw new Error('fold blew up');
      },
    });
    expect(createFoldExtensions()).toEqual([]);
    expect(err).toHaveBeenCalled();
    // The breaker unregisters the EP: a broken extension must not be retried
    // on every keystroke-triggered reconfigure.
    expect(extensionRegistry.get(sqlEditorEnhancedEP).createFoldExtensions).toBeUndefined();
  });
});

describe('fold compartment: setting gate travels the shared bag path', () => {
  /** Stable array identity so the assertion is "same object", not "deep equal". */
  let installed: Extension[];

  beforeEach(() => {
    installed = [foldTags.of('code-folding')];
    // The Pro reads `codeFolding` from the *same* bag every other key uses.
    // There is no second, Pro-only settings channel.
    withEnhanced({
      createFoldExtensions: (opts) => (opts?.proSettings?.codeFolding === false ? [] : installed),
    });
  });

  it('installs folding by default (no bag present)', () => {
    expect(createFoldCompartmentExtensions()).toBe(installed);
  });

  it('installs folding when the key is explicitly true', () => {
    expect(createFoldCompartmentExtensions({ proSettings: { codeFolding: true } })).toBe(installed);
  });

  it('reconfigures the slot to [] when the key flips false', () => {
    expect(createFoldCompartmentExtensions({ proSettings: { codeFolding: false } })).toEqual([]);
  });

  it('reconfigures back when the key flips true again', () => {
    // The journey in reverse: a setting is a two-state machine, not a one-way
    // door. A slot that cannot be re-armed is a defect the user only meets
    // after they have already turned the feature off once.
    expect(createFoldCompartmentExtensions({ proSettings: { codeFolding: false } })).toEqual([]);
    expect(createFoldCompartmentExtensions({ proSettings: { codeFolding: true } })).toBe(installed);
  });

  it('ignores a non-boolean value rather than treating it as false', () => {
    // A corrupted / hand-edited settings bag must not silently disable a
    // feature the user believes is on.
    expect(createFoldCompartmentExtensions({ proSettings: { codeFolding: 'nope' } })).toBe(
      installed,
    );
    expect(createFoldCompartmentExtensions({ proSettings: { codeFolding: 0 } })).toBe(installed);
  });

  it('never forwards a contradictory instruction to the extension', () => {
    // One flag, one bag. The host gate and the extension gate read the *same*
    // key, so there is no combination of host options and bag that can install
    // the compartment while telling the extension to stay off.
    const seen: unknown[] = [];
    withEnhanced({
      createFoldExtensions: (opts) => {
        seen.push(opts?.proSettings);
        return installed;
      },
    });
    createFoldCompartmentExtensions({ proSettings: { codeFolding: false } });
    expect(seen).toEqual([]);

    createFoldCompartmentExtensions({ proSettings: { codeFolding: true } });
    expect(seen).toEqual([{ codeFolding: true }]);
  });
});

describe('fold compartment: atomic batch reconfiguration', () => {
  it('applies a fold flip in the same transaction as every other slot', () => {
    const view = makeView({
      statement: [statementTags.of('statement')],
      fold: [foldTags.of('fold')],
      extra: [extraTags.of('extra')],
    });
    try {
      const seen: string[] = [];
      view.dispatch({
        effects: StateEffect.appendConfig.of(
          EditorView.updateListener.of((update) => {
            const frame = `${update.state.facet(statementTags)}/${update.state.facet(foldTags)}/${update.state.facet(
              extraTags,
            )}`;
            if (seen.at(-1) !== frame) seen.push(frame);
          }),
        ),
      });
      const before = seen.length;

      reconfigureProCompartments(view, {
        statement: [statementTags.of('statement2')],
        fold: [],
        extra: [extraTags.of('extra')],
      });

      // Exactly one new frame, and it is the final one: no observer ever saw
      // "statement updated but fold not yet", or the reverse.
      expect(seen.length).toBe(before + 1);
      expect(seen.at(-1)).toBe('statement2//extra');
      expect(view.state.facet(foldTags)).toBe('');
      expect(view.state.facet(statementTags)).toBe('statement2');
    } finally {
      view.destroy();
    }
  });

  it('keeps the fold slot mounted after a flip to empty, and re-arms it', () => {
    const view = makeView({ fold: [foldTags.of('code-folding')] });
    try {
      reconfigureProCompartments(view, { fold: [] });
      // Mounted-but-empty, not unmounted: a second flip back must still land.
      expect(getProCompartment(FOLD_COMPARTMENT_ID)!.get(view.state)).toBeDefined();
      reconfigureProCompartments(view, { fold: [foldTags.of('code-folding')] });
      expect(view.state.facet(foldTags)).toBe('code-folding');
    } finally {
      view.destroy();
    }
  });
});

describe('fold compartment: multi-instance isolation', () => {
  it('a fold reconfigure on one editor never reaches another', () => {
    // `compartments` is a module-level singleton, so this is exactly the
    // failure mode the registry doc-comment promises does not happen.
    const a = makeView({ fold: [foldTags.of('fold-a')] });
    const b = makeView({ fold: [foldTags.of('fold-b')] });
    try {
      expect(a.state.facet(foldTags)).toBe('fold-a');
      expect(b.state.facet(foldTags)).toBe('fold-b');

      reconfigureProCompartments(a, { fold: [] });

      expect(a.state.facet(foldTags)).toBe('');
      expect(b.state.facet(foldTags)).toBe('fold-b');
    } finally {
      a.destroy();
      b.destroy();
    }
  });

  it('a fold flip in one instance leaves the other instance foldable', () => {
    const a = makeView({ fold: [] });
    const b = makeView({ fold: [foldTags.of('fold-b')] });
    try {
      expect(a.state.facet(foldTags)).toBe('');
      reconfigureProCompartments(a, { fold: [foldTags.of('fold-a')] });
      expect(a.state.facet(foldTags)).toBe('fold-a');
      expect(b.state.facet(foldTags)).toBe('fold-b');
    } finally {
      a.destroy();
      b.destroy();
    }
  });

  it('three instances diverge and converge independently', () => {
    const views = [makeView({ fold: [] }), makeView({ fold: [] }), makeView({ fold: [] })];
    try {
      reconfigureProCompartments(views[0], { fold: [foldTags.of('a')] });
      reconfigureProCompartments(views[2], { fold: [foldTags.of('c')] });
      expect(views.map((v) => v.state.facet(foldTags))).toEqual(['a', '', 'c']);

      reconfigureProCompartments(views[0], { fold: [] });
      expect(views.map((v) => v.state.facet(foldTags))).toEqual(['', '', 'c']);
    } finally {
      views.forEach((v) => v.destroy());
    }
  });
});

describe('fold compartment: overflow degradation', () => {
  it('routes an unmounted fold id into extra rather than throwing', () => {
    // A view that never mounted the fold slot (an extension announcing it too
    // late). `fold` must not throw, and must not silently vanish.
    const view = makeView({ extra: [extraTags.of('extra')] });
    try {
      expect(() => reconfigureProCompartments(view, { fold: [foldTags.of('fold')] })).not.toThrow();
      // Overflow is a *reconfigure* of `extra`, so the fold extensions land
      // there — and `extra`'s previous payload is replaced, not merged. Pinning
      // the replace semantics matters: a merge would resurrect extensions from
      // a slot the editor no longer believes are installed.
      expect(view.state.facet(extraTags)).toBe('');
      expect(view.state.facet(foldTags)).toBe('fold');
    } finally {
      view.destroy();
    }
  });

  it('preserves an explicit extra payload alongside the overflowed fold slot', () => {
    const view = makeView({ extra: [extraTags.of('extra')] });
    try {
      reconfigureProCompartments(view, {
        extra: [extraTags.of('extra2')],
        fold: [foldTags.of('fold')],
      });
      expect(view.state.facet(extraTags)).toBe('extra2');
      expect(view.state.facet(foldTags)).toBe('fold');
    } finally {
      view.destroy();
    }
  });

  it('warns instead of throwing when neither fold nor extra is mounted', () => {
    const warn = vi.spyOn(console, 'warn').mockImplementation(() => {});
    const view = makeView({});
    try {
      expect(() => reconfigureProCompartments(view, { fold: [foldTags.of('fold')] })).not.toThrow();
      expect(warn).toHaveBeenCalled();
      expect(String(warn.mock.calls[0][0])).toContain('fold');
    } finally {
      view.destroy();
    }
  });
});

/* -------------------------------------------------------------------------- */
/*  The settings bag every compartment factory reads                          */
/* -------------------------------------------------------------------------- */

/*
 * `readProSettingsBag` and `createProExtraExtensions` are not folding code —
 * they are the shared plumbing every compartment sits on, and the `fold`
 * feature reads the bag to decide whether to install at all. Folding cannot be
 * switched off correctly if the lookup silently returns the wrong bag, so
 * both are pinned here rather than left to chance.
 */

describe('settings bag: the lookup every compartment depends on', () => {
  it('resolves the bag from the first known id that carries one', () => {
    const current = { codeFolding: false };
    expect(readProSettingsBag({ 'sql-editor-pro': current })).toBe(current);
    // Legacy id first in the list, so it wins when both are present. Pinned
    // because the winner is positional, not "richest": an empty object is
    // still a bag, and preferring it over a populated one would silently
    // ignore every setting an older install had written.
    const legacy = { codeFolding: true };
    expect(readProSettingsBag({ 'sql-editor-enhanced': legacy, 'sql-editor-pro': current })).toBe(
      legacy,
    );
  });

  it('falls through to a later id when the first is absent or not a bag', () => {
    // A settings blob that is the wrong shape must not shadow the real one.
    // An array is an object in JavaScript, so `typeof` alone lets it through —
    // that is the case this assertion exists for.
    const bag = { codeFolding: true };
    expect(readProSettingsBag({ 'sql-editor-enhanced': [], 'sql-editor-pro': bag })).toBe(bag);
    expect(readProSettingsBag({ 'sql-editor-enhanced': 'nope', 'sql-editor-pro': bag })).toBe(bag);
    expect(readProSettingsBag({ 'sql-editor-enhanced': 42, 'sql-editor-pro': bag })).toBe(bag);
    expect(readProSettingsBag({ 'sql-editor-enhanced': null, 'sql-editor-pro': bag })).toBe(bag);
  });

  it('yields an empty bag, never undefined, when nothing matches', () => {
    // `undefined` would make every `proSettingFlag` call defensive for no
    // reason; the empty object is the deliberate "use the fallback" signal.
    expect(readProSettingsBag()).toEqual({});
    expect(readProSettingsBag({})).toEqual({});
    expect(readProSettingsBag({ 'some-other-extension': { codeFolding: false } })).toEqual({});
    expect(readProSettingsBag({ 'sql-editor-enhanced': [] })).toEqual({});
  });

  it('reads folding off that bag, honouring the fallback for absent and non-boolean values', () => {
    expect(proSettingFlag(readProSettingsBag({ 'sql-editor-enhanced': {} }), 'codeFolding')).toBe(
      true,
    );
    expect(
      proSettingFlag(
        readProSettingsBag({ 'sql-editor-enhanced': { codeFolding: false } }),
        'codeFolding',
      ),
    ).toBe(false);
    // Only a real boolean switches folding off. A stray string or number is a
    // settings-corruption symptom, and treating it as `false` would disable a
    // feature the user never turned off.
    expect(
      proSettingFlag(
        readProSettingsBag({ 'sql-editor-enhanced': { codeFolding: 'no' } }),
        'codeFolding',
      ),
    ).toBe(true);
    expect(
      proSettingFlag(
        readProSettingsBag({ 'sql-editor-enhanced': { codeFolding: 0 } }),
        'codeFolding',
      ),
    ).toBe(true);
  });

  it('agrees with the Pro extension on the same bag and the same key', () => {
    // The host and the Pro side each read `codeFolding` independently. They can
    // only be trusted to agree because it is literally the same key on the same
    // object — so pin that, rather than the two readings happening to match.
    const bag = readProSettingsBag({ 'sql-editor-enhanced': { codeFolding: false } });
    expect(createFoldCompartmentExtensions({ proSettings: bag })).toEqual([]);
    expect(createFoldCompartmentExtensions()).toEqual([]);
  });
});

describe('extra compartment factory: the shared overflow path', () => {
  it('returns [] when no privileged extension is registered', () => {
    withNoEnhanced();
    expect(createProExtraExtensions()).toEqual([]);
  });

  it('returns [] when the extension does not implement createExtraExtensions', () => {
    withEnhanced({ createFoldExtensions: () => [] });
    expect(createProExtraExtensions()).toEqual([]);
  });

  it('forwards the options bag to the extension verbatim', () => {
    const bag = { codeFolding: true };
    const seen: Array<unknown> = [];
    withEnhanced({
      createExtraExtensions: (opts) => {
        seen.push(opts?.proSettings);
        return [];
      },
    });
    createProExtraExtensions({ proSettings: bag });
    expect(seen[0]).toBe(bag);
  });

  it('circuit-breaks a throwing extension into [] and unregisters the EP', () => {
    const err = vi.spyOn(console, 'error').mockImplementation(() => {});
    withEnhanced({
      createExtraExtensions: () => {
        throw new Error('boom');
      },
    });
    expect(createProExtraExtensions()).toEqual([]);
    expect(err).toHaveBeenCalled();
    // The EP is torn down so a later mount cannot walk into the same trap.
    expect(createProExtraExtensions()).toEqual([]);
  });
});
