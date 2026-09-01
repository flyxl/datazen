import { describe, expect, it } from 'vitest';
import { queryToolbarExpandedMinWidth } from '../queryToolbarWidth';

describe('queryToolbarExpandedMinWidth', () => {
  it('counts all query toolbar buttons including explain', () => {
    const width = queryToolbarExpandedMinWidth({
      supportsExplain: true,
      hasContextSelectors: false,
      isPathHierarchy: false,
      isMultiDb: false,
      namespaceTree: {},
      pathAliases: {},
      databases: [],
      contextPath: [],
    });
    // 32 padding + 9 buttons * 84 + 8 gaps + 8 separator
    expect(width).toBe(32 + 9 * 84 + 8 * 8 + 8);
  });

  it('reserves path-hierarchy selector width before namespace loads', () => {
    const withoutTree = queryToolbarExpandedMinWidth({
      supportsExplain: false,
      hasContextSelectors: true,
      isPathHierarchy: true,
      isMultiDb: false,
      namespaceTree: {},
      pathAliases: {},
      databases: [],
      contextPath: [],
    });
    const withoutSelectors = queryToolbarExpandedMinWidth({
      supportsExplain: false,
      hasContextSelectors: false,
      isPathHierarchy: false,
      isMultiDb: false,
      namespaceTree: {},
      pathAliases: {},
      databases: [],
      contextPath: [],
    });
    expect(withoutTree).toBeGreaterThan(withoutSelectors);
  });
});
