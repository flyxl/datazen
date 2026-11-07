/**
 * Contract tests for the extensible Pro compartment registry.
 *
 * These justify the design rather than restate it:
 *  - a *new* compartment id is consumable purely by traversal (G2);
 *  - one settings change produces exactly one transaction, and no observer can
 *    see a half-applied frame (G2, atomicity) — with a control case that shows
 *    the intermediate frames the old six-dispatch code really did expose;
 *  - the shared module-level registry cannot leak configuration between two
 *    live editor instances.
 */
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { EditorState, Facet, StateEffect, Transaction, type Extension } from '@codemirror/state';
import { EditorView } from '@codemirror/view';
import {
  compartments,
  ensureProCompartment,
  getProCompartment,
  mountProCompartments,
  registeredProCompartmentIds,
  reconfigureProCompartments,
  BASE_PRO_COMPARTMENT_IDS,
  EXTRA_COMPARTMENT_ID,
  type ProCompartmentPayload,
} from '../proCompartments';

/** Stand-in for a real privileged extension: labels the slot it is installed in. */
const tags = Facet.define<string, string>({ combine: (values) => values.join('|') });

/** The stand-in for a privileged extension is the facet tag itself, so the
 * observable "what is installed right now" is `state.facet(tags)`. */
const slot = (name: string): Extension => tags.of(name);

/** Minimal standalone editor carrying only the Pro compartments. */
function makeView(payload: ProCompartmentPayload): EditorView {
  const state = EditorState.create({ doc: 'select 1', extensions: mountProCompartments(payload) });
  return new EditorView({ state });
}

interface UpdateProbe {
  /** Every distinct configuration an observer saw, in order. */
  seen: string[];
  /** Forget the updates produced while installing the probe itself. */
  reset(): void;
}

function installProbe(view: EditorView): UpdateProbe {
  const seen: string[] = [];
  view.dispatch({
    effects: StateEffect.appendConfig.of(
      EditorView.updateListener.of((update) => {
        const value = update.state.facet(tags);
        if (seen.at(-1) !== value) seen.push(value);
      }),
    ),
  });
  return { seen, reset: () => void (seen.length = 0) };
}

