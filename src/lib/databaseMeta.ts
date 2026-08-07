/**
 * Database type metadata interface.
 * Extracted to avoid circular deps between types/index.ts ↔ plugins/generated.ts.
 */

export type ConnectionMode = 'server' | 'file' | 'url';

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
  /** Identifier quoting character (`"` for SQL standard, `` ` `` for MySQL) */
  quoteChar: string;
  /** Connection mode: server (host:port), file (path), url (connection string) */
  connectionMode: ConnectionMode;
  /** Whether SSH tunneling is supported */
  supportsSSH: boolean;
  /** Whether SSL/TLS configuration is supported */
  supportsSSL: boolean;
  /** Whether database backup is supported */
  supportsBackup: boolean;
  /** Whether this type supports schemas (tables, queries, etc.) */
  supportsTables: boolean;
  /** Key-value stores (e.g. Redis) — no SQL tables in the traditional sense */
  isKeyValue: boolean;
  /** Whether SQL is the primary query language */
  supportsSQL: boolean;
  /** Category aligned with backend `DriverCategory` / connection info */
  category: 'sql' | 'kv' | 'document';
  /** Which connection view component to render: sql (table browser), keyvalue (Redis), document (future MongoDB) */
  connectionView: 'sql' | 'keyvalue' | 'document';
  /** SQL dialect family for DDL/index queries; undefined for non-SQL types */
  sqlDialect?: string;
  /** How the "database" field behaves in the connection form */
  databaseFieldType: 'name' | 'path' | 'index';
  /** Driver capability: schema tree can browse multiple databases (MySQL/MariaDB/PostgreSQL/Kiwi). Session UI uses hasMultiDatabase && databases.length > 1. */
  hasMultiDatabase?: boolean;
  /** Default page size for table data; unset uses per-table or global default */
  defaultPageSize?: number;
  /** Connection form variant — plugins can provide custom form identifiers */
  connectionForm: string;
  /** Schema tree mode: 'standard' (default), 'multiDatabase', or 'custom' (plugin-provided tree) */
  schemaTreeMode?: 'standard' | 'multiDatabase' | 'custom';
  /** Whether this driver is read-only (no DDL, no create/alter table, no import) */
  readOnly?: boolean;
  /** Whether this driver supports EXPLAIN query plan analysis (opt-in; omit = unsupported). */
  supportsExplain?: boolean;
  /** Whether this driver supports ER diagram (requires FK metadata) */
  supportsErDiagram?: boolean;
}
