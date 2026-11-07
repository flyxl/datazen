/** Database engine identifiers for the current build (injected via resolve-drivers). */
export type { DatabaseType } from '../extensions/generated';
import type { DatabaseType } from '../extensions/generated';
import type { SslMode, DatabaseObjectKind } from '@datazen/extension-points';
import type { SshAuthMethod } from '@datazen/driver-sdk';
import type { Value } from '@datazen/backend-client';

export type { SslMode, DatabaseObjectKind } from '@datazen/extension-points';
export type { SshAuthMethod } from '@datazen/driver-sdk';
export type {
  ColumnInfo,
  MultiQueryResult,
  QueryResult,
  QueryStreamEvent,
  StatementResult,
  Value,
} from '@datazen/backend-client';


import type { HttpProxyTunnelConfig, TunnelKind, WebSocketTunnelConfig } from '@datazen/driver-sdk';

/**
 * Connection-facing types.
 *
 * NOTE: most of this module is declared again, identically, in `./index` — and
 * the app imports the `./index` copies. `FavoriteQuery` had a second copy here
 * until the file-first branch, which is what this note records: keep new
 * fields in `./index` only, or the two shapes drift apart silently.
 */
export type {
  TunnelKind,
  HttpProxyTunnelConfig,
  WebSocketTunnelConfig,
  SavedTunnel,
} from '@datazen/driver-sdk';

export interface SshTunnelConfig {
  enabled: boolean;
  host: string;
  port: number;
  username: string;
  authMethod: SshAuthMethod;
  password?: string;
  privateKeyPath?: string;
  passphrase?: string;
  /** Optional ProxyJump hop. */
  jump?: SshTunnelConfig;
}

export interface ConnectionConfig {
  id: string;
  name: string;
  databaseType: DatabaseType;
  host?: string;
  port?: number;
  database?: string;
  /** Presto/Trino schema within catalog */
  schema?: string;
  username?: string;
  password?: string;
  sslMode: SslMode;
  connectionTimeout?: number;
  /** Host-injected pool size; not typically set in the connection form. */
  maxPoolSize?: number;
  sshTunnel?: SshTunnelConfig;
  /** Preferred tunnel strategy. When absent, inferred from legacy ssh/http/ws fields. */
  tunnelKind?: TunnelKind;
  /** Reference to a SavedTunnel in tunnels.json. When set, runtime resolves tunnel fields. */
  tunnelId?: string;
  httpProxyTunnel?: HttpProxyTunnelConfig;
  websocketTunnel?: WebSocketTunnelConfig;
  colorTag?: string;
  group?: string;
  lastConnectedAt?: string;
  serverVersion?: string;
  /** Opaque per-driver connection options (e.g. Redis topology/TLS). */
  options?: Record<string, unknown>;
  /** When true, the host rejects mutating SQL and row edits. */
  readOnly?: boolean;
  /** When true, sorted first within the connection group in the navigator. */
  pinned?: boolean;
}

export interface ServerInfo {
  serverVersion: string;
  serverType: string;
}

/** Capabilities reported by the concrete driver for a live database session. */
export interface DriverCapabilities {
  supportsCancelQuery: boolean;
  /** True only when the driver implements opaque execution-handle cancel. */
  supportsQueryExecutionCancel: boolean;
  supportsExplain: boolean;
  supportsStreamingResults: boolean;
}

export type TableType = 'table' | 'view' | 'materializedView' | 'systemTable';

export interface DatabaseObject {
  kind: DatabaseObjectKind;
  schema?: string | null;
  name: string;
}

export interface PrivilegeGrant {
  grantee: string;
  objectSchema?: string | null;
  objectName: string;
  privilege: string;
}

export interface TableInfo {
  name: string;
  /** `driver-api` declares both as `Option<_>` without `skip_serializing_if`, so
   *  serde puts `null` on the wire — not `undefined`. `?:` alone is a lie that
   *  silently rejects the backend's own payload. */
  schema?: string | null;
  tableType: TableType;
  rowCount?: number | null;
}

