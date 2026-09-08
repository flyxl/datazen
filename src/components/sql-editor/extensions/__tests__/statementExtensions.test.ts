import { describe, it, expect, vi } from 'vitest';
import { createStatementExtensions } from '../../editorExtensions';
import { extensionRegistry, sqlEditorProEP } from '@datazen/extension-points';
import { EditorView } from '@codemirror/view';

describe('createStatementExtensions fallback behavior', () => {
  it('returns pure core extensions without pro gutter or frame decorations when pro is not loaded', () => {
    const extensions = createStatementExtensions({
      onExecuteStatement: () => {},
    });

    expect(Array.isArray(extensions)).toBe(true);
    // statementIndexField + executionStateField = 2 core extensions
    expect(extensions.length).toBe(2);
  });

  it('respects enabled: false and suppresses pro decorations', () => {
    const dummyDecoration = EditorView.theme({});
    const unregister = extensionRegistry.register(sqlEditorProEP, {
      createStatementDecorations: () => [dummyDecoration],
    });

    try {
      const enabledExts = createStatementExtensions({ enabled: true });
      expect(enabledExts.length).toBe(3);

      const disabledExts = createStatementExtensions({ enabled: false });
      expect(disabledExts.length).toBe(2);
    } finally {
      unregister();
    }
  });
});
