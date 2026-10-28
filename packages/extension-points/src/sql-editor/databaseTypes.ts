/**
 * Database types and driver metadata registry for SQL editor.
 */
import type { FunctionEntry } from './functionTypes';
import type {
  ResolvedSqlParameterPolicy,
  SqlDialectProfile,
  SqlDialectStrategy,
  SqlParameterPolicy,
  SqlParameterStrategy,
} from './sqlDialectTypes';

export type DatabaseType = string;

export interface ColumnSchema {
  name: string;
  dataType: string;
  nullable: boolean;
  defaultValue?: string | null;
  isPrimaryKey?: boolean;
  isAutoIncrement?: boolean;
  comment?: string | null;
  extra?: string | null;
}

export interface IndexInfo {
  name: string;
  columns: string[];
  isUnique: boolean;
  isPrimary: boolean;
  unique?: boolean;
  primary?: boolean;
  indexType?: string;
}

export interface ForeignKeyInfo {
  name: string;
  columns: string[];
  referencedTable: string;
  referencedColumns: string[];
  onUpdate?: string;
  onDelete?: string;
}

export interface TableSchema {
  tableName: string;
  columns: ColumnSchema[];
  primaryKeys: string[];
  indexes: IndexInfo[];
  foreignKeys: ForeignKeyInfo[];
}

export interface DatabaseTypeMetaLite {
  id?: string;
  name?: string;
  sqlDialect?: string;
  sqlFunctions?: readonly FunctionEntry[];
  sqlDialectProfile?: SqlDialectProfile;
  sqlParameterPolicy?: ResolvedSqlParameterPolicy;
  sqlParameterStrategy?: SqlParameterStrategy;
  [key: string]: unknown;
}

export const DB_REGISTRY: Record<string, DatabaseTypeMetaLite> = {};

export function registerDriverMeta(id: string, meta: DatabaseTypeMetaLite): void {
  DB_REGISTRY[id] = meta;
}

export function registerDriverMetas(metas: Record<string, DatabaseTypeMetaLite>): void {
  Object.assign(DB_REGISTRY, metas);
}

// ──────────────────────────────────────────────────────────────────────
// 连接/驱动元数据契约（自宿主 src/lib/databaseMeta.ts、src/types、
// src/lib/structureEditor、src/lib/cellTypes 上收；驱动 meta.ts 声明面）
// ──────────────────────────────────────────────────────────────────────

export type SslMode = 'disable' | 'prefer' | 'require' | 'verifyCa' | 'verifyFull';

export type DatabaseObjectKind = 'function' | 'procedure' | 'trigger' | 'sequence' | 'type';

export type CountStrategy = 'exact' | 'estimated' | 'unsupported'
;

export type FilterOperator =
  | 'eq'
  | 'ne'
  | 'gt'
  | 'lt'
  | 'gte'
  | 'lte'
  | 'like'
  | 'in'
  | 'isNull'
  | 'isNotNull'
  | 'notIn'
  | 'between'
  | 'notBetween'
  | 'contains'
  | 'notContains'
  | 'startsWith'
  | 'endsWith'
  | 'isEmpty'
  | 'isNotEmpty'
  | 'regex'
  | 'notRegex'
  | 'jsonContains';

/**
 * The operators that shipped before the extension.
 *
 * This list is the regression baseline for "the existing operators keep their
 * relative order and their SQL"; nothing in the runtime returns it.
 */

export interface StructureEditorUiConfig {
  enabled?: boolean;
  columnTypes: { value: string; label: string }[];
  defaultColumnType: string;
  fields: {
    comment?: boolean;
    charset?: boolean;
    collation?: boolean;
    unsigned?: boolean;
    length?: boolean;
  };
  indexMethods: string[];
}

export type CellType =
  | 'text' // character / text / unknown text family
  | 'number' // integer, fixed point, float
  | 'bool' // boolean
  | 'date' // date only
  | 'time' // time only
  | 'datetime' // date + time, with or without a zone
  | 'json' // structured value (JSON / JSONB / array / VARIANT)
  | 'bytes' // binary
  | 'enum' // enum / low-cardinality column
  | 'uuid' // UUID / uniqueidentifier
  | 'unknown'
;

