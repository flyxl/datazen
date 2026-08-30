import { isLeaf, pathKey, type SqlNamespace } from '../../../lib/sqlNamespace';
import { escapeIdent } from '../../../lib/databaseTypes';
import type { ConnectionConfig, TableInfo } from '../../../types';
import type { UnifiedRow } from './types';

export function depthPadding(depth: number): string {
  return `${0.375 + depth * 1}rem`;
}

export function groupBySchema(
  items: TableInfo[],
  extraSchemaNames?: string[],
): Map<string, TableInfo[]> | null {
  const hasAnySchema = items.some((i) => !!i.schema);
  if (!hasAnySchema && (!extraSchemaNames || extraSchemaNames.length === 0)) return null;

  const map = new Map<string, TableInfo[]>();
  if (extraSchemaNames) {
    for (const s of extraSchemaNames) map.set(s, []);
  }
  for (const item of items) {
    if (!item.name) continue;
    const key = item.schema ?? '';
    const arr = map.get(key);
    if (arr) arr.push(item);
    else map.set(key, [item]);
  }
  return map;
}

/** Pick a safe database to switch to before dropping `dropping`. */
export function resolveDropDatabaseFallback(
  databases: string[],
  dropping: string,
  configuredDb?: string,
): string | null {
  if (databases.includes('postgres') && dropping !== 'postgres') return 'postgres';
  const configured = configuredDb?.trim();
  if (configured && configured !== dropping && databases.includes(configured)) {
    return configured;
  }
  return databases.find((d) => d !== dropping) ?? null;
}

export function extractErrorMessage(error: unknown, fallback: string): string {
  return error instanceof Error ? error.message : fallback;
}

export function quoteRelationName(
  name: string,
  schema: string | undefined,
  databaseType: string,
): string {
  const quote = (part: string) =>
    escapeIdent(part, databaseType as ConnectionConfig['databaseType']);
  return schema ? `${quote(schema)}.${quote(name)}` : quote(name);
}

/** Check if any key in the namespace tree contains the query string. */
export function namespaceTreeContains(tree: SqlNamespace, query: string): boolean {
  if (isLeaf(tree)) return false;
  for (const [key, child] of Object.entries(tree)) {
    if (key.toLowerCase().includes(query)) return true;
    if (!isLeaf(child) && namespaceTreeContains(child, query)) return true;
  }
  return false;
}

/**
 * Flatten a SqlNamespace tree into UnifiedRow entries for path-hierarchy drivers.
 * Branches become expandable namespace-node rows; leaves get their kind from tables metadata.
 */
export function flattenNamespaceTree(
  tree: SqlNamespace,
  connectionId: string,
  dbSessionId: string,
  baseDepth: number,
  rows: UnifiedRow[],
  expandedDbs: Set<string>,
  query: string,
  tableTypeMap: Map<string, TableInfo['tableType']>,
  loadedPaths: Set<string>,
  parentSegments: string[] = [],
): void {
  if (isLeaf(tree)) return;

  const entries = Object.entries(tree).sort(([a], [b]) => a.localeCompare(b));
  for (const [name, child] of entries) {
    if (query && !name.toLowerCase().includes(query)) {
      if (isLeaf(child)) continue;
      if (!namespaceTreeContains(child, query)) continue;
    }

    const segments = [...parentSegments, name];
    const nodeKey = `${connectionId}::ns::${segments.join('/')}`;
    const nodeIsLeaf = isLeaf(child);

    if (nodeIsLeaf) {
      rows.push({
        type: 'namespace-node',
        name,
        depth: baseDepth,
        expanded: false,
        isLeaf: true,
        leafKind: tableTypeMap.get(name) ?? 'table',
        segments,
        key: nodeKey,
        connectionId,
        dbSessionId,
      });
    } else {
      const expanded = expandedDbs.has(nodeKey) || !!query;
      rows.push({
        type: 'namespace-node',
        name,
        depth: baseDepth,
        expanded,
        isLeaf: false,
        segments,
        key: nodeKey,
        connectionId,
        dbSessionId,
      });
      if (expanded) {
        const childEntries = Object.entries(child);
        const pathLoaded = loadedPaths.has(pathKey(segments));
        if (childEntries.length === 0 && !pathLoaded && !query) {
          rows.push({ type: 'db-loading', depth: baseDepth + 1 });
        } else {
          flattenNamespaceTree(
            child,
            connectionId,
            dbSessionId,
            baseDepth + 1,
            rows,
            expandedDbs,
            query,
            tableTypeMap,
            loadedPaths,
            segments,
          );
        }
      }
    }
  }
}
