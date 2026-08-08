import { pathKey, namespaceHasChild, type SqlNamespace } from './sqlNamespace';
import type { TableInfo } from '../types';

export interface EnsureDeps {
  connectionId: string;
  databaseType: string | null;
  isMultiDatabase: boolean;
  loadedPaths: Set<string>;
  supersetDbIds: Record<string, string>;
  namespaceTree: SqlNamespace;
  tables: TableInfo[];
  databases: string[];
  currentDatabase: string | null;
  mergeNamespace: (segments: string[], kind: 'branch' | 'tables', names: string[]) => void;
  registerSupersetDatabases: (entries: { name: string; id: string }[]) => void;
  getDatabases: (connectionId: string) => Promise<string[]>;
  getTables: (connectionId: string, database: string) => Promise<TableInfo[]>;
  useDatabase: (connectionId: string, database: string) => Promise<void>;
}

const inflight = new Map<string, Promise<void>>();

function isSchemaGroupingSchema(schema: string | null | undefined): boolean {
  return schema != null && schema !== 'CATALOG' && schema !== 'SCHEMA';
}

function isSupersetNav(item: TableInfo): boolean {
  return item.schema === 'CATALOG' || item.schema === 'SCHEMA';
}

/** Parse Superset `get_databases` entry: `558:presto (Presto)` → { id, name }. */
export function parseSupersetDatabaseEntry(entry: string): { id: string; name: string } {
  const colonIdx = entry.indexOf(':');
  if (colonIdx < 0) return { id: entry, name: entry };
  const id = entry.slice(0, colonIdx);
  const rest = entry.slice(colonIdx + 1);
  const backendMatch = rest.match(/^(.*?)\s*\(([^)]+)\)$/);
  if (backendMatch) {
    return { id, name: backendMatch[1].trim() };
  }
  return { id, name: rest };
}

/** SQL segment name for a Superset navigation row under dbId. */
export function supersetNavSegment(item: TableInfo, dbId: string): string {
  const prefix = `${dbId}/`;
  const path = item.name.startsWith(prefix) ? item.name.slice(prefix.length) : item.name;
  const segments = path.split('/').filter(Boolean);
  return segments[segments.length - 1] ?? item.name;
}

function tableNames(items: TableInfo[]): string[] {
  return items.filter((item) => item.tableType !== 'view').map((item) => item.name);
}

function resolveSupersetDbId(dbName: string, supersetDbIds: Record<string, string>): string | null {
  return supersetDbIds[dbName] ?? null;
}

async function registerSupersetDatabaseEntries(
  deps: EnsureDeps,
): Promise<{ name: string; id: string }[]> {
  const entries = await deps.getDatabases(deps.connectionId);
  const parsed = entries.map(parseSupersetDatabaseEntry);
  const registered = parsed.map((p) => ({ name: p.name, id: p.id }));
  deps.registerSupersetDatabases(registered);
  return registered;
}

function buildSupersetFetchPath(segments: string[], dbId: string): string | null {
  if (segments.length === 0) return null;
  if (segments.length === 1) return dbId;
  return [dbId, ...segments.slice(1)].join('/');
}

async function ensureSuperset(segments: string[], deps: EnsureDeps): Promise<void> {
  const { connectionId, supersetDbIds } = deps;

  if (segments.length === 0) {
    await registerSupersetDatabaseEntries(deps);
    return;
  }

  const dbName = segments[0];
  let dbId = resolveSupersetDbId(dbName, supersetDbIds);
  if (!dbId) {
    const registered = await registerSupersetDatabaseEntries(deps);
    dbId = registered.find((entry) => entry.name === dbName)?.id ?? null;
    if (!dbId) return;
  }

  const fetchPath = buildSupersetFetchPath(segments, dbId);
  if (!fetchPath) return;

  const items = await deps.getTables(connectionId, fetchPath);

  if (segments.length === 1) {
    const catalogs = items.filter(isSupersetNav).map((item) => supersetNavSegment(item, dbId));
    deps.mergeNamespace(segments, 'branch', catalogs);
    return;
  }

  if (segments.length === 2) {
    const schemas = items.filter(isSupersetNav).map((item) => supersetNavSegment(item, dbId));
    deps.mergeNamespace(segments, 'branch', schemas);
    return;
  }

  deps.mergeNamespace(segments, 'tables', tableNames(items.filter((item) => !isSupersetNav(item))));
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
    const fromMemory = tables.filter(
      (item) => item.tableType !== 'view' && item.schema === schema,
    );
    if (fromMemory.length > 0) {
      deps.mergeNamespace([schema], 'tables', fromMemory.map((item) => item.name));
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
    switch (deps.databaseType) {
      case 'superset':
        await ensureSuperset(segments, deps);
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

export async function ensureNamespacePath(segments: string[], deps: EnsureDeps): Promise<void> {
  const key = `${deps.connectionId}|${segments.join('.')}`;
  const existing = inflight.get(key);
  if (existing) return existing;
  const p = runEnsure(segments, deps).finally(() => inflight.delete(key));
  inflight.set(key, p);
  return p;
}

export function namespacePathLoaded(deps: EnsureDeps, segments: string[]): boolean {
  return deps.loadedPaths.has(pathKey(segments)) || namespaceHasChild(deps.namespaceTree, segments);
}
