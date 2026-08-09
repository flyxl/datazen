/**
 * Unified database type registry.
 *
 * Driver metadata is injected at build/dev time into src/plugins/generated.ts
 * (path + git drivers selected via --drivers / DATAZEN_DRIVERS).
 */

import type { DatabaseType } from '../types';
import { DRIVER_DB_ENTRIES, DRIVER_ICON_ENTRIES, DRIVER_ICON_PARENTS } from '../plugins/generated';
import type { IconSourceMap } from './iconResolver';

export type { ConnectionMode, DatabaseTypeMeta } from './databaseMeta';
import type { DatabaseTypeMeta } from './databaseMeta';

export const DB_REGISTRY: Record<DatabaseType, DatabaseTypeMeta> = {
  ...DRIVER_DB_ENTRIES,
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

/** Built-in driver SVG URLs keyed by semantic icon id (`db.<type>`). */
export function getDriverIconMap(): IconSourceMap {
  return { ...DRIVER_ICON_ENTRIES };
}

/** Parent dbType for protocol-reuse badges that lack their own SVG. */
export function getDriverIconParents(): Record<string, string> {
  return { ...DRIVER_ICON_PARENTS };
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
