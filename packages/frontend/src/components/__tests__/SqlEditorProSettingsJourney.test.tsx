/**
 * G3 — privileged settings actually take effect.
 *
 * Before this track the editor read five hard-coded keys out of the
 * `sql-editor-pro` settings bag. Every other setting an extension declared in
 * `settingsContributions` was saved by the settings UI, rendered as a switch,
 * and then silently ignored by the editor. These tests pin the generic path
 * that fixes that, plus the two new escape-hatch hooks.
 *
 * `src/components/sql-editor/**` sits outside the vitest coverage gate, so
 * every assertion here is behavioural: each one toggles a setting and then
 * observes a real CodeMirror facet, so it fails if the wiring is removed even
 * though no line count changes.
 */
import { afterEach, describe, expect, it, vi } from 'vitest';
import { act, cleanup, render } from '@testing-library/react';
import { Facet } from '@codemirror/state';
import { EditorView } from '@codemirror/view';
import { extensionRegistry, sqlEditorEnhancedEP } from '@datazen/extension-points';
import { SqlEditor } from '../SqlEditor';
import { useSettingsStore } from '../../stores/settingsStore';
import type { PasteCompartmentOptions } from '../sql-editor/paste/createPasteExtensions';

const EXTENSION_ID = 'sql-editor-pro';
const previousSettings = useSettingsStore.getState().settings;

/** Pro-side "is the paste extension installed?" probe. */
const pasteInstalled = Facet.define<boolean, boolean>({ combine: (values) => values[0] ?? false });
/** Pro-side "what did the extension label itself?" probe. */
const extraInstalled = Facet.define<string, string>({ combine: (values) => values[0] ?? 'none' });

function setDriverSettings(driverSettings: Record<string, unknown>) {
  act(() => {
    useSettingsStore.setState((state) => ({
      settings: { ...state.settings, driverSettings },
    }));
  });
}

afterEach(() => {
  cleanup();
  useSettingsStore.setState({ settings: previousSettings });
});

describe('privileged settings are not inert (G3)', () => {
  it('applies a setting key the host has no hard-coded knowledge of', () => {
    useSettingsStore.setState({ settings: { ...previousSettings, driverSettings: {} } });

    // `pasteAsIn` exists only in the extension. The host never names it: it
    // forwards the bag and the extension reads its own key. Before G3 there
    // was no path for such a key to reach the editor at all.
    const createPasteExtensions = vi.fn((opts: PasteCompartmentOptions) => [
      pasteInstalled.of(opts?.proSettings?.pasteAsIn !== false),
    ]);
    const unregister = extensionRegistry.register(sqlEditorEnhancedEP, {
      createPasteExtensions,
    });

    const { container, unmount } = render(<SqlEditor value="SELECT 1" onChange={vi.fn()} />);
    try {
      const dom = container.querySelector('.cm-editor');
      if (!(dom instanceof HTMLElement)) throw new Error('Missing editor');
      const view = EditorView.findFromDOM(dom);
      if (!view) throw new Error('Missing editor view');

      expect(view.state.facet(pasteInstalled)).toBe(true);
      expect(createPasteExtensions.mock.calls.at(-1)?.[0]?.proSettings).toEqual({});

      setDriverSettings({ [EXTENSION_ID]: { pasteAsIn: false } });

      // The whole point: the view survives, the document survives, and the
      // compartment really switched.
      expect(EditorView.findFromDOM(dom)).toBe(view);
      expect(view.state.doc.toString()).toBe('SELECT 1');
      expect(view.state.facet(pasteInstalled)).toBe(false);
      expect(createPasteExtensions.mock.calls.at(-1)?.[0]?.proSettings).toEqual({
        pasteAsIn: false,
      });

      setDriverSettings({ [EXTENSION_ID]: { pasteAsIn: true } });
      expect(view.state.facet(pasteInstalled)).toBe(true);
    } finally {
      unmount();
      unregister();
    }
  });

  it('re-runs every compartment on one settings write — not just the one that was named', () => {
    useSettingsStore.setState({ settings: { ...previousSettings, driverSettings: {} } });

    const probes = {
      paste: vi.fn((opts: PasteCompartmentOptions) => [
        pasteInstalled.of(opts?.proSettings?.pasteAsIn !== false),
      ]),
      // A second slot whose factory the host also invokes; proves the batch is
      // rebuilt as a unit rather than key by key.
      hover: vi.fn((opts: Record<string, unknown>) => [
        extraInstalled.of(String(opts['proSettings'] ? 'hover-rebuilt' : 'hover-static')),
      ]),
    };
    const unregister = extensionRegistry.register(sqlEditorEnhancedEP, {
      createPasteExtensions: probes.paste,
      createHoverExtensions: probes.hover,
    });

    const { container, unmount } = render(<SqlEditor value="SELECT 1" onChange={vi.fn()} />);
    try {
      const dom = container.querySelector('.cm-editor');
      if (!(dom instanceof HTMLElement)) throw new Error('Missing editor');
      const view = EditorView.findFromDOM(dom);
      if (!view) throw new Error('Missing editor view');
      expect(view.state.facet(extraInstalled)).toBe('hover-rebuilt');

      const before = {
        paste: probes.paste.mock.calls.length,
        hover: probes.hover.mock.calls.length,
      };
      setDriverSettings({ [EXTENSION_ID]: { pasteAsIn: false } });

      expect(probes.paste.mock.calls.length).toBe(before.paste + 1);
      expect(probes.hover.mock.calls.length).toBe(before.hover + 1);
    } finally {
      unmount();
      unregister();
    }
  });

  it('sends one settings write through as a single reconfiguration', () => {
    // Atomicity in the production path, not just in the registry helper: a
    // single write must not produce a cascade of independent transactions.
    useSettingsStore.setState({ settings: { ...previousSettings, driverSettings: {} } });
    const unregister = extensionRegistry.register(sqlEditorEnhancedEP, {
      createPasteExtensions: () => [pasteInstalled.of(true)],
      createHoverExtensions: () => [extraInstalled.of('hover')],
    });

    const { container, unmount } = render(<SqlEditor value="SELECT 1" onChange={vi.fn()} />);
    try {
      const dom = container.querySelector('.cm-editor');
      if (!(dom instanceof HTMLElement)) throw new Error('Missing editor');
      const view = EditorView.findFromDOM(dom);
      if (!view) throw new Error('Missing editor view');

      const original = view.dispatch.bind(view);
      let dispatches = 0;
      view.dispatch = ((...args: Parameters<EditorView['dispatch']>) => {
        dispatches += 1;
        return original(...args);
      }) as EditorView['dispatch'];

      setDriverSettings({ [EXTENSION_ID]: { pasteAsIn: false, tableHover: false } });

      expect(dispatches).toBe(1);
    } finally {
      unmount();
      unregister();
    }
  });
});

