import {
  isSchemaGroupingSchema,
  pathKey,
  namespaceHasChild,
  type SqlNamespace,
} from './sqlNamespace';
import { DB_REGISTRY } from './databaseTypes';
import type { DatabaseType, TableInfo } from '../types';
import type { DatabaseTypeMeta } from './databaseMeta';
import { resolveEnsureSegments } from './sqlPathPrefix';

export interface EnsureDeps {
  connectionId: string;
  databaseType: string | null;
  isMultiDatabase: boolean;
  loadedPaths: Set<string>;
  /** SQL display name → fetch path root (e.g. numeric id). Filled by plugins via SDK. */
  pathAliases: Record<string, string>;
  namespaceTree: SqlNamespace;
  tables: TableInfo[];
  databases: string[];
  currentDatabase: string | null;
  mergeNamespace: (segments: string[], kind: 'branch' | 'tables', names: string[]) => void;
  registerPathAliases: (entries: { name: string; id: string }[]) => void;
  getDatabases: (connectionId: string) => Promise<string[]>;
  getTables: (connectionId: string, database: string) => Promise<TableInfo[]>;
  useDatabase: (connectionId: string, database: string) => Promise<void>;
}

const inflight = new Map<string, Promise<void>>();

/** Navigation sentinel rows used by path-hierarchy drivers (catalog/schema browsers). */
function isPathNav(item: TableInfo): boolean {
  return item.schema === 'CATALOG' || item.schema === 'SCHEMA';
}

/** Last slash-path segment, optionally stripping `${rootId}/` prefix. */
export function pathNavSegment(item: TableInfo, rootId: string): string {
  const prefix = `${rootId}/`;
  const path = item.name.startsWith(prefix) ? item.name.slice(prefix.length) : item.name;
  const segments = path.split('/').filter(Boolean);
  return segments[segments.length - 1] ?? item.name;
}

function tableNames(items: TableInfo[]): string[] {
  return items.filter((item) => item.tableType !== 'view').map((item) => item.name);
}

function resolvePathRoot(name: string, pathAliases: Record<string, string>): string {
  return pathAliases[name] ?? name;
}

function buildSlashFetchPath(segments: string[], rootId: string): string | null {
  if (segments.length === 0) return null;
  if (segments.length === 1) return rootId;
  return [rootId, ...segments.slice(1)].join('/');
}

function resolveEnsureStrategy(databaseType: string | null): DatabaseTypeMeta['namespaceEnsure'] {
  if (!databaseType) return 'default-sql';
  const meta = DB_REGISTRY[databaseType as DatabaseType];
  if (meta?.namespaceEnsure) return meta.namespaceEnsure;
  if (databaseType === 'postgresql') return 'postgresql';
  return 'default-sql';
}

/**
 * Path-hierarchy ensure: uses plugin-registered pathAliases for the root segment,
 * then `get_tables(rootId[/…])`. Does not parse plugin-specific database list formats.
 */
async function ensurePathHierarchy(segments: string[], deps: EnsureDeps): Promise<void> {
  const { connectionId, pathAliases } = deps;

  if (segments.length === 0) {
    const names = Object.keys(pathAliases);
    if (names.length > 0) {
      deps.mergeNamespace([], 'branch', names);
    }
    return;
  }

  const rootId = resolvePathRoot(segments[0], pathAliases);
  const fetchPath = buildSlashFetchPath(segments, rootId);
  if (!fetchPath) return;

  const items = await deps.getTables(connectionId, fetchPath);

  if (segments.length === 1) {
    const children = items.filter(isPathNav).map((item) => pathNavSegment(item, rootId));
    deps.mergeNamespace(segments, 'branch', children);
    return;
  }

  if (items.some(isPathNav) && segments.length === 2) {
    const children = items.filter(isPathNav).map((item) => pathNavSegment(item, rootId));
    deps.mergeNamespace(segments, 'branch', children);
    return;
  }

  deps.mergeNamespace(segments, 'tables', tableNames(items.filter((item) => !isPathNav(item))));
}

