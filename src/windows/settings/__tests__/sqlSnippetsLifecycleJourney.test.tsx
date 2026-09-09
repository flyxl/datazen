/**
 * Continuous End-to-End User Journey for SQL Snippets.
 *
 * Adheres strictly to `docs/development/interaction-and-testing-principles.md` (Principle 3):
 * Continuous State Machine & Full Lifecycle Testing (no single static slices):
 *
 * 1. Discoverability & Initial State (builtins visible, custom empty state)
 * 2. Validation Gate (format regex, duplicate prefix against builtins, empty template)
 * 3. Successful Custom Snippet Creation (selu)
 * 4. Built-in Template Duplication & Customization (selc -> selc_copy)
 * 5. In-flight Edit & Self-Collision Immunity (selu edited without colliding with itself)
 * 6. Deletion Safety Gate (cancel retains snippet -> confirm removes snippet)
 * 7. Export/Import Serialization Round-Trip (safe whitelist JSON preservation)
 * 8. Live CodeMirror Autocomplete Progression (keystrokes, soft boost, hard exclusion, tabstop forward/backward)
 * 9. Real-Time Hotplug Removal (deletion immediately disables completion)
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { render, screen, fireEvent, waitFor, cleanup, within } from '@testing-library/react';
import { EditorState, EditorSelection } from '@codemirror/state';
import {
  CompletionContext,
  snippet,
  nextSnippetField,
  prevSnippetField,
  hasNextSnippetField,
  hasPrevSnippetField,
} from '@codemirror/autocomplete';

import { SqlSnippetsCard } from '../SqlSnippetsCard';
import { useSettingsStore } from '../../../stores/settingsStore';
import { BUILTIN_SQL_SNIPPETS } from '../../../components/sql-editor/snippets/builtinSnippets';
import { createSnippetCompletionSource } from '../../../components/sql-editor/snippets/snippetCompletionSource';
import { exportEditorSettings, importEditorSettings } from '../../../lib/settingsExport';
import type { AppSettings, SqlSnippetItem } from '../../../types';

vi.mock('../../../hooks/useI18n', () => {
  const t = (key: string, params?: Record<string, string | number>) => {
    if (params) {
      let str = key;
      for (const [k, v] of Object.entries(params)) {
        str = str.replace(`{${k}}`, String(v));
      }
      return str;
    }
    return key;
  };
  return {
    useI18n: () => ({ t }),
  };
});

vi.mock('../../../commands/settings', () => ({
  settingsCommands: {
    saveSettings: vi.fn().mockResolvedValue(undefined),
    getSettings: vi.fn().mockResolvedValue({}),
  },
}));

let clipboardBuffer = '';

function setupClipboardMock() {
  clipboardBuffer = '';
  Object.defineProperty(navigator, 'clipboard', {
    value: {
      writeText: vi.fn().mockImplementation((text: string) => {
        clipboardBuffer = text;
        return Promise.resolve();
      }),
      readText: vi.fn().mockImplementation(() => Promise.resolve(clipboardBuffer)),
    },
    configurable: true,
  });
}

function createEditorHarness(doc = '') {
  let state = EditorState.create({ doc });
  const view = {
    get state() {
      return state;
    },
    dispatch: (tr: { state: EditorState } | any) => {
      state = tr.state ?? state.update(tr).state;
    },
  };
  return {
    view,
    get doc() {
      return state.doc.toString();
    },
    get selection() {
      return state.selection.main;
    },
    get state() {
      return state;
    },
    apply(command: (target: any) => boolean) {
      return command({ state, dispatch: view.dispatch });
    },
  };
}

/** Connected wrapper that synchronizes with useSettingsStore */
function ConnectedSnippetsCard() {
  const settings = useSettingsStore((s) => s.settings);
  const updateSettings = useSettingsStore((s) => s.updateSettings);

  return (
    <SqlSnippetsCard
      settings={settings}
      onUpdateSnippets={(snippets) => updateSettings({ sqlSnippets: snippets })}
    />
  );
}

