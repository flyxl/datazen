import type { SqlNamespace } from '../lib/sqlNamespace';
import type { TableInfo, RelationColumns } from '@datazen/driver-sdk';

/** Per-session schema cache entry (map key = runtime DB session id). */
export interface ConnectionSchemaState {
  currentDatabase: string | null;
  /** F7: PG-family current schema — sent as the `schema` envelope field. */
  currentSchema: string | null;
  databases: string[];
  databaseType: string | null;
  isMultiDatabase: boolean;
  tables: TableInfo[];
  /** Directory snapshots are owned by their database, not the active pointer. */
  tableCatalogs: Record<string, TableInfo[]>;
  views: TableInfo[];
  /** All schema names including those with no tables (e.g. from PG schemata). */
  schemaNames: string[];
  relationColumns: Record<string, RelationColumns>;
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

export function createEmptyConnectionSchema(): ConnectionSchemaState {
  return {
    currentDatabase: null,
    currentSchema: null,
    databases: [],
    databaseType: null,
    isMultiDatabase: false,
    tables: [],
    tableCatalogs: {},
    views: [],
    schemaNames: [],
    relationColumns: {},
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