export type CellTypeMatch =
  | 'exact' // the whole name equals the pattern
  | 'prefix' // the name starts with the pattern
  | 'suffix' // the name ends with the pattern
  | 'token' // some token equals the pattern
  | 'tokenPrefix' // some token starts with the pattern
  | 'substring'
;

export interface CellTypeRule {
  readonly match: CellTypeMatch;
  /** A lower-case literal. Never a regular expression. */
  readonly pattern: string;
  readonly cellType: CellType;
}

export interface CellTypeHints {
  /**
   * Driver rules, evaluated **before** the shared table; the first hit wins.
   * This is where a dialect trap is corrected (`timestamp` in SQL Server is a
   * row version, not a time).
   */
  readonly overrides?: readonly CellTypeRule[];
  /**
   * Type names this driver uses for enumerations (token / token-prefix
   * match) → `CellType` `enum`. PostgreSQL reports every enum column as
   * `USER-DEFINED`, so only the driver can know what that means.
   */
  readonly enumTypes?: readonly string[];
  /**
   * Type names whose values carry a zone (`timestamptz`, `datetimeoffset`).
   * They decide whether the date editor may use the picker: the picker
   * cannot carry an offset, so a zoned column is edited as plain text.
   */
  readonly zonedTypes?: readonly string[];
  /**
   * Type names that cannot be written in place in this database (SQL Server's
   * `rowversion`) → read-only reason `typeNotEditable`.
   */
  readonly readOnlyTypes?: readonly string[];
}

export type ConnectionMode = 'server' | 'file' | 'url';

/**
 * Which KV workspace surfaces a driver is able to fill.
 *
 * These are *capabilities*, not driver ids: the host opens a slot only when the
 * flag is set, and it still needs a component registered for that slot
 * (`scripts/resolve-drivers.mjs` → `getDriverKvSlot`) before it renders one.
 * Every flag is optional and defaults to `false`, so a driver that does not
 * declare `kvWorkspace` keeps today's behaviour bit for bit — which is exactly
 * the backward-compatibility contract of this field.
 *
 * Add the declaration in the driver's own meta file (e.g. redis:
 * `packages/drivers/redis/ui/shared/meta.ts`), never in the host.
 */
export interface KvWorkspaceCapabilities {
  /** Fill the 48px content-toolbar left cluster (db picker, key counts, …). */
  contextBar?: boolean;
  /** Replace the bottom status bar's centre cluster with KV facts. */
  statusBar?: boolean;
  /** Render a key-props sidebar in the detail drawer instead of the row table. */
  keyPropsSidebar?: boolean;
  /** Take over the connected landing screen (no panel open) with a KV overview. */
  home?: boolean;
}

/**
 * What the data grid may do on this database.
 *
 * Same shape as {@link KvWorkspaceCapabilities}: every flag is optional and
 * defaults to the behaviour of the host *before* the flag existed, so a driver
 * that declares nothing keeps today's grid bit for bit.
 *
 * The back end stays the authority (00-contracts §7): these flags let the front
 * end hide an entry the database cannot serve, while a refusal that still
 * happens is reported by the back end (`grid.filter.unsupportedOperator`).
 */
export interface DataGridCapabilities {
  /**
   * Table level writes. **Defaults to true** — the grid is editable unless a
   * driver says otherwise. A default of `false` would turn every driver that
   * never declared the flag read-only overnight, which is a regression and not
   * a conservative choice.
   */
  editable?: boolean;
  /** The database can fill the new row's primary key back after an insert. */
  insertReturning?: boolean;
  /**
   * The filter operators this database serves. Defaults to the shared set
   * (the existing ten plus the nine ANSI extensions); the three dialect
   * operators (`regex` / `notRegex` / `jsonContains`) only appear when a
   * driver names them here.
   */
  filterOperators?: FilterOperator[];
  /**
   * `LIKE` is case-insensitive under this database's default collation.
   *
   * Copy only — it never changes the SQL. Defaults to `false`, and `false`
   * means "say nothing": a driver-level default says nothing about a column
   * level `COLLATE`.
   */
  likeCaseInsensitive?: boolean;
  /**
   * The row-count strategy this database serves (00-contracts §7).
   *
   * Unset means `'exact'`, which is the branch every read took before the
   * three states existed: a driver that says nothing still counts exactly.
   *
   * The declaration is a **hint, not the authority**: the back end still
   * refuses to count against the driver's own `count_strategy()`, and the
   * response carries the strategy that was actually used
   * (`TableDataResult.totalRowsKind`). The grid renders that answer, never
   * this one.
   */
  countStrategy?: CountStrategy;
}