describe('proCompartments registry', () => {
  let view: EditorView | null = null;

  beforeEach(() => {
    view = null;
  });

  afterEach(() => {
    view?.destroy();
    view = null;
  });

  it('registers every base id eagerly so mount order is deterministic', () => {
    for (const id of BASE_PRO_COMPARTMENT_IDS) {
      expect(getProCompartment(id)).toBeDefined();
    }
    expect(getProCompartment(EXTRA_COMPARTMENT_ID)).toBeDefined();
    // `compartments` is the historical property-access surface; it must stay in
    // sync with the registry for the old `@/editorExtensions` call sites.
    expect(compartments.statement).toBe(getProCompartment('statement'));
  });

  it('is idempotent: re-registering an id returns the same Compartment', () => {
    const first = ensureProCompartment('statement');
    expect(ensureProCompartment('statement')).toBe(first);
    expect(registeredProCompartmentIds().filter((id) => id === 'statement')).toHaveLength(1);
  });

  describe('G2: an extension-owned compartment needs no host change', () => {
    it('is mountable and independently switchable purely by traversal', () => {
      // The point of `Record<string, Extension[]>`: an id the host has never
      // seen still gets a first-class, separately reconfigurable slot, with no
      // new member on the payload type.
      const hostPayload: ProCompartmentPayload = {
        statement: [slot('statement')],
        // No compile-time member exists for this key.
        'sql-folding': [slot('folding')],
      };
      view = makeView(hostPayload);
      expect(view.state.facet(tags)).toBe('statement|folding');

      const folding = getProCompartment('sql-folding');
      expect(folding).toBeDefined();
      expect(folding?.get(view.state)).toBeDefined();

      reconfigureProCompartments(view, { 'sql-folding': [slot('folding-off')] });
      expect(view.state.facet(tags)).toBe('statement|folding-off');

      // Traversal, not destructuring: walk the payload keys and read each slot
      // back off the state, exactly as a diagnostic tool would.
      const observed = Object.keys(hostPayload).map((id) =>
        getProCompartment(id)?.get(view!.state),
      );
      expect(observed).toHaveLength(2);
      expect(observed.every((content) => content !== undefined)).toBe(true);
    });

    it('routes an unmounted id into `extra` instead of throwing', () => {
      // CodeMirror throws RangeError when reconfiguring a Compartment that was
      // never part of the state. A late registration must degrade, not explode.
      view = makeView({ extra: [slot('extra-1')] });
      const warn = vi.spyOn(console, 'warn').mockImplementation(() => {});

      expect(() =>
        reconfigureProCompartments(view!, { extra: [slot('extra-2')], ghost: [slot('ghost')] }),
      ).not.toThrow();

      expect(warn).not.toHaveBeenCalled();
      expect(view.state.facet(tags)).toBe('extra-2|ghost');
      warn.mockRestore();
    });

    it('merges overflow into a mounted `extra` the payload never names', () => {
      // `extra` is mounted in this view, but this payload does not mention it —
      // so the merge has to start from an empty base. Spreading the absent key
      // would throw `undefined is not iterable` and lose the whole batch. The
      // editor's own payload always carries `extra`, but this function is
      // exported and its contract is stated in terms of the *view*, not in
      // terms of the caller's payload shape.
      view = makeView({ extra: [slot('mounted-base')] });
      const warn = vi.spyOn(console, 'warn').mockImplementation(() => {});

      expect(() => reconfigureProCompartments(view!, { ghost: [slot('ghost')] })).not.toThrow();

      expect(warn).not.toHaveBeenCalled();
      // `extra` is reconfigured wholesale, so the late id is all that is left.
      expect(view!.state.facet(tags)).toBe('ghost');
      warn.mockRestore();
    });

    it('warns and skips when even `extra` is unmounted, but still applies the rest', () => {
      view = makeView({ statement: [slot('statement-1')] });
      const warn = vi.spyOn(console, 'warn').mockImplementation(() => {});

      reconfigureProCompartments(view, { statement: [slot('statement-2')], ghost: [slot('g')] });

      expect(warn).toHaveBeenCalledOnce();
      expect(String(warn.mock.calls[0][0])).toContain('ghost');
      // A partial skip must not swallow the slots that were applicable —
      // otherwise one unregistered id would freeze the whole editor.
      expect(view.state.facet(tags)).toBe('statement-2');
      warn.mockRestore();
    });
  });

  describe('atomicity', () => {
    const before: ProCompartmentPayload = {
      a: [slot('a1')],
      b: [slot('b1')],
      c: [slot('c1')],
    };
    const after: ProCompartmentPayload = {
      a: [slot('a2')],
      b: [slot('b2')],
      c: [slot('c2')],
    };

    it('applies every slot in a single transaction', () => {
      view = makeView(before);
      const probe = installProbe(view);
      probe.reset();

      reconfigureProCompartments(view, after);

      // One update, and it already shows the fully swapped configuration —
      // no frame in which `a` is new while `c` is still old.
      expect(probe.seen).toEqual(['a2|b2|c2']);
    });

    it('control: the previous one-dispatch-per-compartment shape leaked partial frames', () => {
      // Without this the assertion above is vacuous — it would also pass if an
      // observer simply never saw the intermediate states.
      view = makeView(before);
      const probe = installProbe(view);
      probe.reset();

      // Six separate `view.dispatch` calls — the shape this track replaced.
      for (const id of ['a', 'b', 'c'] as const) {
        view.dispatch({
          effects: getProCompartment(id)!.reconfigure(after[id]),
          annotations: Transaction.addToHistory.of(false),
        });
      }

      // Three dispatches in a row: an observer is handed two half-applied
      // configurations first. This is the behaviour G2 removed.
      expect(probe.seen).toEqual(['a2|b1|c1', 'a2|b2|c1', 'a2|b2|c2']);
      expect(probe.seen).toHaveLength(3);
    });

    it('marks the batch as not part of the undo history', () => {
      view = makeView(before);
      const annotations: Array<boolean | undefined> = [];
      view.dispatch({
        effects: StateEffect.appendConfig.of(
          EditorView.updateListener.of((update) => {
            annotations.push(update.transactions[0].annotation(Transaction.addToHistory));
          }),
        ),
      });
      annotations.length = 0;

      reconfigureProCompartments(view, after);

      expect(annotations).toEqual([false]);
    });

    it('dispatches nothing when the payload has no applicable slot', () => {
      view = makeView(before);
      const probe = installProbe(view);
      probe.reset();

      reconfigureProCompartments(view, {});

      expect(probe.seen).toEqual([]);
      expect(view.state.facet(tags)).toBe('a1|b1|c1');
    });
  });

  describe('multi-instance isolation', () => {
    it('reconfigures one view without touching another', () => {
      // `compartments` is a module-level singleton shared by every editor
      // instance. CodeMirror stores the compartment → content association per
      // *state*, so this holds; it is pinned here because the failure mode
      // (two query tabs fighting over one registry) is invisible in a
      // single-instance test.
      const first = makeView({ shared: [slot('first')] });
      const second = makeView({ shared: [slot('second')] });

      reconfigureProCompartments(first, { shared: [slot('first-updated')] });
      expect(first.state.facet(tags)).toBe('first-updated');
      expect(second.state.facet(tags)).toBe('second');

      reconfigureProCompartments(second, { shared: [slot('second-updated')] });
      expect(first.state.facet(tags)).toBe('first-updated');
      expect(second.state.facet(tags)).toBe('second-updated');

      first.destroy();
      second.destroy();
    });
  });
});
