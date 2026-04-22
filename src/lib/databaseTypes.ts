/**
 * Unified database type registry.
 *
 * Adding a new database type? Just add an entry here — all UI components,
 * form defaults, identifier quoting, and label/icon rendering will pick it up
 * automatically.
 */

import type { DatabaseType } from '../types';

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
  /** Default username */
  defaultUser: string;
  /** Identifier quoting character (`"` for SQL standard, `` ` `` for MySQL) */
  quoteChar: string;
  /** Connection mode: server (host:port), file (path), url (connection string) */
  connectionMode: ConnectionMode;
  /** Whether SSH tunneling is supported */
  supportsSSH: boolean;
  /** Whether database backup is supported */
  supportsBackup: boolean;
  /** Whether this type supports schemas (tables, queries, etc.) */
  supportsTables: boolean;
}

export const DB_REGISTRY: Record<DatabaseType, DatabaseTypeMeta> = {
  postgresql: {
    label: 'PostgreSQL',
    shortLabel: 'Pg',
    iconBg: 'bg-blue-600',
    iconColor: 'text-blue-400',
    defaultPort: 5432,
    defaultHost: '127.0.0.1',
    defaultUser: 'postgres',
    quoteChar: '"',
    connectionMode: 'server',
    supportsSSH: true,
    supportsBackup: true,
    supportsTables: true,
  },
  mysql: {
    label: 'MySQL',
    shortLabel: 'My',
    iconBg: 'bg-orange-500',
    iconColor: 'text-orange-400',
    defaultPort: 3306,
    defaultHost: '127.0.0.1',
    defaultUser: 'root',
    quoteChar: '`',
    connectionMode: 'server',
    supportsSSH: true,
    supportsBackup: true,
    supportsTables: true,
  },
  mariadb: {
    label: 'MariaDB',
    shortLabel: 'Ma',
    iconBg: 'bg-sky-600',
    iconColor: 'text-amber-500',
    defaultPort: 3306,
    defaultHost: '127.0.0.1',
    defaultUser: 'root',
    quoteChar: '`',
    connectionMode: 'server',
    supportsSSH: true,
    supportsBackup: true,
    supportsTables: true,
  },
  sqlite: {
    label: 'SQLite',
    shortLabel: 'Lt',
    iconBg: 'bg-emerald-600',
    iconColor: 'text-green-400',
    defaultPort: 0,
    defaultHost: '',
    defaultUser: '',
    quoteChar: '"',
    connectionMode: 'file',
    supportsSSH: false,
    supportsBackup: false,
    supportsTables: true,
  },
  redis: {
    label: 'Redis',
    shortLabel: 'Rd',
    iconBg: 'bg-red-600',
    iconColor: 'text-red-400',
    defaultPort: 6379,
    defaultHost: '127.0.0.1',
    defaultUser: '',
    quoteChar: '',
    connectionMode: 'server',
    supportsSSH: true,
    supportsBackup: false,
    supportsTables: false,
  },
};

/** All database types available for the "new connection" UI. */
export const DB_TYPE_LIST: { value: DatabaseType; label: string; color: string }[] = (
  Object.entries(DB_REGISTRY) as [DatabaseType, DatabaseTypeMeta][]
).map(([value, meta]) => ({
  value,
  label: meta.label,
  color: meta.iconBg.replace('bg-', '').split('-').length > 1
    ? `#${meta.iconBg}` // fallback; real colors below
    : meta.iconBg,
}));

/** Get the identifier quoting function for a given database type. */
export function escapeIdent(name: string, dbType?: DatabaseType): string {
  const q = dbType ? DB_REGISTRY[dbType]?.quoteChar ?? '"' : '"';
  if (q === '`') return `\`${name.replaceAll('`', '``')}\``;
  if (q === '"') return `"${name.replaceAll('"', '""')}"`;
  return name; // no quoting (e.g. Redis)
}

/** Get the display label for a database type. */
export function getDbLabel(dbType: DatabaseType): string {
  return DB_REGISTRY[dbType]?.label ?? dbType;
}

/** Get the icon info (short label + bg class) for a database type. */
export function getDbIcon(dbType: DatabaseType): { label: string; bg: string } {
  const meta = DB_REGISTRY[dbType];
  return meta
    ? { label: meta.shortLabel, bg: meta.iconBg }
    : { label: 'DB', bg: 'bg-gray-500' };
}

/** Get the icon color class for compact displays. */
export function getDbIconColor(dbType: DatabaseType): string {
  return DB_REGISTRY[dbType]?.iconColor ?? 'text-fg-muted';
}

/** Build a display address string for a connection. */
export function formatConnectionAddr(conn: {
  databaseType: DatabaseType;
  host?: string;
  port?: number;
  database?: string;
  sshTunnel?: { enabled: boolean; host: string };
}): string {
  const meta = DB_REGISTRY[conn.databaseType];
  if (meta?.connectionMode === 'file') {
    return conn.database ?? meta.label;
  }
  const hasSSH = conn.sshTunnel?.enabled === true;
  if (hasSSH) {
    return `${conn.sshTunnel!.host} → ${conn.host ?? ''} : ${conn.database ?? ''}`;
  }
  return `${conn.host ?? ''} : ${conn.database ?? ''}`;
}
