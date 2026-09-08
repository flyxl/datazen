/**
 * Editor compartment hot-plug journey test.
 *
 * Simulates a user typing SQL while Pro extensions are dynamically registered
 * and unregistered. Asserts compartment reconfiguration, decoration presence,
 * and preservation of document, selection, and undo/redo history.
 */
import { describe, it, expect, beforeEach, afterEach } from 'vitest';
import { EditorState, Transaction } from '@codemirror/state';
import { EditorView } from '@codemirror/view';
import { undo, redo } from '@codemirror/commands';
import { extensionRegistry, sqlEditorProEP } from '@datazen/extension-points';
import {
  compartments,
  createBaseEditorExtensions,
  createStatementExtensions,
  createCompletionExtensions,
  createIntentionExtensions,
  createHoverExtensions,
  createPasteExtensions,
  createLinterExtensions,
  reconfigureProCompartments,
} from '../editorExtensions';

function mountEditor(initialDoc = 'SELECT * FROM users'): {
  view: EditorView;
  parent: HTMLDivElement;
  modelRef: { current: null };
  metadataSnapshotRef: { current: undefined };
} {
  const parent = document.createElement('div');
  document.body.appendChild(parent);

  const modelRef = { current: null };
  const metadataSnapshotRef = { current: undefined };

  const statementExts = createStatementExtensions({ enabled: true });
  const completionExts = createCompletionExtensions(
    { databaseType: 'postgresql' },
    { modelRef, metadataSnapshotRef },
  );
  const intentionExts = createIntentionExtensions(
    { databaseType: 'postgresql' },
    { modelRef, metadataSnapshotRef },
  );
  const hoverExts = createHoverExtensions(
    { databaseType: 'postgresql' },
    { modelRef, metadataSnapshotRef },
  );
  const pasteExts = createPasteExtensions({});
  const linterExts = createLinterExtensions(
    { databaseType: 'postgresql' },
    { modelRef, metadataSnapshotRef },
  );

  const state = EditorState.create({
    doc: initialDoc,
    selection: { anchor: initialDoc.length },
    extensions: [
      ...createBaseEditorExtensions(),
      compartments.statement.of(statementExts),
      compartments.completion.of(completionExts),
      compartments.intention.of(intentionExts),
      compartments.hover.of(hoverExts),
      compartments.paste.of(pasteExts),
      compartments.linter.of(linterExts),
    ],
  });

  const view = new EditorView({ state, parent });
  return { view, parent, modelRef, metadataSnapshotRef };
}

function buildProPayload(
  modelRef: { current: null },
  metadataSnapshotRef: { current: undefined },
) {
  return {
    statement: createStatementExtensions({ enabled: true }),
    completion: createCompletionExtensions(
      { databaseType: 'postgresql' },
      { modelRef, metadataSnapshotRef },
    ),
    intention: createIntentionExtensions(
      { databaseType: 'postgresql' },
      { modelRef, metadataSnapshotRef },
    ),
    hover: createHoverExtensions(
      { databaseType: 'postgresql' },
      { modelRef, metadataSnapshotRef },
    ),
    paste: createPasteExtensions({}),
    linter: createLinterExtensions(
      { databaseType: 'postgresql' },
      { modelRef, metadataSnapshotRef },
    ),
  };
}

function statementExtensionCount(enabled = true): number {
  return createStatementExtensions({ enabled }).length;
}

