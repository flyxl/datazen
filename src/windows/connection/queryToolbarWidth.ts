import { TOOLBAR_GAP, TOOLBAR_HORIZONTAL_PADDING } from '../../hooks/useCompactToolbar';
import { pathHierarchySelectorSegmentsForUi } from '../../lib/queryContextPath';
import type { SqlNamespace } from '../../lib/sqlNamespace';

/** Slightly under measured width so expanded labels show when space is adequate. */
const QUERY_TOOLBAR_BUTTON_WIDTH = 84;
const PATH_HIERARCHY_SELECT_MAX = 88;
const PATH_HIERARCHY_SELECT_MIN = 40;
const PATH_HIERARCHY_LABEL_MAX = 72;
const PATH_HIERARCHY_CHAR_PX = 6.5;
const PATH_HIERARCHY_SELECT_CHROME = 28;
const PATH_HIERARCHY_SEGMENT_GAP = 8;
const PATH_HIERARCHY_PLACEHOLDER_CHARS = 6;

function queryContextSelectWidth(value: string): number {
  const len = value.length > 0 ? value.length : PATH_HIERARCHY_PLACEHOLDER_CHARS;
  return Math.min(
    PATH_HIERARCHY_SELECT_MAX,
    Math.max(
      PATH_HIERARCHY_SELECT_MIN,
      Math.ceil(len * PATH_HIERARCHY_CHAR_PX + PATH_HIERARCHY_SELECT_CHROME),
    ),
  );
}

function pathHierarchyLabelWidth(name: string): number {
  return Math.min(
    PATH_HIERARCHY_LABEL_MAX,
    Math.max(24, Math.ceil(name.length * PATH_HIERARCHY_CHAR_PX)),
  );
}

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
  currentDatabase?: string | null;
}): number {
  const buttonCount = 8 + (options.supportsExplain ? 1 : 0);

  let contextWidth = 0;
  if (options.hasContextSelectors) {
    if (options.isPathHierarchy) {
      const segments = pathHierarchySelectorSegmentsForUi(
        options.namespaceTree,
        options.pathAliases,
        options.databases,
        options.contextPath,
      );
      const segmentWidth = segments.reduce((sum, segment) => {
        if (segment.kind === 'label') {
          return sum + pathHierarchyLabelWidth(segment.name);
        }
        return sum + queryContextSelectWidth(segment.value);
      }, 0);
      contextWidth =
        20 +
        Math.max(segmentWidth, PATH_HIERARCHY_SELECT_MIN * 2) +
        Math.max(0, segments.length - 1) * PATH_HIERARCHY_SEGMENT_GAP;
    } else if (options.isMultiDb) {
      const selectWidth = queryContextSelectWidth(
        options.currentDatabase ?? options.databases[0] ?? '',
      );
      contextWidth = 20 + Math.max(selectWidth, PATH_HIERARCHY_SELECT_MIN * 2);
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