export interface DatabaseTypeMeta {
  /** Human-readable name, e.g. "PostgreSQL" */
  label: string;
  /** 2-char abbreviation for icons, e.g. "Pg" */
  shortLabel: string;
  /** Tailwind bg class for the icon badge */
  iconBg: string;
  /** Tailwind text-color class for compact icon (backup window etc.) */
  iconColor: string;
  /** Default port (0 = not applicable) */
  defaultPort: number;
  /** Default host */
  defaultHost: string;
  /** Default username (empty string = no username field in form) */
  defaultUser: string;
  /** Force username into the connection draft even when `defaultUser` is empty */
  requiresUsername?: boolean;
  /** Persist `schema` on the connection config (e.g. catalog engines) */
  connectionIncludesSchema?: boolean;
  /** Identifier quoting character (`"` for SQL standard, `` ` `` for MySQL) */
  quoteChar: string;
  /** Connection mode: server (host:port), file (path), url (connection string) */
  connectionMode: ConnectionMode;
  /** Whether SSH tunneling is supported */
  supportsSSH: boolean;
  /** Whether SSL/TLS configuration is supported */
  supportsSSL: boolean;
  /** Default SSL/TLS mode when switching to this database type */
  defaultSslMode?: SslMode;
  /** Whether database backup is supported */
  supportsBackup: boolean;
  /** Whether this type supports schemas (tables, queries, etc.) */
  supportsTables: boolean;
  /** Key-value stores (e.g. Redis) — no SQL tables in the traditional sense */
  isKeyValue: boolean;
  /**
   * Popularity rank for the new-connection sidebar ordering (lower = shown first).
   * Omitted types sort after all ranked types, then alphabetically by id.
   * The host derives `DB_TYPE_POPULARITY_ORDER` from this field instead of
   * hard-coding a list, so new drivers get correct placement by setting this value.
   */
  popularityOrder?: number;
  /** Whether SQL is the primary query language */
  supportsSQL: boolean;
  /** Category aligned with backend `DriverCategory` / connection info */
  category: 'sql' | 'kv' | 'document';
  /** Which connection view component to render: sql (table browser), keyvalue (Redis), document (future MongoDB) */
  connectionView: 'sql' | 'keyvalue' | 'document';
  /** SQL dialect family for DDL/index queries; undefined for non-SQL types */
  sqlDialect?: string;
  /**
   * Sync/transfer pairing category. Defaults from {@link category}; set to
   * `'other'` for proxy/exploration drivers (kiwi, superset).
   */
  syncCategory?: 'sql' | 'document' | 'kv' | 'other';
  /**
   * Sync dialect family for Transfer pairing. Defaults to {@link sqlDialect}
   * or the registry db type id when omitted.
   */
  syncFamily?: string;
  /** How the "database" field behaves in the connection form.
   *  - name: logical database name (MySQL/PG)
   *  - path: file path (SQLite)
   *  - index: numeric index (Redis)
   *  - domain: instance host/domain for a proxy — NOT a logical DB for sidebar lock
   */
  databaseFieldType: 'name' | 'path' | 'index' | 'domain';
  /** Default database / index / path when switching type in the connection form */
  defaultDatabase?: string;
  /** Default opaque connection options (e.g. Redis topology) */
  defaultOptions?: Record<string, unknown>;
  /** Inclusive max when `databaseFieldType === 'index'` (default 15) */
  maxDatabaseIndex?: number;
  /** Driver capability: schema tree can browse multiple databases. Session UI uses hasMultiDatabase && databases.length > 1. */
  hasMultiDatabase?: boolean;
  /**
   * Driver command (executed via `execute_driver_command`) returning per-database
   * key counts for KV drivers, so the navigator can render `db0 (52)`. Unset = no counts.
   * Expected result: an array of `{ db: number; keys: number }`.
   */
  dbCountsCommand?: string;
  /**
   * KV workspace slots this driver can fill (context bar / status bar /
   * key-props sidebar / connection home). Omit entirely — or leave a flag out —
   * to keep the host's SQL-oriented default rendering for that surface; the
   * host checks this capability before it even looks for a registered component.
   * See {@link KvWorkspaceCapabilities}.
   */
  kvWorkspace?: KvWorkspaceCapabilities;
  /**
   * Data-grid capabilities of this driver: write access, the filter operators
   * it serves, whether `LIKE` ignores case. Omit entirely to keep the host's
   * default grid behaviour. See {@link DataGridCapabilities}.
   */
  dataGrid?: DataGridCapabilities;
  /**
   * The driver's own column-type rules (DB-05).
   *
   * The shared table in `src/lib/cellTypes.ts` carries the common SQL type
   * names; the traps that belong to one dialect are declared **here**, by
   * the driver, in its own `ui/meta.ts`. The host never branches on a driver
   * id for a type.
   *
   * Omit it and the shared table alone decides — which is exactly the
   * behaviour before this field existed (00-contracts R4).
   */
  cellTypes?: CellTypeHints;
  /** Default page size for table data; unset uses per-table or global default */
  defaultPageSize?: number;
  /** Connection form variant — extensions can provide custom form identifiers */
  connectionForm: string;
  /**
   * URL schemes (without `://`) this driver claims for new-connection clipboard auto-detect.
   * Custom parsers registered in `generated.ts` run first and can override this.
   */
  clipboardSchemes?: string[];
  /** Schema tree mode: 'standard' (default), 'multiDatabase', or 'custom' (wapp-provided tree) */
  schemaTreeMode?: 'standard' | 'multiDatabase' | 'custom';
  /** Whether this driver is read-only (no DDL, no create/alter table, no import) */
  readOnly?: boolean;
  /** Whether this driver supports EXPLAIN query plan analysis (opt-in; omit = unsupported). */
  supportsExplain?: boolean;
  /** Whether this driver supports ER diagram (requires FK metadata) */
  supportsErDiagram?: boolean;
  /**
   * On-demand SQL namespace completion strategy (host is wapp-agnostic).
   * - `default-sql`: database → tables (MySQL/MariaDB/…)
   * - `postgresql`: database → schema → table (or schema → table when single-db)
   * - `path-hierarchy`: slash-path levels via `list_catalog` + optional name→id aliases
   *   (extensions that use catalog/schema navigation rows with schema CATALOG|SCHEMA)
   */
  namespaceEnsure?: 'default-sql' | 'postgresql' | 'path-hierarchy';
  /**
   * Default schema name to sort first in schema tree (e.g. 'public' for PostgreSQL, 'dbo' for SQL Server).
   * When set, this schema is always displayed first in the schema list.
   */
  defaultSchema?: string;
  /**
   * When true, host `setLoadedTables` does not merge into `namespaceTree`
   * (wapp owns hierarchy via SDK `syncSchemaNamespace` / aliases).
   */
  namespaceOwnedByPlugin?: boolean;
  /**
   * Database object kinds this driver supports (function, procedure, trigger, sequence, type).
   * When set, schema tree only shows categories matching these kinds.
   * When omitted, no database-object categories are shown (tables/views are always shown if supportsTables is true).
   */
  supportedObjectKinds?: DatabaseObjectKind[];
  /** Table structure editor UI config; omit or `enabled: false` for non-SQL / opt-out drivers. */
  structureEditor?: StructureEditorUiConfig;
  /**
   * Data-export capability of this driver, from weakest to strongest:
   * - `'none'`        — the driver must not export any data (security/compliance).
   * - `'loaded_only'` — can only export rows already loaded/queried into the UI;
   *                     cannot reliably pull an entire table (e.g. drivers that cap
   *                     query results and lack paging).
   * - `'full_table'`  (default) — can stream/retrieve the whole table for export.
   * Omit the field for drivers that support full-table export.
   */
  exportScope?: 'none' | 'loaded_only' | 'full_table';
  /** Whether this driver supports CREATE DATABASE via Driver Command. */
  supportsCreateDatabase?: boolean;
  /** Whether this driver supports CREATE SCHEMA (e.g. PostgreSQL). */
  supportsCreateSchema?: boolean;
  /** Whether this driver supports CREATE USER via Driver Command. */
  supportsCreateUser?: boolean;
  /** Dialect-specific SQL functions contributed by this driver for completion, signature help, and inlay hints */
  sqlFunctions?: readonly FunctionEntry[];
  /** SQL dialect strategy for DDL, Index, and Table template generation. */
  sqlDialectStrategy?: SqlDialectStrategy;
  /** Semantic editor profile for identifier quoting, casing, alias visibility, and parameter policies. */
  sqlDialectProfile?: SqlDialectProfile;
  /** Native SQL bind syntax for drivers without a full semantic dialect profile. */
  sqlParameterPolicy?: SqlParameterPolicy;
  /** Driver-owned filtering for bind syntax that is ambiguous in this dialect. */
  sqlParameterStrategy?: SqlParameterStrategy;
  /**
   * Custom SQL literal value escaper for "Copy as SQL" / export.
   *
   * The host provides a default PostgreSQL-style escaper (single-quote
   * doubling). Drivers whose dialect requires additional escaping (e.g.
   * MySQL's backslash-escape semantics) override this function.
   *
   * Signature: receives a raw JS value, returns a SQL-literal string
   * (including surrounding quotes for strings, or `NULL` / bare number).
   */
  escapeSqlValue?: (value: unknown) => string;
}