describe('SQL Snippets Full Lifecycle & Autocomplete Continuous Journey', () => {
  beforeEach(() => {
    setupClipboardMock();
    useSettingsStore.setState({
      settings: {
        ...useSettingsStore.getState().settings,
        sqlSnippets: [],
      },
    });
  });

  afterEach(() => {
    cleanup();
    vi.clearAllMocks();
  });

  it('navigates from empty state -> validation errors -> create -> duplicate -> edit -> safety delete -> import/export -> editor autocomplete & tabstops', async () => {
    // -------------------------------------------------------------------------
    // Phase 1: Discoverability & Initial State
    // -------------------------------------------------------------------------
    render(<ConnectedSnippetsCard />);

    // Builtins are visible with their prefixes and 'query.snippets.builtin' badges
    expect(screen.getByText('sel*')).toBeInTheDocument();
    expect(screen.getByText('selc')).toBeInTheDocument();
    expect(screen.getByText('ins')).toBeInTheDocument();

    // Custom snippets empty state message is shown
    expect(screen.getByText('query.snippets.emptyCustom')).toBeInTheDocument();

    // -------------------------------------------------------------------------
    // Phase 2: Validation Gate on Creation (Character format, Duplicate, Empty)
    // -------------------------------------------------------------------------
    fireEvent.click(screen.getByRole('button', { name: 'query.snippets.add' }));

    const prefixInput = await screen.findByPlaceholderText('query.snippets.prefixPlaceholder');
    const descInput = screen.getByPlaceholderText('query.snippets.descriptionPlaceholder');
    const templateInput = screen.getByPlaceholderText('query.snippets.templatePlaceholder');
    const saveBtn = screen.getByRole('button', { name: 'common.save' });

    // 2a: Invalid prefix characters (starts with digit, contains hyphens)
    fireEvent.change(prefixInput, { target: { value: '99-invalid-prefix' } });
    fireEvent.click(saveBtn);
    expect(screen.getByText('query.snippets.prefixInvalid')).toBeInTheDocument();
    expect(useSettingsStore.getState().settings.sqlSnippets).toHaveLength(0);

    // 2b: Duplicate prefix collision with built-in prefix (e.g. 'sel*')
    fireEvent.change(prefixInput, { target: { value: 'sel*' } });
    fireEvent.click(saveBtn);
    expect(screen.getByText('query.snippets.prefixDuplicate')).toBeInTheDocument();

    // 2c: Empty template validation
    fireEvent.change(prefixInput, { target: { value: 'selu' } });
    fireEvent.change(templateInput, { target: { value: '   ' } });
    fireEvent.click(saveBtn);
    expect(screen.getByText('query.snippets.templateRequired')).toBeInTheDocument();

    // -------------------------------------------------------------------------
    // Phase 3: Successful Custom Snippet Creation (selu)
    // -------------------------------------------------------------------------
    fireEvent.change(descInput, { target: { value: 'Select active users' } });
    fireEvent.change(templateInput, {
      target: { value: 'SELECT id, name FROM ${1:users} WHERE ${2:status} = 1;${3}' },
    });
    fireEvent.click(saveBtn);

    // Modal closes and custom snippet appears in UI
    await waitFor(() => {
      expect(
        screen.queryByPlaceholderText('query.snippets.prefixPlaceholder'),
      ).not.toBeInTheDocument();
    });
    expect(screen.getByText('selu')).toBeInTheDocument();
    expect(screen.getByText('Select active users')).toBeInTheDocument();
    expect(screen.queryByText('query.snippets.emptyCustom')).not.toBeInTheDocument();

    // Store state updated
    const savedSnippets = useSettingsStore.getState().settings.sqlSnippets ?? [];
    expect(savedSnippets).toHaveLength(1);
    expect(savedSnippets[0].prefix).toBe('selu');

    // -------------------------------------------------------------------------
    // Phase 4: Duplicate Built-in Snippet (selc -> selc_copy)
    // -------------------------------------------------------------------------
    const duplicateBtns = screen.getAllByRole('button', { name: 'query.snippets.duplicate' });
    // Click duplicate on selc (second builtin button)
    fireEvent.click(duplicateBtns[1]);

    const dupPrefixInput = await screen.findByPlaceholderText('query.snippets.prefixPlaceholder');
    expect(dupPrefixInput).toHaveValue('selc_copy');

    const dupDescInput = screen.getByPlaceholderText('query.snippets.descriptionPlaceholder');
    fireEvent.change(dupDescInput, { target: { value: 'Custom column projection' } });
    fireEvent.click(screen.getByRole('button', { name: 'common.save' }));

    await waitFor(() => {
      expect(
        screen.queryByPlaceholderText('query.snippets.prefixPlaceholder'),
      ).not.toBeInTheDocument();
    });
    expect(screen.getByText('selc_copy')).toBeInTheDocument();
    expect(useSettingsStore.getState().settings.sqlSnippets).toHaveLength(2);

    // -------------------------------------------------------------------------
    // Phase 5: In-flight Edit & Self-Collision Immunity
    // -------------------------------------------------------------------------
    // Edit 'selu': clicking edit should pre-fill fields and NOT collide with 'selu' itself
    const editBtns = screen.getAllByTitle('query.snippets.edit');
    fireEvent.click(editBtns[0]); // First custom snippet (selu)

    const editPrefixInput = await screen.findByPlaceholderText('query.snippets.prefixPlaceholder');
    expect(editPrefixInput).toHaveValue('selu');

    // Trying to save with same prefix must succeed without duplicate error
    const editTemplateInput = screen.getByPlaceholderText('query.snippets.templatePlaceholder');
    fireEvent.change(editTemplateInput, {
      target: { value: 'SELECT id, name, email FROM ${1:users} WHERE ${2:status} = 1;${3}' },
    });
    fireEvent.click(screen.getByRole('button', { name: 'common.save' }));

    await waitFor(() => {
      expect(
        screen.queryByPlaceholderText('query.snippets.prefixPlaceholder'),
      ).not.toBeInTheDocument();
    });
    const currentSnippets = useSettingsStore.getState().settings.sqlSnippets ?? [];
    const seluItem = currentSnippets.find((s) => s.prefix === 'selu');
    expect(seluItem?.template).toContain('email');

    // -------------------------------------------------------------------------
    // Phase 6: Deletion Safety Gate (Cancel then Confirm)
    // -------------------------------------------------------------------------
    const selcCopyItem = currentSnippets.find((s) => s.prefix === 'selc_copy')!;
    const deleteBtn = screen.getByTestId(`delete-snippet-${selcCopyItem.id}`);
    fireEvent.click(deleteBtn);

    // Confirm dialog appears
    const cancelConfirmBtn = await screen.findByRole('button', { name: 'common.cancel' });
    fireEvent.click(cancelConfirmBtn);

    // Snippet remains in store
    expect(useSettingsStore.getState().settings.sqlSnippets).toHaveLength(2);
    expect(screen.getByText('selc_copy')).toBeInTheDocument();

    // Click delete again and confirm
    fireEvent.click(screen.getByTestId(`delete-snippet-${selcCopyItem.id}`));
    const confirmDeleteBtn = await screen.findByRole('button', { name: 'common.confirm' });
    fireEvent.click(confirmDeleteBtn);

    await waitFor(() => {
      expect(screen.queryByText('selc_copy')).not.toBeInTheDocument();
    });
    expect(useSettingsStore.getState().settings.sqlSnippets).toHaveLength(1);

    // -------------------------------------------------------------------------
    // Phase 7: Settings Export & Import Round-trip
    // -------------------------------------------------------------------------
    // Export settings to clipboard
    fireEvent.click(screen.getByRole('button', { name: 'query.exportSettings' }));
    await waitFor(() => {
      expect(navigator.clipboard.writeText).toHaveBeenCalled();
    });

    // Inspect exported JSON
    const exportedPayload = JSON.parse(clipboardBuffer);
    expect(exportedPayload.version).toBe(1);
    expect(exportedPayload.sqlSnippets).toEqual(
      expect.arrayContaining([expect.objectContaining({ prefix: 'selu' })]),
    );

    // Simulate importing an updated snippet set from clipboard
    const importedSnippet: SqlSnippetItem = {
      id: 'import-99',
      prefix: 'selimported',
      descriptionKey: 'Imported users query',
      template: 'SELECT * FROM imported_table WHERE ${1:col} = ${2:val};${3}',
    };
    clipboardBuffer = JSON.stringify({
      version: 1,
      sqlSnippets: [importedSnippet],
    });

    fireEvent.click(screen.getByRole('button', { name: 'query.importSettings' }));
    await waitFor(() => {
      expect(screen.getByText('selimported')).toBeInTheDocument();
    });
    expect(useSettingsStore.getState().settings.sqlSnippets).toHaveLength(1);
    expect(useSettingsStore.getState().settings.sqlSnippets![0].prefix).toBe('selimported');

    // -------------------------------------------------------------------------
    // Phase 8: Live CodeMirror Autocomplete Progression & Tabstop Expansion
    // -------------------------------------------------------------------------
    const activeSnippets = [
      ...BUILTIN_SQL_SNIPPETS,
      ...(useSettingsStore.getState().settings.sqlSnippets ?? []),
    ];
    const completionSource = createSnippetCompletionSource({ snippets: activeSnippets });

    // Step 8a: Statement start - typing 'sel' proposes builtins and custom snippet
    const selState = EditorState.create({ doc: 'sel' });
    const selResult = completionSource(new CompletionContext(selState, 3, false));
    expect(selResult).not.toBeNull();
    if (selResult && 'options' in selResult) {
      const labels = selResult.options.map((o) => o.label);
      expect(labels).toContain('sel*');
      expect(labels).toContain('selc');
      expect(labels).toContain('selimported');
      expect(selResult.options[0].boost).toBeGreaterThan(0);
    }

    // Step 8b: Precise match for 'selimported'
    const matchState = EditorState.create({ doc: 'selimported' });
    const matchResult = completionSource(
      new CompletionContext(matchState, 'selimported'.length, false),
    );
    expect(matchResult).not.toBeNull();
    if (matchResult && 'options' in matchResult) {
      expect(matchResult.from).toBe(0);
      expect(matchResult.options.map((o) => o.label)).toContain('selimported');
    }

    // Step 8c: CodeMirror snippet expansion and tabstop traversal
    const harness = createEditorHarness('selimported');
    snippet(importedSnippet.template)(harness.view as any, null, 0, 'selimported'.length);

    expect(harness.doc).toBe('SELECT * FROM imported_table WHERE col = val;');

    // First tabstop ${1:col} is highlighted
    expect(harness.state.sliceDoc(harness.selection.from, harness.selection.to)).toBe('col');
    expect(hasNextSnippetField(harness.state)).toBe(true);
    expect(hasPrevSnippetField(harness.state)).toBe(false);

    // Type replacement 'user_uuid' directly over selected placeholder
    harness.view.dispatch(harness.state.replaceSelection('user_uuid'));
    expect(harness.doc).toBe('SELECT * FROM imported_table WHERE user_uuid = val;');

    // Tab forward to second tabstop ${2:val}
    expect(harness.apply(nextSnippetField)).toBe(true);
    expect(harness.state.sliceDoc(harness.selection.from, harness.selection.to)).toBe('val');
    expect(hasPrevSnippetField(harness.state)).toBe(true);
    expect(hasNextSnippetField(harness.state)).toBe(true);

    // Tab back to ${1} with Shift-Tab (test backward state transition)
    expect(harness.apply(prevSnippetField)).toBe(true);
    expect(harness.state.sliceDoc(harness.selection.from, harness.selection.to)).toBe('user_uuid');

    // Tab forward to ${2}
    expect(harness.apply(nextSnippetField)).toBe(true);
    expect(harness.state.sliceDoc(harness.selection.from, harness.selection.to)).toBe('val');

    // Tab forward to terminal ${3} -> session deactivates
    expect(harness.apply(nextSnippetField)).toBe(true);
    expect(hasNextSnippetField(harness.state)).toBe(false);
    expect(hasPrevSnippetField(harness.state)).toBe(false);

    // Step 8d: Soft boost in table context (ranked down, not hard-killed)
    const tableState = EditorState.create({ doc: 'SELECT * FROM sel' });
    const tableResult = completionSource(
      new CompletionContext(tableState, tableState.doc.length, false),
    );
    expect(tableResult).not.toBeNull();
    if (tableResult && 'options' in tableResult) {
      expect(tableResult.options.map((o) => o.label)).toContain('selimported');
      expect(tableResult.options[0].boost).toBeLessThan(0);
    }

    // Step 8e: Hard exclusion on dot-qualified identifier
    const dotState = EditorState.create({ doc: 'SELECT tbl.sel' });
    const dotResult = completionSource(new CompletionContext(dotState, dotState.doc.length, false));
    expect(dotResult).toBeNull();

    // -------------------------------------------------------------------------
    // Phase 9: Real-Time Hotplug Removal
    // -------------------------------------------------------------------------
    // Delete the snippet from store
    useSettingsStore.setState({
      settings: {
        ...useSettingsStore.getState().settings,
        sqlSnippets: [],
      },
    });

    const refreshedSnippets = [
      ...BUILTIN_SQL_SNIPPETS,
      ...(useSettingsStore.getState().settings.sqlSnippets ?? []),
    ];
    const refreshedSource = createSnippetCompletionSource({ snippets: refreshedSnippets });

    const finalState = EditorState.create({ doc: 'selimported' });
    const finalResult = refreshedSource(
      new CompletionContext(finalState, finalState.doc.length, false),
    );
    if (finalResult && 'options' in finalResult) {
      expect(finalResult.options.map((o) => o.label)).not.toContain('selimported');
    }
  });
});