describe('editorHotplug journey — compartment reconfiguration without state loss', () => {
  beforeEach(() => {
    extensionRegistry.reset();
  });

  afterEach(() => {
    extensionRegistry.reset();
  });

  it('preserves doc, selection, scroll, and undo history across Pro hot-register and hot-unregister', () => {
    const { view, parent, modelRef, metadataSnapshotRef } = mountEditor('SELECT');

    // Simulate user typing " * FROM users" (separate userEvent so undo steps are distinct)
    view.dispatch({
      changes: { from: 6, insert: ' * FROM users' },
      selection: { anchor: view.state.doc.length },
      annotations: Transaction.userEvent.of('input.typing'),
    });
    expect(view.state.doc.toString()).toBe('SELECT * FROM users');

    const docBefore = view.state.doc.toString();
    const selBefore = view.state.selection.main;
    const scrollTopBefore = view.scrollDOM.scrollTop;
    const docEnd = view.state.doc.length;

    view.dispatch({
      changes: { from: docEnd, insert: ' WHERE id = 1' },
      selection: { anchor: docEnd + ' WHERE id = 1'.length },
      annotations: Transaction.userEvent.of('input.typing'),
    });
    expect(undo(view)).toBe(true);
    expect(view.state.doc.toString()).toBe(docBefore);
    expect(view.state.selection.main.anchor).toBe(selBefore.anchor);

    // Hot-register Pro with an extra decoration extension
    const proMarker = EditorView.theme({
      '.pro-hotplug-marker': { color: 'red' },
    });

    let unregister: (() => void) | undefined;
    unregister = extensionRegistry.register(sqlEditorProEP, {
      createStatementDecorations: () => [proMarker],
    });

    expect(statementExtensionCount()).toBe(3);

    const payloadWithPro = buildProPayload(modelRef, metadataSnapshotRef);
    reconfigureProCompartments(view, payloadWithPro);

    // Document, selection, scroll preserved after hot-register
    expect(view.state.doc.toString()).toBe(docBefore);
    expect(view.state.selection.main.anchor).toBe(selBefore.anchor);
    expect(view.scrollDOM.scrollTop).toBe(scrollTopBefore);

    // Hot-unregister Pro → fallback (no pro decoration)
    unregister!();
    expect(statementExtensionCount()).toBe(2);

    const payloadFallback = buildProPayload(modelRef, metadataSnapshotRef);
    reconfigureProCompartments(view, payloadFallback);

    // Still preserved after hot-unregister
    expect(view.state.doc.toString()).toBe(docBefore);
    expect(view.state.selection.main.anchor).toBe(selBefore.anchor);
    expect(view.scrollDOM.scrollTop).toBe(scrollTopBefore);

    view.destroy();
    parent.remove();
  });

  it('compartment reconfigure does not pollute undo/redo history', () => {
    const { view, parent, modelRef, metadataSnapshotRef } = mountEditor('SELECT');

    view.dispatch({
      changes: { from: 6, insert: ' * FROM users' },
      selection: { anchor: view.state.doc.length },
      annotations: Transaction.userEvent.of('input.typing'),
    });
    const docEnd = view.state.doc.length;
    view.dispatch({
      changes: { from: docEnd, insert: ' WHERE id = 1' },
      selection: { anchor: docEnd + ' WHERE id = 1'.length },
      annotations: Transaction.userEvent.of('input.typing'),
    });
    expect(view.state.doc.toString()).toBe('SELECT * FROM users WHERE id = 1');

    expect(undo(view)).toBe(true);
    expect(view.state.doc.toString()).toBe('SELECT * FROM users');

    reconfigureProCompartments(view, buildProPayload(modelRef, metadataSnapshotRef));

    expect(redo(view)).toBe(true);
    expect(view.state.doc.toString()).toBe('SELECT * FROM users WHERE id = 1');

    view.destroy();
    parent.remove();
  });

  it('statement compartment extension count changes dynamically without remounting EditorView', () => {
    const { view, parent, modelRef, metadataSnapshotRef } = mountEditor('SELECT 1');

    const coreCount = createStatementExtensions({ enabled: true }).length;
    expect(coreCount).toBe(2); // statementIndexField + executionStateField

    const dummyDeco = EditorView.theme({});
    const unsub = extensionRegistry.register(sqlEditorProEP, {
      createStatementDecorations: () => [dummyDeco],
    });

    const proCount = createStatementExtensions({ enabled: true }).length;
    expect(proCount).toBe(coreCount + 1);

    reconfigureProCompartments(view, buildProPayload(modelRef, metadataSnapshotRef));
    expect(view.state.doc.toString()).toBe('SELECT 1');

    unsub();
    reconfigureProCompartments(view, buildProPayload(modelRef, metadataSnapshotRef));
    expect(createStatementExtensions({ enabled: true }).length).toBe(coreCount);

    view.destroy();
    parent.remove();
  });

  it('circuit-breaker removes crashing Pro extensions while editor remains editable', () => {
    const { view, parent, modelRef, metadataSnapshotRef } = mountEditor('SELECT 1');

    extensionRegistry.register(sqlEditorProEP, {
      createStatementDecorations: () => {
        throw new Error('pro gutter crash');
      },
    });
    expect(extensionRegistry.isEnhanced(sqlEditorProEP)).toBe(true);

    // SafeCompartmentWrapper inside createStatementExtensions catches and unregisters
    const payload = buildProPayload(modelRef, metadataSnapshotRef);
    expect(extensionRegistry.isEnhanced(sqlEditorProEP)).toBe(false);

    reconfigureProCompartments(view, payload);
    expect(view.state.doc.toString()).toBe('SELECT 1');

    view.dispatch({
      changes: { from: 8, insert: '2' },
      selection: { anchor: 9 },
    });
    expect(view.state.doc.toString()).toBe('SELECT 12');

    view.destroy();
    parent.remove();
  });
});
