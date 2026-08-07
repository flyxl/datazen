/**
 * Unified database type registry.
 *
 * Built-in types are defined here. Plugin types are merged in from
 * src/plugins/generated.ts at import time.
 */

import type { DatabaseType } from '../types';
import { PLUGIN_DB_ENTRIES } from '../plugins/generated';

export type { ConnectionMode, DatabaseTypeMeta } from './databaseMeta';
import type { DatabaseTypeMeta } from './databaseMeta';

const BUILTIN_DB_REGISTRY: Record<string, DatabaseTypeMeta> = {
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
    supportsSSL: true,
    supportsBackup: true,
    supportsTables: true,
    isKeyValue: false,
    supportsSQL: true,
    category: 'sql',
    connectionView: 'sql',
    sqlDialect: 'postgresql',
    databaseFieldType: 'name',
    connectionForm: 'standard',
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
    supportsSSL: true,
    supportsBackup: true,
    supportsTables: true,
    isKeyValue: false,
    supportsSQL: true,
    category: 'sql',
    connectionView: 'sql',
    sqlDialect: 'mysql',
    databaseFieldType: 'name',
    connectionForm: 'standard',
    hasMultiDatabase: true,
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
    supportsSSL: true,
    supportsBackup: true,
    supportsTables: true,
    isKeyValue: false,
    supportsSQL: true,
    category: 'sql',
    connectionView: 'sql',
    sqlDialect: 'mysql',
    databaseFieldType: 'name',
    connectionForm: 'standard',
    hasMultiDatabase: true,
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
    supportsSSL: false,
    supportsBackup: false,
    supportsTables: true,
    isKeyValue: false,
    supportsSQL: true,
    category: 'sql',
    connectionView: 'sql',
    sqlDialect: 'sqlite',
    databaseFieldType: 'path',
    connectionForm: 'file',
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
    supportsSSL: false,
    supportsBackup: false,
    supportsTables: false,
    isKeyValue: true,
    supportsSQL: false,
    category: 'kv',
    connectionView: 'keyvalue',
    databaseFieldType: 'index',
    connectionForm: 'index',
  },
};

export const DB_REGISTRY: Record<DatabaseType, DatabaseTypeMeta> = {
  ...BUILTIN_DB_REGISTRY,
  ...PLUGIN_DB_ENTRIES,
} as Record<DatabaseType, DatabaseTypeMeta>;

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

/** Redis logical DB index (0–15); invalid input becomes `"0"`. */
export function normalizeRedisDatabaseField(s: string): string {
  const u = s.trim();
  if (u === '' || !/^\d+$/.test(u)) return '0';
  return String(Math.min(15, Math.max(0, parseInt(u, 10))));
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
  if (meta?.connectionMode === 'url') {
    return conn.host ?? meta.label;
  }
  const hasSSH = conn.sshTunnel?.enabled === true;
  if (hasSSH) {
    return `${conn.sshTunnel!.host} → ${conn.host ?? ''} : ${conn.database ?? ''}`;
  }
  return `${conn.host ?? ''} : ${conn.database ?? ''}`;
}
