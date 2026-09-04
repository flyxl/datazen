import type { SqlNamespace } from '../lib/sqlNamespace';
import type { TableInfo } from '../types';

/** Per-session schema cache entry (map key = runtime DB session id). */
export interface ConnectionSchemaState {
  currentDatabase: string | null;
  /** F7: PG-family current schema — sent as the `schema` envelope field. */
  currentSchema: string | null;
  databases: string[];
  databaseType: string | null;
  isMultiDatabase: boolean;
  tables: TableInfo[];
  views: TableInfo[];
  /** All schema names including those with no tables (e.g. from PG schemata). */
  schemaNames: string[];
  columnMap: Record<string, string[]>;
  namespaceTree: SqlNamespace;
  loadedPaths: Set<string>;
  pathItems: Record<string, TableInfo[]>;
  pathAliases: Record<string, string>;
  namespaceOwnedByPlugin: boolean;
  schemaEpoch: number;
  expanded: Set<string>;
  selectedId: string | null;
  loading: boolean;
  ensuringCount: number;
  error: string | null;
  columnInflight: Set<string>;
}

export const EMPTY_NAMESPACE: SqlNamespace = {};

/** Fallback key when mutating schema without an active DB session (singleton compat). */
export const DEFAULT_SCHEMA_KEY = '__default__';

export const CONNECTION_STATE_KEYS = [
  'currentDatabase',
  'currentSchema',
  'databases',
  'databaseType',
  'isMultiDatabase',
  'tables',
  'views',
  'schemaNames',
  'columnMap',
  'namespaceTree',
  'loadedPaths',
  'pathItems',
  'pathAliases',
  'namespaceOwnedByPlugin',
  'schemaEpoch',
  'expanded',
  'selectedId',
  'loading',
  'ensuringCount',
  'error',
  'columnInflight',
] as const satisfies readonly (keyof ConnectionSchemaState)[];

export function createEmptyConnectionSchema(): ConnectionSchemaState {
  return {
    currentDatabase: null,
    currentSchema: null,
    databases: [],
    databaseType: null,
    isMultiDatabase: false,
    tables: [],
    views: [],
    schemaNames: [],
    columnMap: {},
    namespaceTree: EMPTY_NAMESPACE,
    loadedPaths: new Set(),
    pathItems: {},
    pathAliases: {},
    namespaceOwnedByPlugin: false,
    schemaEpoch: 0,
    expanded: new Set(),
    selectedId: null,
    loading: false,
    ensuringCount: 0,
    error: null,
    columnInflight: new Set(),
  };
}

export function activeFlatten(
  schemas: Map<string, ConnectionSchemaState>,
  activeDbSessionId: string | null,
): ConnectionSchemaState & { dbSessionId: string | null } {
  const readKey =
    activeDbSessionId ?? (schemas.has(DEFAULT_SCHEMA_KEY) ? DEFAULT_SCHEMA_KEY : null);
  if (!readKey) {
    return { ...createEmptyConnectionSchema(), dbSessionId: null };
  }
  const schema = schemas.get(readKey);
  if (!schema) {
    return { ...createEmptyConnectionSchema(), dbSessionId: activeDbSessionId };
  }
  return { ...schema, dbSessionId: activeDbSessionId };
}

export function extractSchemaPatch(
  partial: Record<string, unknown>,
): Partial<ConnectionSchemaState> {
  const result: Partial<ConnectionSchemaState> = {};
  for (const key of CONNECTION_STATE_KEYS) {
    if (!(key in partial)) continue;
    (result as Record<string, unknown>)[key] = partial[key as keyof ConnectionSchemaState];
  }
  return result;
}

export function patchConnectionSchema(
  schemas: Map<string, ConnectionSchemaState>,
  dbSessionId: string,
  patch: Partial<ConnectionSchemaState>,
): Map<string, ConnectionSchemaState> {
  const next = new Map(schemas);
  const prev = next.get(dbSessionId) ?? createEmptyConnectionSchema();
  next.set(dbSessionId, { ...prev, ...patch });
  return next;
}

export function resolveTargetConnectionId(
  state: { activeDbSessionId: string | null },
  dbSessionId?: string,
): string {
  return dbSessionId ?? state.activeDbSessionId ?? DEFAULT_SCHEMA_KEY;
}

export function resolveRealConnectionId(
  state: { activeDbSessionId: string | null },
  dbSessionId?: string,
): string | null {
  return dbSessionId ?? state.activeDbSessionId;
}
