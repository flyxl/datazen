import type { DatabaseType } from '../types';
import type { ConnectionFormState } from '../components/connection/useConnectionForm';
import { DB_REGISTRY, sortDbTypesByPopularity } from './databaseTypes';
import type { DatabaseTypeMeta } from './databaseMeta';
import type {
  ConnectionClipboardFill,
  MatchedConnectionClipboard,
} from './connectionClipboardTypes';
import { getPluginClipboardParsers } from '../plugins/generated';

export type {
  ConnectionClipboardFill,
  ConnectionClipboardParser,
  MatchedConnectionClipboard,
} from './connectionClipboardTypes';

export function matchConnectionClipboard(
  raw: string,
  availableTypes?: readonly string[],
): MatchedConnectionClipboard | null {
  const text = stripEnvPrefix(normalizeClipboard(raw));
  if (!text) return null;

  const allowed = new Set(
    availableTypes && availableTypes.length > 0
      ? availableTypes
      : Object.keys(DB_REGISTRY),
  );

  for (const entry of getPluginClipboardParsers()) {
    if (!allowed.has(entry.dbType)) continue;
    const fill = entry.parse(text);
    if (fill) return { databaseType: entry.dbType, fill };
  }

  const scheme = extractUrlScheme(text);
  if (scheme) {
    const matches = Object.entries(DB_REGISTRY).filter(
      ([id, meta]) =>
        allowed.has(id) &&
        (meta.clipboardSchemes ?? []).some((item) => item.toLowerCase() === scheme),
    );
    if (matches.length > 0) {
      const picked = pickPreferredType(matches.map(([id]) => id));
      const meta = DB_REGISTRY[picked as DatabaseType];
      const fill = parseGenericConnectionUrl(text, meta);
      if (fill) return { databaseType: picked, fill };
    }
    return null;
  }

  const hostPort = parseBareHostPort(text);
  if (!hostPort) return null;
  const portMatches = Object.entries(DB_REGISTRY).filter(
    ([id, meta]) => allowed.has(id) && meta.defaultPort === hostPort.port,
  );
  if (portMatches.length !== 1) return null;
  return {
    databaseType: portMatches[0][0],
    fill: {
      host: hostPort.host,
      port: String(hostPort.port),
      name: `${hostPort.host}:${hostPort.port}`,
    },
  };
}

export function applyMatchedClipboard(
  form: ConnectionFormState,
  matched: MatchedConnectionClipboard,
): void {
  if (form.databaseType !== matched.databaseType) {
    form.handleDatabaseTypeChange(matched.databaseType as DatabaseType);
  }
  applyConnectionClipboardFill(form, matched.fill);
}

export function applyConnectionClipboardFill(
  form: ConnectionFormState,
  fill: ConnectionClipboardFill,
): void {
  if (fill.host) form.setHost(fill.host);
  if (fill.port) form.setPort(fill.port);
  if (fill.database !== undefined) form.setDatabase(fill.database);
  if (fill.username !== undefined) form.setUsername(fill.username);
  if (fill.password !== undefined) form.setPassword(fill.password);
  if (fill.schema !== undefined) form.setSchema(fill.schema);
  if (fill.sslMode) form.setSslMode(fill.sslMode);
  if (fill.options) {
    form.setOptions({ ...(form.options ?? {}), ...fill.options });
  }
  if (fill.expandAdvanced || fill.sslMode === 'require' || fill.sslMode === 'disable') {
    form.setShowAdvanced(true);
  }
  if (fill.name && !form.name.trim()) {
    form.setName(fill.name);
  }
}

