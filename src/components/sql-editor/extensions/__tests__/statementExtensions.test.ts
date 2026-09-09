import { describe, it, expect, vi } from 'vitest';
import { createStatementExtensions } from '../../editorExtensions';
import { extensionRegistry, sqlEditorEnhancedEP } from '@datazen/extension-points';
import { EditorView } from '@codemirror/view';

describe('createStatementExtensions fallback behavior', () => {
  it('returns pure core extensions without enhanced gutter or frame decorations when enhanced is not loaded', () => {
    const extensions = createStatementExtensions({
      onExecuteStatement: () => {},
    });

    expect(Array.isArray(extensions)).toBe(true);
    // statementIndexField + executionStateField = 2 core extensions
    expect(extensions.length).toBe(2);
  });

  it('respects enabled: false and suppresses enhanced decorations', () => {
    const dummyDecoration = EditorView.theme({});
    const unregister = extensionRegistry.register(sqlEditorEnhancedEP, {
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