export interface KvWorkspaceCapabilities {
  /** Fill the 48px content-toolbar left cluster (db picker, key counts, …). */
  contextBar?: boolean;
  /** Replace the bottom status bar's centre cluster with KV facts. */
  statusBar?: boolean;
  /** Render a key-props sidebar in the detail drawer instead of the row table. */
  keyPropsSidebar?: boolean;
  /** Take over the connected landing screen (no panel open) with a KV overview. */
  home?: boolean;
}

export interface DataGridCapabilities {
  /**
   * Table level writes. **Defaults to true** — the grid is editable unless a
   * driver says otherwise. A default of `false` would turn every driver that
   * never declared the flag read-only overnight, which is a regression and not
   * a conservative choice.
   */
  editable?: boolean;
  /** The database can fill the new row's primary key back after an insert. */
  insertReturning?: boolean;
  /**
   * The filter operators this database serves. Defaults to the shared set
   * (the existing ten plus the nine ANSI extensions); the three dialect
   * operators (`regex` / `notRegex` / `jsonContains`) only appear when a
   * driver names them here.
   */
  filterOperators?: FilterOperator[];
  /**
   * `LIKE` is case-insensitive under this database's default collation.
   *
   * Copy only — it never changes the SQL. Defaults to `false`, and `false`
   * means "say nothing": a driver-level default says nothing about a column
   * level `COLLATE`.
   */
  likeCaseInsensitive?: boolean;
  /**
   * The row-count strategy this database serves (00-contracts §7).
   *
   * Unset means `'exact'`, which is the branch every read took before the
   * three states existed: a driver that says nothing still counts exactly.
   *
   * The declaration is a **hint, not the authority**: the back end still
   * refuses to count against the driver's own `count_strategy()`, and the
   * response carries the strategy that was actually used
   * (`TableDataResult.totalRowsKind`). The grid renders that answer, never
   * this one.
   */
  countStrategy?: CountStrategy;
}