async function ensurePostgresql(segments: string[], deps: EnsureDeps): Promise<void> {
  const { connectionId, isMultiDatabase, tables, databases, currentDatabase } = deps;

  if (segments.length === 0) {
    if (isMultiDatabase) {
      const dbs = await deps.getDatabases(connectionId);
      deps.mergeNamespace([], 'branch', dbs);
      return;
    }

    const preferred =
      (currentDatabase && databases.includes(currentDatabase) ? currentDatabase : null) ??
      databases[0];
    if (!preferred) return;

    await deps.useDatabase(connectionId, preferred);
    const all = await deps.getTables(connectionId, preferred);
    const bySchema = new Map<string, string[]>();
    for (const item of all) {
      if (!isSchemaGroupingSchema(item.schema)) continue;
      const list = bySchema.get(item.schema!) ?? [];
      list.push(item.name);
      bySchema.set(item.schema!, list);
    }
    const schemaNames = [...bySchema.keys()];
    deps.mergeNamespace([], 'branch', schemaNames);
    for (const [schema, names] of bySchema) {
      deps.mergeNamespace([schema], 'tables', names);
    }
    return;
  }

  if (isMultiDatabase && segments.length === 1) {
    const [db] = segments;
    await deps.useDatabase(connectionId, db);
    const all = await deps.getTables(connectionId, db);
    const tableItems = all.filter((item) => item.tableType !== 'view');
    const bySchema = new Map<string, string[]>();
    for (const item of tableItems) {
      if (!isSchemaGroupingSchema(item.schema)) continue;
      const list = bySchema.get(item.schema!) ?? [];
      list.push(item.name);
      bySchema.set(item.schema!, list);
    }
    const schemaNames = [...bySchema.keys()];
    deps.mergeNamespace([db], 'branch', schemaNames);
    for (const [schema, names] of bySchema) {
      deps.mergeNamespace([db, schema], 'tables', names);
    }
    return;
  }

  if (!isMultiDatabase && segments.length === 1) {
    const [schema] = segments;
    const fromMemory = tables.filter((item) => item.tableType !== 'view' && item.schema === schema);
    if (fromMemory.length > 0) {
      deps.mergeNamespace(
        [schema],
        'tables',
        fromMemory.map((item) => item.name),
      );
      return;
    }

    const preferred =
      (currentDatabase && databases.includes(currentDatabase) ? currentDatabase : null) ??
      databases[0];
    if (!preferred) return;

    await deps.useDatabase(connectionId, preferred);
    const all = await deps.getTables(connectionId, preferred);
    const names = all
      .filter((item) => item.tableType !== 'view' && item.schema === schema)
      .map((item) => item.name);
    deps.mergeNamespace([schema], 'tables', names);
  }
}

async function ensureDefaultSql(segments: string[], deps: EnsureDeps): Promise<void> {
  const { connectionId } = deps;

  if (segments.length === 0) {
    const dbs = await deps.getDatabases(connectionId);
    deps.mergeNamespace([], 'branch', dbs);
    return;
  }

  if (segments.length === 1) {
    const [db] = segments;
    await deps.useDatabase(connectionId, db);
    const all = await deps.getTables(connectionId, db);
    deps.mergeNamespace([db], 'tables', tableNames(all));
  }
}

async function runEnsure(segments: string[], deps: EnsureDeps): Promise<void> {
  if (deps.loadedPaths.has(pathKey(segments))) return;

  try {
    const strategy = resolveEnsureStrategy(deps.databaseType);
    switch (strategy) {
      case 'path-hierarchy':
        await ensurePathHierarchy(segments, deps);
        break;
      case 'postgresql':
        await ensurePostgresql(segments, deps);
        break;
      default:
        await ensureDefaultSql(segments, deps);
        break;
    }
  } catch {
    // Failed ensure: do not mark path loaded.
  }
}

function usesCurrentDatabaseRoot(deps: EnsureDeps): boolean {
  if (!deps.currentDatabase) return false;
  const strategy = resolveEnsureStrategy(deps.databaseType);
  if (strategy === 'path-hierarchy' || strategy === 'default-sql') return true;
  return strategy === 'postgresql' && deps.isMultiDatabase;
}

function resolveForEnsure(segments: string[], deps: EnsureDeps): string[] {
  return resolveEnsureSegments(segments, {
    currentDatabase: deps.currentDatabase,
    knownRoots: [...deps.databases, ...Object.keys(deps.pathAliases)],
    useCurrentDatabaseRoot: usesCurrentDatabaseRoot(deps),
  });
}

/** True when this path is not in `loadedPaths` yet (a fetch may still be in flight). */
export function namespaceEnsurePending(segments: string[], deps: EnsureDeps): boolean {
  return !deps.loadedPaths.has(pathKey(resolveForEnsure(segments, deps)));
}

export async function ensureNamespacePath(segments: string[], deps: EnsureDeps): Promise<void> {
  const resolved = resolveForEnsure(segments, deps);
  const key = `${deps.connectionId}|${resolved.join('.')}`;
  const existing = inflight.get(key);
  if (existing) return existing;
  const p = runEnsure(resolved, deps).finally(() => inflight.delete(key));
  inflight.set(key, p);
  return p;
}

export function namespacePathLoaded(deps: EnsureDeps, segments: string[]): boolean {
  return deps.loadedPaths.has(pathKey(segments)) || namespaceHasChild(deps.namespaceTree, segments);
}
