import { effectiveExpanded, searchedRowLevels } from '@datazen/ui';
import { isLeaf, pathKey, type SqlNamespace } from '../../../lib/sqlNamespace';
import { escapeIdent } from '../../../lib/databaseTypes';
import { databaseObjectIdentityKey } from '../../../lib/databaseObjectIdentity';
import type { DatabaseTypeMeta } from '@datazen/extension-points';
import type { ConnectionConfig, TableInfo } from '../../../types';
import type { UnifiedRow } from './types';

/**
 * The depth every child of a connection is painted at — databases, namespace
 * branches, and their loading placeholders alike.
 *
 * A connection is emitted at depth 1 beneath its section/group header, or at
 * depth 0 when a search suppresses that header. Its children do not move with
 * it: they always start here, so a search removes exactly one level of parent
 * between a connection and its databases.
 *
 * It lives here rather than in `buildFlatRows` because `flattenNamespaceTree`
 * builds rows of the same ladder and would otherwise have to import from a
 * module that already imports it.
 */
export const CONNECTION_CHILD_DEPTH = 2;

/**
 * The depth a database's children are painted at — its schema row, and the
 * placeholders shown while it loads.
 *
 * Derived rather than declared, so the ladder has a single root: move
 * `CONNECTION_CHILD_DEPTH` and every level below it follows. A database with
 * no schemas puts its categories here instead of one level deeper.
 */
export const DATABASE_CHILD_DEPTH = CONNECTION_CHILD_DEPTH + 1;

/**
 * Multi-DB tree when the driver supports it, unless connection.database is a
 * *logical* DB name that should lock the sidebar.
 *
 * Kiwi (`databaseFieldType: 'domain'`) stores the instance domain in
 * `connection.database` — that must not force StandardSchemaTree.
 */
export function shouldUseMultiDatabaseTree(
  meta: Pick<DatabaseTypeMeta, 'hasMultiDatabase' | 'databaseFieldType'> | undefined,
  initialDatabase?: string,
): boolean {
  if (!meta?.hasMultiDatabase) return false;
  if (meta.databaseFieldType === 'domain') return true;
  return !initialDatabase?.trim();
}

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
  schema: string | null,
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
): { tableName: string; schema: string | null; database: string | null } {
  if (segments.length === 0) return { tableName: '', schema: null, database: null };
  const tableName = segments[segments.length - 1]!;
  const parentSegments = segments.slice(0, -1);
  if (parentSegments.length === 0) return { tableName, schema: null, database: null };
  const rootId = pathAliases[parentSegments[0]!] ?? parentSegments[0]!;
  const fetchPath =
    parentSegments.length === 1 ? rootId : [rootId, ...parentSegments.slice(1)].join('/');
  const schema = parentSegments.length >= 2 ? parentSegments[parentSegments.length - 1]! : null;
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

  const searching = query !== '';
  const levels = (depth: number) => searchedRowLevels(depth, CONNECTION_CHILD_DEPTH, searching);
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
        ...levels(baseDepth),
        expanded: false,
        isLeaf: true,
        leafKind: tableTypeMap.get(name) ?? 'table',
        segments,
        key: nodeKey,
        connectionId,
        dbSessionId,
      });
    } else {
      const expanded = effectiveExpanded(expandedDbs.has(nodeKey), searching);
      rows.push({
        type: 'namespace-node',
        name,
        ...levels(baseDepth),
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
          // This spinner stands in for the namespace path still being ensured,
          // so it is owned by that path — not by where it happens to sit.
          rows.push({
            type: 'db-loading',
            ...levels(baseDepth + 1),
            ownerKey: `ns:${pathKey(segments)}`,
          });
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

/**
 * Stable identity of a navigator row, for use as a React `<key>`.
 *
 * **No index parameter, on purpose.** A virtualizer only ever hands a renderer
 * positions, and a position is not an identity: scroll one row out of the
 * window and every row after it changes index. An index-derived key therefore
 * remounts a suffix of the list on every scroll frame — losing focus,
 * scroll-into-view and in-flight transitions — and, worse, *two different rows
 * can share a key* whenever the list is filtered. Two of the cases below did
 * exactly that:
 *
 * - `db-loading` keyed off its own list position, so the spinner that was
 *   waiting for database A became the spinner for database B as soon as A's
 *   row scrolled out of the window. It now names the row it stands in for
 *   (`ownerKey`).
 * - `empty-group` fell back to `index` whenever `groupName` was missing, which
 *   made the hint row under a group rename itself whenever anything above it
 *   appeared or disappeared. `groupName` is required now.
 * - `object` rows keyed off `(catId, name)` alone, so two connections that each
 *   hold a function called `fn_calc` produced one duplicate key. The row now
 *   carries its owner tuple, the same as the `table` variant.
 *
 * Every branch is built from fields that identify *which object the row is*,
 * so the key survives re-sorts, re-filters and window changes.
 */
export function getUnifiedRowKey(row: UnifiedRow): string {
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
      return `obj:${row.connectionId}:${row.dbName}:${row.schemaName ?? ''}:${row.catId}:${databaseObjectIdentityKey(row.obj)}`;
    case 'kv-db':
      return `kv:${row.connectionId}:${row.dbName}`;
    case 'db-loading':
      return `loading:${row.ownerKey}`;
    case 'db-error':
      return `error:${row.ownerKey}`;
    case 'namespace-node':
      return `ns:${row.key}`;
    case 'empty-group':
      return `empty:${row.groupName}`;
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