export interface DatabaseTypeMeta {
  /** Human-readable name, e.g. "PostgreSQL" */
  label: string;
  /** 2-char abbreviation for icons, e.g. "Pg" */
  shortLabel: string;
  /** Tailwind bg class for the icon badge */
  iconBg: string;
  /** Tailwind text-color class for compact icon (backup window etc.) */
  iconColor: string;
  /** Default port (0 = not applicable) */
  defaultPort: number;
  /** Default host */
  defaultHost: string;
  /** Default username (empty string = no username field in form) */
  defaultUser: string;
  /** Force username into the connection draft even when `defaultUser` is empty */
  requiresUsername?: boolean;
  /** Persist `schema` on the connection config (e.g. catalog engines) */
  connectionIncludesSchema?: boolean;
  /** Identifier quoting character (`"` for SQL standard, `` ` `` for MySQL) */
  quoteChar: string;
  /** Connection mode: server (host:port), file (path), url (connection string) */
  connectionMode: ConnectionMode;
  /** Whether SSH tunneling is supported */
  supportsSSH: boolean;
  /** Whether SSL/TLS configuration is supported */
  supportsSSL: boolean;
  /** Default SSL/TLS mode when switching to this database type */
  defaultSslMode?: SslMode;
  /** Whether database backup is supported */
  supportsBackup: boolean;
  /** Whether this type supports schemas (tables, queries, etc.) */
  supportsTables: boolean;
  /** Key-value stores (e.g. Redis) — no SQL tables in the traditional sense */
  isKeyValue: boolean;
  /**
   * Popularity rank for the new-connection sidebar ordering (lower = shown first).
   * Omitted types sort after all ranked types, then alphabetically by id.
   * The host derives `DB_TYPE_POPULARITY_ORDER` from this field instead of
   * hard-coding a list, so new drivers get correct placement by setting this value.
   */
  popularityOrder?: number;
  /** Whether SQL is the primary query language */
  supportsSQL: boolean;
  /** Category aligned with backend `DriverCategory` / connection info */
  category: 'sql' | 'kv' | 'document';
  /** Which connection view component to render: sql (table browser), keyvalue (Redis), document (future MongoDB) */
  connectionView: 'sql' | 'keyvalue' | 'document';
  /** SQL dialect family for DDL/index queries; undefined for non-SQL types */
  sqlDialect?: string;
  /**
   * Sync/transfer pairing category. Defaults from {@link category}; set to
   * `'other'` for proxy/exploration drivers (kiwi, superset).
   */
  syncCategory?: 'sql' | 'document' | 'kv' | 'other';
  /**
   * Sync dialect family for Transfer pairing. Defaults to {@link sqlDialect}
   * or the registry db type id when omitted.
   */
  syncFamily?: string;
  /** How the "database" field behaves in the connection form.
   *  - name: logical database name (MySQL/PG)
   *  - path: file path (SQLite)
   *  - index: numeric index (Redis)
   *  - domain: instance host/domain for a proxy — NOT a logical DB for sidebar lock
   */
  databaseFieldType: 'name' | 'path' | 'index' | 'domain';
  /** Default database / index / path when switching type in the connection form */
  defaultDatabase?: string;
  /** Default opaque connection options (e.g. Redis topology) */
  defaultOptions?: Record<string, unknown>;
  /** Inclusive max when `databaseFieldType === 'index'` (default 15) */
  maxDatabaseIndex?: number;
  /** Driver capability: schema tree can browse multiple databases. Session UI uses hasMultiDatabase && databases.length > 1. */
  hasMultiDatabase?: boolean;
  /**
   * Driver command (executed via `execute_driver_command`) returning per-database
   * key counts for KV drivers, so the navigator can render `db0 (52)`. Unset = no counts.
   * Expected result: an array of `{ db: number; keys: number }`.
   */
  dbCountsCommand?: string;
  /**
   * KV workspace slots this driver can fill (context bar / status bar /
   * key-props sidebar / connection home). Omit entirely — or leave a flag out —
   * to keep the host's SQL-oriented default rendering for that surface; the
   * host checks this capability before it even looks for a registered component.
   * See {@link KvWorkspaceCapabilities}.
   */
  kvWorkspace?: KvWorkspaceCapabilities;
  /**
   * Data-grid capabilities of this driver: write access, the filter operators
   * it serves, whether `LIKE` ignores case. Omit entirely to keep the host's
   * default grid behaviour. See {@link DataGridCapabilities}.
   */
  dataGrid?: DataGridCapabilities;
  /**
   * The driver's own column-type rules (DB-05).
   *
   * The shared table in `src/lib/cellTypes.ts` carries the common SQL type
   * names; the traps that belong to one dialect are declared **here**, by
   * the driver, in its own `ui/meta.ts`. The host never branches on a driver
   * id for a type.
   *
   * Omit it and the shared table alone decides — which is exactly the
   * behaviour before this field existed (00-contracts R4).
   */
  cellTypes?: CellTypeHints;
  /** Default page size for table data; unset uses per-table or global default */
  defaultPageSize?: number;
  /** Connection form variant — extensions can provide custom form identifiers */
  connectionForm: string;
  /**
   * URL schemes (without `://`) this driver claims for new-connection clipboard auto-detect.
   * Custom parsers registered in `generated.ts` run first and can override this.
   */
  clipboardSchemes?: string[];
  /** Schema tree mode: 'standard' (default), 'multiDatabase', or 'custom' (wapp-provided tree) */
  schemaTreeMode?: 'standard' | 'multiDatabase' | 'custom';
  /** Whether this driver is read-only (no DDL, no create/alter table, no import) */
  readOnly?: boolean;
  /** Whether this driver supports EXPLAIN query plan analysis (opt-in; omit = unsupported). */
  supportsExplain?: boolean;
  /** Whether this driver supports ER diagram (requires FK metadata) */
  supportsErDiagram?: boolean;
  /**
   * On-demand SQL namespace completion strategy (host is wapp-agnostic).
   * - `default-sql`: database → tables (MySQL/MariaDB/…)
   * - `postgresql`: database → schema → table (or schema → table when single-db)
   * - `path-hierarchy`: slash-path levels via `list_catalog` + optional name→id aliases
   *   (extensions that use catalog/schema navigation rows with schema CATALOG|SCHEMA)
   */
  namespaceEnsure?: 'default-sql' | 'postgresql' | 'path-hierarchy';
  /**
   * Default schema name to sort first in schema tree (e.g. 'public' for PostgreSQL, 'dbo' for SQL Server).
   * When set, this schema is always displayed first in the schema list.
   */
  defaultSchema?: string;
  /**
   * When true, host `setLoadedTables` does not merge into `namespaceTree`
   * (wapp owns hierarchy via SDK `syncSchemaNamespace` / aliases).
   */
  namespaceOwnedByPlugin?: boolean;
  /**
   * Database object kinds this driver supports (function, procedure, trigger, sequence, type).
   * When set, schema tree only shows categories matching these kinds.
   * When omitted, no database-object categories are shown (tables/views are always shown if supportsTables is true).
   */
  supportedObjectKinds?: DatabaseObjectKind[];
  /** Table structure editor UI config; omit or `enabled: false` for non-SQL / opt-out drivers. */
  structureEditor?: StructureEditorUiConfig;
  /**
   * Data-export capability of this driver, from weakest to strongest:
   * - `'none'`        — the driver must not export any data (security/compliance).
   * - `'loaded_only'` — can only export rows already loaded/queried into the UI;
   *                     cannot reliably pull an entire table (e.g. drivers that cap
   *                     query results and lack paging).
   * - `'full_table'`  (default) — can stream/retrieve the whole table for export.
   * Omit the field for drivers that support full-table export.
   */
  exportScope?: 'none' | 'loaded_only' | 'full_table';
  /** Whether this driver supports CREATE DATABASE via Driver Command. */
  supportsCreateDatabase?: boolean;
  /** Whether this driver supports CREATE SCHEMA (e.g. PostgreSQL). */
  supportsCreateSchema?: boolean;
  /** Whether this driver supports CREATE USER via Driver Command. */
  supportsCreateUser?: boolean;
  /** Dialect-specific SQL functions contributed by this driver for completion, signature help, and inlay hints */
  sqlFunctions?: readonly FunctionEntry[];
  /** SQL dialect strategy for DDL, Index, and Table template generation. */
  sqlDialectStrategy?: SqlDialectStrategy;
  /** Semantic editor profile for identifier quoting, casing, alias visibility, and parameter policies. */
  sqlDialectProfile?: SqlDialectProfile;
  /** Native SQL bind syntax for drivers without a full semantic dialect profile. */
  sqlParameterPolicy?: SqlParameterPolicy;
  /** Driver-owned filtering for bind syntax that is ambiguous in this dialect. */
  sqlParameterStrategy?: SqlParameterStrategy;
  /**
   * Custom SQL literal value escaper for "Copy as SQL" / export.
   *
   * The host provides a default PostgreSQL-style escaper (single-quote
   * doubling). Drivers whose dialect requires additional escaping (e.g.
   * MySQL's backslash-escape semantics) override this function.
   *
   * Signature: receives a raw JS value, returns a SQL-literal string
   * (including surrounding quotes for strings, or `NULL` / bare number).
   */
  escapeSqlValue?: (value: unknown) => string;
}
