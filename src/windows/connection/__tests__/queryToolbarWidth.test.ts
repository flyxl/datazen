import { describe, expect, it } from 'vitest';
import { queryToolbarExpandedMinWidth } from '../queryToolbarWidth';

describe('queryToolbarExpandedMinWidth', () => {
  it('counts consolidated top-level toolbar buttons', () => {
    const width = queryToolbarExpandedMinWidth({
      hasContextSelectors: false,
      isPathHierarchy: false,
      isMultiDb: false,
      namespaceTree: {},
      pathAliases: {},
      databases: [],
      contextPath: [],
    });
    // 32 padding + 7 buttons * 84 + 6 gaps + 8 separator
    expect(width).toBe(32 + 7 * 84 + 6 * 8 + 8);
  });

  it('adds commit and rollback buttons when in transaction', () => {
    const idle = queryToolbarExpandedMinWidth({
      hasContextSelectors: false,
      isPathHierarchy: false,
      isMultiDb: false,
      inTransaction: false,
      namespaceTree: {},
      pathAliases: {},
      databases: [],
      contextPath: [],
    });
    const inTx = queryToolbarExpandedMinWidth({
      hasContextSelectors: false,
      isPathHierarchy: false,
      isMultiDb: false,
      inTransaction: true,
      namespaceTree: {},
      pathAliases: {},
      databases: [],
      contextPath: [],
    });
    expect(inTx - idle).toBe(2 * 84 + 2 * 8);
  });

  it('calculates compact required width with consolidated layout', () => {
    const width = queryToolbarExpandedMinWidth({
      hasContextSelectors: true,
      isPathHierarchy: false,
      isMultiDb: false,
      namespaceTree: {},
      pathAliases: {},
      databases: [],
      contextPath: [],
    });
    expect(width).toBeGreaterThan(400);
    expect(width).toBeLessThan(1200);
  });

  it('reserves path-hierarchy selector width before namespace loads', () => {
    const withoutTree = queryToolbarExpandedMinWidth({
      hasContextSelectors: true,
      isPathHierarchy: true,
      isMultiDb: false,
      namespaceTree: {},
      pathAliases: {},
      databases: [],
      contextPath: [],
    });
    const withoutSelectors = queryToolbarExpandedMinWidth({
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

  it('reserves compact multi-db selector width', () => {
    const withMultiDb = queryToolbarExpandedMinWidth({
      hasContextSelectors: true,
      isPathHierarchy: false,
      isMultiDb: true,
      namespaceTree: {},
      pathAliases: {},
      databases: ['postgres'],
      contextPath: [],
      currentDatabase: 'postgres',
    });
    const withoutSelectors = queryToolbarExpandedMinWidth({
      hasContextSelectors: false,
      isPathHierarchy: false,
      isMultiDb: false,
      namespaceTree: {},
      pathAliases: {},
      databases: [],
      contextPath: [],
    });
    expect(withMultiDb).toBeGreaterThan(withoutSelectors);
    expect(withMultiDb - withoutSelectors).toBeLessThan(140);
  });
});
