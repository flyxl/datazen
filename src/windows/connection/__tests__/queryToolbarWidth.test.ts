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

  it('adds path-hierarchy selector width by level count', () => {
    const withOneLevel = queryToolbarExpandedMinWidth({
      supportsExplain: false,
      hasContextSelectors: true,
      isPathHierarchy: true,
      isMultiDb: false,
      namespaceTree: { hive: {} },
      pathAliases: { hive: '558' },
      databases: [],
      contextPath: [],
    });
    const withTwoLevels = queryToolbarExpandedMinWidth({
      supportsExplain: false,
      hasContextSelectors: true,
      isPathHierarchy: true,
      isMultiDb: false,
      namespaceTree: { hive: { snap: {} } },
      pathAliases: { hive: '558' },
      databases: [],
      contextPath: ['hive'],
    });
    expect(withTwoLevels).toBeGreaterThan(withOneLevel);
  });
});
