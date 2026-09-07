import { describe, it, expect } from 'vitest';
import { createStatementExtensions } from '../../editorExtensions';

describe('createStatementExtensions fallback behavior', () => {
  it('returns pure core extensions without pro gutter or frame decorations when pro is not loaded', () => {
    const extensions = createStatementExtensions({
      onExecuteStatement: () => {},
    });

    expect(Array.isArray(extensions)).toBe(true);
    // statementIndexField + executionStateField = 2 core extensions
    expect(extensions.length).toBe(2);
  });
});