describe('generic EP hooks (contract 1.1.0)', () => {
  it('installs createExtraExtensions output into the generic `extra` slot', () => {
    useSettingsStore.setState({ settings: { ...previousSettings, driverSettings: {} } });
    const createExtraExtensions = vi.fn(() => [extraInstalled.of('folding-on')]);
    const unregister = extensionRegistry.register(sqlEditorEnhancedEP, { createExtraExtensions });

    const { container, unmount } = render(<SqlEditor value="SELECT 1" onChange={vi.fn()} />);
    try {
      const dom = container.querySelector('.cm-editor');
      if (!(dom instanceof HTMLElement)) throw new Error('Missing editor');
      const view = EditorView.findFromDOM(dom);
      if (!view) throw new Error('Missing editor view');

      expect(view.state.facet(extraInstalled)).toBe('folding-on');
      expect(createExtraExtensions).toHaveBeenCalled();

      setDriverSettings({ [EXTENSION_ID]: { anythingNew: true } });
      expect(view.state.facet(extraInstalled)).toBe('folding-on');
    } finally {
      unmount();
      unregister();
    }
  });

  it('leaves an unimplemented optional hook inert (absent === empty)', () => {
    // The fallback contract: an EP that implements none of the new hooks must
    // still mount, and the host must not assume their presence.
    useSettingsStore.setState({ settings: { ...previousSettings, driverSettings: {} } });
    const unregister = extensionRegistry.register(sqlEditorEnhancedEP, {});
    const { container, unmount } = render(<SqlEditor value="SELECT 1" onChange={vi.fn()} />);
    try {
      const dom = container.querySelector('.cm-editor');
      if (!(dom instanceof HTMLElement)) throw new Error('Missing editor');
      const view = EditorView.findFromDOM(dom);
      if (!view) throw new Error('Missing editor view');
      expect(view.state.facet(extraInstalled)).toBe('none');
      expect(() => setDriverSettings({ [EXTENSION_ID]: { x: 1 } })).not.toThrow();
    } finally {
      unmount();
      unregister();
    }
  });

  it('survives an extension hook that throws, via the circuit breaker', () => {
    useSettingsStore.setState({ settings: { ...previousSettings, driverSettings: {} } });
    const unregister = extensionRegistry.register(sqlEditorEnhancedEP, {
      createExtraExtensions: () => {
        throw new Error('boom');
      },
    });
    const { container, unmount } = render(<SqlEditor value="SELECT 1" onChange={vi.fn()} />);
    try {
      const dom = container.querySelector('.cm-editor');
      if (!(dom instanceof HTMLElement)) throw new Error('Missing editor');
      const view = EditorView.findFromDOM(dom);
      if (!view) throw new Error('Missing editor view');
      // Degraded to the fallback, editor still alive.
      expect(view.state.facet(extraInstalled)).toBe('none');
    } finally {
      unmount();
      unregister();
    }
  });
});
