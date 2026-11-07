/**
 * BUG-002 — the mount commit must not re-dispatch the payload it just installed.
 *
 * `SqlEditor`'s reconfigure effect deduplicates through `appliedPayloadRef`,
 * which starts as `null`. React runs effects in declaration order, so in the
 * very same commit the mount effect (declared first) builds the view from
 * `proPayload` and the reconfigure effect (declared second) immediately
 * re-reads that ref. Unless the mount effect seeds the ref, the guard
 * `appliedPayloadRef.current === proPayload` reads `null === proPayload`, and
 * the editor pays for a second, completely redundant full 8-slot batch on
 * every single mount.
 *
 * The probe is installed on `EditorView.prototype` rather than on an instance
 * because the transaction under test is dispatched *during* `render()` —
 * there is no view handle to patch until afterwards.
 *
 * This file also pins the behaviour the redundant transaction was *not*:
 * one settings write must still land as exactly one 8-slot batch
 * (ep-hooks-settings acceptance §四.5).
 */
import { afterAll, afterEach, beforeAll, describe, expect, it, vi } from 'vitest';
import { act, cleanup, render } from '@testing-library/react';
import { EditorView } from '@codemirror/view';
import { SqlEditor } from '../SqlEditor';
import { BASE_PRO_COMPARTMENT_IDS } from '../sql-editor/proCompartments';
import { useSettingsStore } from '../../stores/settingsStore';

const EXTENSION_ID = 'sql-editor-pro';
const SLOT_COUNT = BASE_PRO_COMPARTMENT_IDS.length;

const previousSettings = useSettingsStore.getState().settings;

/** State-effect count of every transaction dispatched since the last reset. */
let transactions: number[] = [];

const originalDispatch: EditorView['dispatch'] = EditorView.prototype.dispatch;

/** `TransactionSpec` is a union; read its `effects` without narrowing pain. */
function effectCount(input: unknown): number | null {
  if (typeof input !== 'object' || input === null || !('effects' in input)) return null;
  const effects = (input as { effects?: unknown }).effects;
  return Array.isArray(effects) ? effects.length : null;
}

/** Transactions that reconfigure more than one compartment — i.e. a Pro batch. */
const compartmentBatches = (): number[] => transactions.filter((count) => count > 1);

beforeAll(() => {
  EditorView.prototype.dispatch = function probedDispatch(
    this: EditorView,
    ...args: Parameters<EditorView['dispatch']>
  ) {
    const count = effectCount(args[0]);
    if (count !== null) transactions.push(count);
    originalDispatch.apply(this, args);
  } as EditorView['dispatch'];
});

afterAll(() => {
  EditorView.prototype.dispatch = originalDispatch;
});

afterEach(() => {
  cleanup();
  transactions = [];
  useSettingsStore.setState({ settings: previousSettings });
});

function setDriverSettings(driverSettings: Record<string, unknown>) {
  act(() => {
    useSettingsStore.setState((state) => ({
      settings: { ...state.settings, driverSettings },
    }));
  });
}

describe('mount installs each privileged slot exactly once (BUG-002)', () => {
  it('dispatches no compartment batch at mount, then exactly one on a settings write', () => {
    useSettingsStore.setState({ settings: { ...previousSettings, driverSettings: {} } });

    const { container, unmount } = render(<SqlEditor value="SELECT 1" onChange={vi.fn()} />);
    try {
      const dom = container.querySelector('.cm-editor');
      if (!(dom instanceof HTMLElement)) throw new Error('Missing editor');
      expect(EditorView.findFromDOM(dom)).toBeTruthy();

      // The mount commit already carried all 8 slots into the state. Anything
      // reconfiguring more than one compartment here is the redundant batch.
      expect(compartmentBatches()).toEqual([]);
      // Spelled out against the real slot count too, so the guard reads as the
      // invariant it is rather than as an opaque "more than one".
      expect(transactions.filter((count) => count === SLOT_COUNT)).toEqual([]);

      // And the accepted behaviour the fix must not cost us: one settings
      // write, one atomic 8-slot transaction.
      transactions = [];
      setDriverSettings({ [EXTENSION_ID]: { tableHover: false } });

      expect(compartmentBatches()).toEqual([SLOT_COUNT]);
      expect(transactions.filter((count) => count === SLOT_COUNT)).toHaveLength(1);
    } finally {
      unmount();
    }
  });
});
