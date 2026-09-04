import type { DatabaseType, TableInfo } from '../types';
import {
  collectTableLeafNames,
  type SqlNamespace,
} from '../lib/sqlNamespace';

/** Session multi-db UI: capability flag AND more than one *visible* database. */
export function computeIsMultiDatabase(
  hasMultiDatabase: boolean | undefined,
  databaseCount: number,
): boolean {
  return Boolean(hasMultiDatabase && databaseCount > 1);
}

export function resolvePreferredDatabase(
  databases: string[],
  preferredDatabase?: string,
): string | null {
  if (preferredDatabase && databases.includes(preferredDatabase)) {
    return preferredDatabase;
  }
  return databases[0] ?? null;
}

/**
 * When the connection config specifies a *logical* database that appears in
 * the driver list, lock the sidebar to that single database.
 */
export function resolveVisibleDatabases(
  allDatabases: string[],
  preferredDatabase?: string,
): { databases: string[]; preferred: string | null; lockedToConfigured: boolean } {
  const configured = preferredDatabase?.trim();
  if (configured && allDatabases.includes(configured)) {
    return {
      databases: [configured],
      preferred: configured,
      lockedToConfigured: true,
    };
  }
  return {
    databases: allDatabases,
    preferred: resolvePreferredDatabase(allDatabases, configured || undefined),
    lockedToConfigured: false,
  };
}

/** Parse plugin database list entries (`id:name (backend)`) for path-hierarchy trees. */
export function parsePathHierarchyDatabaseEntry(entry: string): { id: string; name: string } {
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

/** Names that are safe to pass to `get_columns` (complete loaded tables only). */
export function knownTableNames(
  namespaceTree: SqlNamespace,
  tables: TableInfo[],
  views: TableInfo[] = [],
  pathItems: Record<string, TableInfo[]> = {},
): Set<string> {
  const names = collectTableLeafNames(namespaceTree);
  for (const item of [...tables, ...views]) {
    names.add(item.name);
  }
  for (const items of Object.values(pathItems)) {
    for (const item of items) {
      if (item.schema === 'CATALOG' || item.schema === 'SCHEMA') continue;
      const parts = item.name.split('/').filter(Boolean);
      names.add(parts[parts.length - 1] ?? item.name);
    }
  }
  return names;
}

export interface LoadForConnectionOptions {
  skipLoadTables?: boolean;
  preferredDatabase?: string;
  /** Used to resolve hasMultiDatabase for session isMultiDatabase. */
  databaseType?: DatabaseType | string;
}
