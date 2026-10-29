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
    // 32 padding + 11 icon-only buttons * 32 + 10 gaps * 8 + 236 status zone + 8
    expect(width).toBe(32 + 11 * 32 + 10 * 8 + 236 + 8);
  });

  it('adds the Explain button when the driver supports it', () => {
    const withoutExplain = queryToolbarExpandedMinWidth({
      hasContextSelectors: false,
      isPathHierarchy: false,
      isMultiDb: false,
      namespaceTree: {},
      pathAliases: {},
      databases: [],
      contextPath: [],
    });
    const withExplain = queryToolbarExpandedMinWidth({
      hasContextSelectors: false,
      isPathHierarchy: false,
      isMultiDb: false,
      supportsExplain: true,
      namespaceTree: {},
      pathAliases: {},
      databases: [],
      contextPath: [],
    });
    expect(withExplain - withoutExplain).toBe(32 + 8);
  });

  it('replaces begin transaction with commit and rollback when in transaction', () => {
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
    // One extra icon button plus one extra gap.
    expect(inTx - idle).toBe(32 + 8);
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