export interface ColumnSchema {
  name: string;
  dataType: string;
  nullable: boolean;
  defaultValue?: string | null;
  isPrimaryKey?: boolean;
  isAutoIncrement?: boolean;
  comment?: string;
}

export interface IndexInfo {
  name: string;
  columns: string[];
  isUnique: boolean;
  isPrimary: boolean;
  indexType?: string;
}

export interface ForeignKeyInfo {
  name: string;
  columns: string[];
  referencedTable: string;
  referencedColumns: string[];
  onUpdate?: string;
  onDelete?: string;
  deferrability?:
    | 'unknown'
    | 'notDeferrable'
    | 'deferrableInitiallyImmediate'
    | 'deferrableInitiallyDeferred';
}

export interface TableSchema {
  tableName: string;
  columns: ColumnSchema[];
  primaryKeys: string[];
  indexes: IndexInfo[];
  foreignKeys: ForeignKeyInfo[];
}

/**
 * The referenced side of a foreign key, as the host resolved it (DB-07).
 *
 * The host is the only authority on where the target lives: the driver's
 * `referenced_table` differs per driver (PostgreSQL prefixes the schema, MySQL
 * and SQLite do not), so the front end must never split that string itself.
 *
 * Field-for-field identical to the port's `ForeignKeyTargetRef`
 * (`packages/backend-client/src/types/tableData.ts`) — the port is the cross-shape
 * contract, this file is the host-side mirror of it.
 */
export interface ForeignKeyTargetRef {
  /** Target database. */
  database: string;
  /** Target namespace; `null` when the driver has no schema level. */
  schema: string | null;
  /** Target table. */
  table: string;
  /** The referenced columns, in the same order as the local columns. */
  columns: string[];
  /** The type names of `columns`, in the same order. */
  dataTypes: string[];
  /** The effective primary key of the target table. */
  primaryKeys: string[];
  /** The referenced columns are covered by a primary key or a unique index. */
  unique: boolean;
  /** The candidate limit that was actually applied. */
  candidateLimit: number;
}

/**
 * The result of resolving a foreign key (DB-07).
 *
 * `exists: false` means the target table is readable but the referenced row is
 * gone — a successful answer, not an error.
 */
export interface ForeignKeyTargetResolution {
  target: ForeignKeyTargetRef;
  exists: boolean;
  candidates: (Value | null)[][];
  truncated: boolean;
}

/**
 * The request of a foreign-key resolution (DB-07).
 *
 * Field-for-field identical to the port's `ResolveForeignKeyTargetsRequest`.
 */
export interface ResolveForeignKeyTargetsRequest {
  dbSessionId: string;
  database?: string | null;
  schema?: string | null;
  table: string;
  qualifiedTable?: string | null;
  /** The local columns of the foreign key, in pairing order. */
  columns: string[];
  /** Disambiguates two constraints over the same columns. */
  constraintName?: string | null;
  /** The local column values, positionally paired with `columns`. */
  values: Value[];
  limit?: number;
}


export interface ExplainPlanDetail {
  key: string;
  value: string;
}

export interface ExplainPlanNode {
  id: string;
  label: string;
  cost?: number;
  rows?: number;
  details: ExplainPlanDetail[];
  children: ExplainPlanNode[];
}

export interface ExplainResult {
  planText: string;
  planJson?: unknown;
  planTree?: ExplainPlanNode;
  totalCost?: number;
  estimatedRows?: number;
}

export interface QueryHistoryEntry {
  id: string;
  connectionId: string;
  database: string;
  /** Schema namespace when known (PG search_path is not session-tracked yet → usually absent). */
  schema?: string | null;
  sql: string;
  executedAt: string;
  executionTimeMs: number;
  rowsAffected?: number;
  success: boolean;
  errorMessage?: string;
}

export interface ContextEntry {
  name: string;
  path: string;
  isDir: boolean;
  size?: number;
  children?: ContextEntry[];
}

export type ContextKind = 'file' | 'dir' | 'table';

export interface ContextItem {
  kind: ContextKind;
  id: string;
  name: string;
  path?: string;
  database?: string;
}