export function parseGenericConnectionUrl(
  raw: string,
  meta: DatabaseTypeMeta,
): ConnectionClipboardFill | null {
  const text = stripEnvPrefix(normalizeClipboard(raw));
  const matched = text.match(/^(?:jdbc:)?([a-z][a-z0-9+.-]*):\/\/(.*)$/i);
  if (!matched) return null;

  let rest = matched[2];
  let query = '';
  const queryIdx = rest.indexOf('?');
  if (queryIdx >= 0) {
    query = rest.slice(queryIdx + 1);
    rest = rest.slice(0, queryIdx);
  }

  if (meta.connectionMode === 'file') {
    const filePath = decodeUri(rest.replace(/^\/\//, '/'));
    if (!filePath) return null;
    return {
      database: filePath,
      name: filePath.split(/[\\/]/).filter(Boolean).at(-1),
    };
  }

  let path = '';
  const slashIdx = rest.indexOf('/');
  if (slashIdx >= 0) {
    path = rest.slice(slashIdx + 1);
    rest = rest.slice(0, slashIdx);
  }

  let username = '';
  let password: string | undefined;
  const atIdx = rest.lastIndexOf('@');
  if (atIdx >= 0) {
    const auth = rest.slice(0, atIdx);
    rest = rest.slice(atIdx + 1);
    const colonIdx = auth.indexOf(':');
    if (colonIdx >= 0) {
      username = decodeUri(auth.slice(0, colonIdx));
      password = decodeUri(auth.slice(colonIdx + 1));
    } else {
      username = decodeUri(auth);
    }
  }

  const hostPort = parseAuthority(rest, meta.defaultPort || 0);
  if (!hostPort) return null;

  const params = new URLSearchParams(query);
  const database = path.split('/')[0] ? decodeUri(path.split('/')[0]) : undefined;
  const sslMode = readSslMode(params.get('sslmode') ?? params.get('ssl'));
  const fill: ConnectionClipboardFill = {
    host: hostPort.host,
    port: hostPort.port ? String(hostPort.port) : undefined,
    database,
    username: username || undefined,
    password,
    name: hostPort.port ? `${hostPort.host}:${hostPort.port}` : hostPort.host,
  };
  if (sslMode) {
    fill.sslMode = sslMode;
    fill.expandAdvanced = sslMode !== 'prefer';
  }
  return fill;
}

function pickPreferredType(ids: string[]): string {
  if (ids.length === 1) return ids[0];
  return sortDbTypesByPopularity(ids.map((value) => ({ value })))[0].value;
}

function extractUrlScheme(text: string): string | null {
  const jdbc = text.match(/^jdbc:([a-z0-9+.-]+):\/\//i);
  if (jdbc) return jdbc[1].toLowerCase();
  const matched = text.match(/^([a-z][a-z0-9+.-]*):\/\//i);
  return matched ? matched[1].toLowerCase() : null;
}

function parseBareHostPort(text: string): { host: string; port: number } | null {
  if (text.includes('://') || /[\s,;]/.test(text)) return null;
  return parseAuthority(text, 0);
}

function parseAuthority(
  token: string,
  defaultPort: number,
): { host: string; port: number } | null {
  const trimmed = token.trim();
  if (!trimmed) return null;
  if (trimmed.startsWith('[')) {
    const end = trimmed.indexOf(']');
    if (end < 2) return null;
    const host = trimmed.slice(0, end + 1);
    const rest = trimmed.slice(end + 1);
    if (!rest) return defaultPort > 0 ? { host, port: defaultPort } : null;
    if (!rest.startsWith(':')) return null;
    const port = Number(rest.slice(1));
    if (!Number.isInteger(port) || port <= 0 || port > 65535) return null;
    return { host, port };
  }
  const idx = trimmed.lastIndexOf(':');
  if (idx <= 0) {
    return defaultPort > 0 ? { host: trimmed, port: defaultPort } : null;
  }
  const host = trimmed.slice(0, idx);
  const port = Number(trimmed.slice(idx + 1));
  if (!host || !Number.isInteger(port) || port <= 0 || port > 65535) return null;
  return { host, port };
}

function readSslMode(value: string | null): ConnectionClipboardFill['sslMode'] | undefined {
  if (!value) return undefined;
  const normalized = value.toLowerCase();
  if (normalized === 'disable' || normalized === 'false' || normalized === '0') return 'disable';
  if (normalized === 'prefer') return 'prefer';
  if (
    normalized === 'require' ||
    normalized === 'true' ||
    normalized === '1' ||
    normalized === 'verify-ca' ||
    normalized === 'verify-full'
  ) {
    return 'require';
  }
  return undefined;
}

function normalizeClipboard(raw: string): string {
  return raw.replace(/^\uFEFF/, '').trim().replace(/^['"]+|['"]+$/g, '').trim();
}

function stripEnvPrefix(text: string): string {
  const matched = text.match(/^(?:export\s+)?(?:DATABASE_URL|DB_URL|REDIS_URL|URL)\s*=\s*(.+)$/i);
  return matched?.[1]?.trim() ?? text;
}

function decodeUri(value: string): string {
  try {
    return decodeURIComponent(value);
  } catch {
    return value;
  }
}
