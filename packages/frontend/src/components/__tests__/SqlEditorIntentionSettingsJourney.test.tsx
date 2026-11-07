import { afterEach, describe, expect, it, vi } from 'vitest';
import { act, cleanup, render } from '@testing-library/react';
import { Facet } from '@codemirror/state';
import { EditorView } from '@codemirror/view';
import { undo } from '@codemirror/commands';
import { extensionRegistry, sqlEditorEnhancedEP } from '@datazen/extension-points';
import { SqlEditor } from '../SqlEditor';
import { useSettingsStore } from '../../stores/settingsStore';
import type {
  IntentionCompartmentOptions,
  LinterCompartmentOptions,
} from '../sql-editor/editorExtensions';

const actionsEnabled = Facet.define<boolean, boolean>({ combine: (values) => values[0] ?? false });
const diagnosticsEnabled = Facet.define<boolean, boolean>({
  combine: (values) => values[0] ?? false,
});
const previousSettings = useSettingsStore.getState().settings;

afterEach(() => {
  cleanup();
  useSettingsStore.setState({ settings: previousSettings });
});

describe('Intention Actions settings journey', () => {
  it.each(['sql-editor-enhanced', 'sql-editor-pro'])(
    'reconfigures off/on/off live without losing typing, selection or other enhancements (%s)',
    (extensionId) => {
      useSettingsStore.setState({ settings: { ...previousSettings, driverSettings: {} } });
      // A small EP implementation observes the actual host compartment contract;
      // Pro owns the lightbulb renderer and its own DOM journey tests.
      const createLinterExtensions = vi.fn((opts: LinterCompartmentOptions) => [
        diagnosticsEnabled.of(true),
        actionsEnabled.of(opts.intentionActions === true),
      ]);
      const createIntentionExtensions = vi.fn((_opts: IntentionCompartmentOptions) => []);
      const unregister = extensionRegistry.register(sqlEditorEnhancedEP, {
        createLinterExtensions,
        createIntentionExtensions,
      });
      const { container, unmount } = render(<SqlEditor value="SELECT " onChange={vi.fn()} />);
      try {
        const dom = container.querySelector('.cm-editor');
        if (!(dom instanceof HTMLElement)) throw new Error('Missing editor');
        const view = EditorView.findFromDOM(dom);
        if (!view) throw new Error('Missing editor view');
        expect(view.state.facet(actionsEnabled)).toBe(false);
        expect(view.state.facet(diagnosticsEnabled)).toBe(true);
        const initialIntentionOpts = createIntentionExtensions.mock.calls.at(-1)?.[0];
        act(() => {
          view.dispatch({ changes: { from: 7, insert: '*' }, selection: { anchor: 8 } });
        });

        for (const enabled of [true, false]) {
          act(() => {
            useSettingsStore.setState((state) => ({
              settings: {
                ...state.settings,
                driverSettings: { [extensionId]: { intentionActions: enabled } },
              },
            }));
          });
          expect(EditorView.findFromDOM(dom)).toBe(view);
          expect(view.state.facet(actionsEnabled)).toBe(enabled);
          expect(view.state.facet(diagnosticsEnabled)).toBe(true);
          expect(view.state.doc.toString()).toBe('SELECT *');
          expect(view.state.selection.main.head).toBe(8);
          // Toggling only lightbulbs must not disturb INSERT hints or Alt+Enter.
          // The factory is allowed to re-run — since the generic settings path
          // every compartment is keyed on the whole bag — so assert the
          // behaviour it is given, not how often it was called.
          const latestIntentionOpts = createIntentionExtensions.mock.calls.at(-1)?.[0];
          expect(latestIntentionOpts?.insertValueHints).toBe(
            initialIntentionOpts?.insertValueHints,
          );
          expect(latestIntentionOpts?.databaseType).toBe(initialIntentionOpts?.databaseType);
        }
        expect(createLinterExtensions.mock.calls.map(([opts]) => opts.intentionActions)).toEqual([
          false,
          true,
          false,
        ]);
        act(() => {
          undo(view);
        });
        expect(view.state.doc.toString()).toBe('SELECT ');
      } finally {
        unmount();
        unregister();
      }
    },
  );
});
