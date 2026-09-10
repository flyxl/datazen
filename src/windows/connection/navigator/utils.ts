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
  if (typeof error === 'string' && error.trim()) return error;
  if (error instanceof Error && error.message.trim()) return error.message;
  if (error && typeof error === 'object') {
    const msg = (error as { message?: unknown }).message;
    if (typeof msg === 'string' && msg.trim()) return msg;
  }
  return fallback;
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

/** Map namespace leaf segments to driver fetch path + SQL schema for table panels. */
export function namespaceLeafContext(
  segments: string[],
  pathAliases: Record<string, string>,
): { tableName: string; schema?: string; database?: string } {
  if (segments.length === 0) return { tableName: '' };
  const tableName = segments[segments.length - 1]!;
  const parentSegments = segments.slice(0, -1);
  if (parentSegments.length === 0) return { tableName };
  const rootId = pathAliases[parentSegments[0]!] ?? parentSegments[0]!;
  const fetchPath =
    parentSegments.length === 1 ? rootId : [rootId, ...parentSegments.slice(1)].join('/');
  const schema = parentSegments.length >= 2 ? parentSegments[parentSegments.length - 1] : undefined;
  return { tableName, schema, database: fetchPath };
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

export function getUnifiedRowKey(row: UnifiedRow, index: number): string {
  switch (row.type) {
    case 'section':
      return `sec:${row.section}`;
    case 'group':
      return `grp:${row.groupName}`;
    case 'connection':
      return `conn:${row.sectionGroup}:${row.conn.id}`;
    case 'db':
      return `db:${row.connectionId}:${row.dbName}`;
    case 'schema':
      return `schema:${row.connectionId}:${row.dbName}:${row.schemaName}`;
    case 'category':
      return `cat:${row.key}`;
    case 'table':
      return `tbl:${row.connectionId}:${row.dbName}:${row.item.schema ?? ''}:${row.item.name}`;
    case 'object':
      return `obj:${row.catId}:${row.obj.name}`;
    case 'kv-db':
      return `kv:${row.connectionId}:${row.dbName}`;
    case 'db-loading':
      return `loading:${index}`;
    case 'namespace-node':
      return `ns:${row.key}`;
    case 'empty-group':
      return `empty:${row.groupName ?? index}`;
    case 'no-connections':
      return 'no-connections';
  }
}

let activeGhost: HTMLElement | null = null;

/**
 * Creates a clean detached ghost element for HTML5 drag-and-drop.
 *
 * Essential for WebKit / Safari: calling setDragImage on elements inside
 * virtualized containers with CSS transforms (like translateY) causes WebKit
 * to snapshot the wrong coordinates (e.g. at (0,0) which shows whatever is at the top
 * of the list, such as "最近 (3)"), or to produce corrupted drag ghosts.
 */
export function createDragGhost(label: string): HTMLElement {
  removeDragGhost();
  const ghost = document.createElement('div');
  ghost.id = 'datazen-drag-ghost';
  ghost.style.cssText =
    'position:fixed;top:-1000px;left:-1000px;padding:4px 10px;border-radius:6px;' +
    'background:#202b3a;color:#f3f4f6;font-size:12px;font-weight:500;' +
    'border:1px solid rgba(59,130,246,0.6);box-shadow:0 4px 12px rgba(0,0,0,0.5);' +
    'pointer-events:none;z-index:99999;white-space:nowrap;user-select:none;-webkit-user-select:none;';
  ghost.textContent = label;
  document.body.appendChild(ghost);
  activeGhost = ghost;
  return ghost;
}

export function removeDragGhost(): void {
  if (activeGhost) {
    try {
      activeGhost.remove();
    } catch {
      // ignore
    }
    activeGhost = null;
  }
  const existing = document.getElementById('datazen-drag-ghost');
  if (existing) {
    try {
      existing.remove();
    } catch {
      // ignore
    }
  }
}
