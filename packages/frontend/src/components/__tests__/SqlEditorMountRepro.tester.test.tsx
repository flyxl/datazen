/**
 * [tester] Independent reproduction probe for BUG-002 / BUG-003.
 *
 * This file is deliberately *not* derived from the Coder's
 * `SqlEditorProMountTransaction.test.tsx`. It exists so the original defect can
 * be reproduced from first principles:
 *
 *  1. `EditorView.prototype.dispatch` is patched, so every transaction the editor
 *     dispatches is recorded — including the ones fired *during* `render()`,
 *     before any view handle exists.
 *  2. Each record keeps the raw `effects.length` **and** the `SqlEditor.tsx`
 *     frame that issued it, so "the 8-slot batch" and "the zero-effect
 *     theme / sqlCompartment dispatches" can be told apart by origin instead of
 *     by an opaque number.
 *  3. It runs unchanged against both the pre-fix and post-fix business code, so
 *     a red/green flip is a genuine behavioural difference, not a test that was
 *     written to the fixed shape.
 *
 * Probe expectations after the fix (asserted below):
 *   mount  -> no transaction reconfiguring all 8 Pro slots
 *   write  -> exactly one transaction reconfiguring all 8 Pro slots
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

interface DispatchRecord {
  /** `effects.length`, or `null` when the spec carried no `effects` array. */
  len: number | null;
  /** The `SqlEditor.tsx` frames that issued this dispatch, outermost first. */
  origin: string[];
}

let records: DispatchRecord[] = [];
let captureStacks = false;

const originalDispatch: EditorView['dispatch'] = EditorView.prototype.dispatch;

function effectCount(spec: unknown): number | null {
  if (typeof spec !== 'object' || spec === null || !('effects' in spec)) return null;
  const effects = (spec as { effects?: unknown }).effects;
  return Array.isArray(effects) ? effects.length : null;
}

/** Only frames from the component under test — the noise floor we care about. */
function sqlEditorFrames(): string[] {
  if (!captureStacks) return [];
  const stack = new Error().stack ?? '';
  return stack
    .split('\n')
    .map((line) => line.trim())
    .filter((line) => line.includes('SqlEditor.tsx:'))
    .map((line) => line.replace(/^at\s+/, '').split(' ')[0]);
}

beforeAll(() => {
  EditorView.prototype.dispatch = function probedDispatch(
    this: EditorView,
    ...args: Parameters<EditorView['dispatch']>
  ) {
    if (captureStacks) {
      records.push({ len: effectCount(args[0]), origin: sqlEditorFrames() });
    }
    originalDispatch.apply(this, args);
  } as EditorView['dispatch'];
});

afterAll(() => {
  EditorView.prototype.dispatch = originalDispatch;
});

afterEach(() => {
  captureStacks = false;
  records = [];
  cleanup();
  useSettingsStore.setState({ settings: previousSettings });
});

function setDriverSettings(driverSettings: Record<string, unknown>) {
  act(() => {
    useSettingsStore.setState((state) => ({
      settings: { ...state.settings, driverSettings },
    }));
  });
}

