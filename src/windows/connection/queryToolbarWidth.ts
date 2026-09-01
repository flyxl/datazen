import { TOOLBAR_GAP, TOOLBAR_HORIZONTAL_PADDING } from '../../hooks/useCompactToolbar';
import { namespaceRootsFrom } from '../../lib/queryContextPath';
import type { SqlNamespace } from '../../lib/sqlNamespace';

/** Slightly under measured width so expanded labels show when space is adequate. */
const QUERY_TOOLBAR_BUTTON_WIDTH = 84;

export function queryToolbarExpandedMinWidth(options: {
  supportsExplain: boolean;
  hasContextSelectors: boolean;
  isPathHierarchy: boolean;
  isMultiDb: boolean;
  contextSchema?: string | null;
  namespaceTree: SqlNamespace;
  pathAliases: Record<string, string>;
  databases: readonly string[];
  contextPath: readonly string[];
}): number {
  const buttonCount = 8 + (options.supportsExplain ? 1 : 0);

  let contextWidth = 0;
  if (options.hasContextSelectors) {
    if (options.isPathHierarchy) {
      const roots = namespaceRootsFrom(
        options.namespaceTree,
        options.pathAliases,
        options.databases,
      );
      if (roots.length > 0) {
        const levelCount = Math.max(1, Math.min(4, 1 + options.contextPath.length));
        contextWidth = 20 + levelCount * 112 + Math.max(0, levelCount - 1) * 6;
      }
    } else if (options.isMultiDb) {
      contextWidth = 196;
    } else if (options.contextSchema) {
      contextWidth = 96;
    }
  }

  return (
    TOOLBAR_HORIZONTAL_PADDING +
    buttonCount * QUERY_TOOLBAR_BUTTON_WIDTH +
    Math.max(0, buttonCount - 1) * TOOLBAR_GAP +
    contextWidth +
    8
  );
}