describe('[tester] BUG-002 mount transaction probe', () => {
  it('mount issues no 8-slot Pro batch; one settings write issues exactly one', () => {
    useSettingsStore.setState({ settings: { ...previousSettings, driverSettings: {} } });

    captureStacks = true;
    const { container, unmount } = render(<SqlEditor value="SELECT 1" onChange={vi.fn()} />);
    try {
      const dom = container.querySelector('.cm-editor');
      if (!(dom instanceof HTMLElement)) throw new Error('Missing editor');
      expect(EditorView.findFromDOM(dom)).toBeTruthy();

      // eslint-disable-next-line no-console
      console.log('[tester] MOUNT dispatches:', JSON.stringify(records));

      const mountRecords = records.slice();
      records = [];
      setDriverSettings({ [EXTENSION_ID]: { tableHover: false } });

      // eslint-disable-next-line no-console
      console.log('[tester] WRITE dispatches:', JSON.stringify(records));
      const writeRecords = records.slice();

      // A full 8-slot Pro batch reconfigures every Pro compartment at once.
      const mountSlotBatches = mountRecords.filter((r) => r.len === SLOT_COUNT);
      const writeSlotBatches = writeRecords.filter((r) => r.len === SLOT_COUNT);

      // The two single-slot listeners must survive untouched.
      const mountSingleSlot = mountRecords.filter((r) => r.len === null);

      // eslint-disable-next-line no-console
      console.log(
        '[tester] SUMMARY',
        JSON.stringify({
          mountHistogram: mountRecords.map((r) => r.len),
          writeHistogram: writeRecords.map((r) => r.len),
        }),
      );

      expect(mountSlotBatches).toEqual([]);
      expect(writeSlotBatches).toHaveLength(1);
      expect(mountSingleSlot).toHaveLength(2);
    } finally {
      captureStacks = false;
      unmount();
    }
  });

  it('keeps the single-slot theme / sqlCompartment dispatches (no over-fix)', () => {
    // `themeCompartment.reconfigure(...)` and `sqlCompartment.reconfigure(...)`
    // each return a *single* `StateEffect`, so their transactions carry a
    // non-array `effects` — they contribute zero entries to any compartment
    // batch. A fix that silenced the whole reconfigure path would silently drop
    // these two, so they are pinned here.
    useSettingsStore.setState({
      settings: { ...previousSettings, driverSettings: {}, sqlSyntaxTheme: 'default' },
    });

    captureStacks = true;
    const { unmount } = render(<SqlEditor value="SELECT 1" onChange={vi.fn()} />);
    try {
      const singleSlot = records.filter((r) => r.len === null);
      // eslint-disable-next-line no-console
      console.log(
        '[tester] SINGLE-SLOT dispatches:',
        JSON.stringify(singleSlot.map((r) => r.origin)),
      );
      // eslint-disable-next-line no-console
      console.log('[tester] ALL-MOUNT len histogram:', JSON.stringify(records.map((r) => r.len)));

      // Exactly two, both issued from the component under test.
      expect(singleSlot).toHaveLength(2);
      for (const record of singleSlot) {
        expect(record.origin.length).toBeGreaterThan(0);
        expect(record.origin[0]).toContain('SqlEditor.tsx:');
      }
      // ...and still no array-shaped Pro batch among them.
      expect(records.filter((r) => r.len === SLOT_COUNT)).toEqual([]);
    } finally {
      captureStacks = false;
      unmount();
    }
  });

  it('a second mount of a fresh instance is also batch-free, and writes still land', () => {
    // `appliedPayloadRef` is per-component-instance, so a remount starts from
    // `null` again. The seed has to happen on *every* mount, not just the first
    // one a given component instance sees.
    useSettingsStore.setState({ settings: { ...previousSettings, driverSettings: {} } });

    const first = render(<SqlEditor value="SELECT 1" onChange={vi.fn()} />);
    first.unmount();

    captureStacks = true;
    const second = render(<SqlEditor value="SELECT 1" onChange={vi.fn()} />);
    try {
      // eslint-disable-next-line no-console
      console.log('[tester] REMOUNT histogram:', JSON.stringify(records.map((r) => r.len)));
      expect(records.filter((r) => r.len === SLOT_COUNT)).toEqual([]);

      records = [];
      setDriverSettings({ [EXTENSION_ID]: { tableHover: false } });
      const writeBatches = records.filter((r) => r.len === SLOT_COUNT);
      // eslint-disable-next-line no-console
      console.log('[tester] REMOUNT write histogram:', JSON.stringify(records.map((r) => r.len)));
      expect(writeBatches).toHaveLength(1);
    } finally {
      captureStacks = false;
      second.unmount();
      cleanup();
    }
  });
});
