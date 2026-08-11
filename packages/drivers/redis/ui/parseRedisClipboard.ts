import type { ConnectionClipboardFill } from '@datazen/plugin-sdk';
import type { RedisConnectionOptions, RedisTopology } from './connectionOptions';
import { readRedisOptions } from './connectionOptions';
import { mergeRedisOptions } from './connectionWizardValidate';

const REDIS_SCHEME = /^(redis|rediss|redis\+tls|redis-sentinel|sentinel):\/\//i;
const OTHER_DB_SCHEME = /^(postgres|postgresql|mysql|mongodb|http|https|amqp|kafka):\/\//i;
const REDIS_URL_RE = /(?:redis|rediss|redis\+tls|redis-sentinel|sentinel):\/\/\S+/gi;
const AUTO_FILL_PORTS = new Set([6379, 6380, 26379]);

export interface ParsedRedisClipboard {
  topology: RedisTopology;
  host?: string;
  port?: string;
  database?: string;
  username?: string;
  password?: string;
  clusterNodes?: string[];
  sentinelNodes?: string[];
  sentinelMasterName?: string;
  sentinelNodePassword?: string;
  tlsEnabled?: boolean;
}

export interface RedisClipboardForm {
  name: string;
  host: string;
  port: string;
  database: string;
  username: string;
  password: string;
  options: Record<string, unknown> | undefined;
  setName: (value: string) => void;
  setHost: (value: string) => void;
  setPort: (value: string) => void;
  setDatabase: (value: string) => void;
  setUsername: (value: string) => void;
  setPassword: (value: string) => void;
  setOptions: (value: Record<string, unknown>) => void;
  setSslMode?: (value: 'disable' | 'prefer' | 'require') => void;
  setShowAdvanced?: (value: boolean | ((prev: boolean) => boolean)) => void;
}

export function clipboardHasRedisScheme(raw: string): boolean {
  return REDIS_SCHEME.test(normalizeClipboard(raw));
}

export function parseRedisClipboard(raw: string): ParsedRedisClipboard | null {
  const text = stripEnvPrefix(normalizeClipboard(raw));
  if (!text || OTHER_DB_SCHEME.test(text)) return null;

  const urls = text.match(REDIS_URL_RE)?.map((url) => url.replace(/[),.;]+$/, '')) ?? [];
  if (urls.length > 1) {
    return mergeStandaloneUrls(urls);
  }
  if (urls.length === 1) {
    return parseRedisUrl(urls[0]);
  }
  if (REDIS_SCHEME.test(text)) {
    return parseRedisUrl(text);
  }

  return parseHostPortList(text);
}

export function looksLikeRedisClipboard(raw: string, parsed: ParsedRedisClipboard): boolean {
  if (clipboardHasRedisScheme(raw) || (raw.match(REDIS_URL_RE)?.length ?? 0) > 0) {
    return true;
  }
  if (parsed.topology === 'cluster' || parsed.topology === 'sentinel') return true;
  return AUTO_FILL_PORTS.has(Number(parsed.port));
}

export function isPristineRedisForm(form: RedisClipboardForm): boolean {
  const opts = readRedisOptions(form.options);
  const topology = opts.topology ?? 'standalone';
  if (topology !== 'standalone') return false;
  if ((opts.clusterNodes?.length ?? 0) > 0) return false;
  if ((opts.sentinelNodes?.length ?? 0) > 0) return false;
  if (form.name.trim()) return false;
  if (form.password) return false;
  if (form.username.trim()) return false;
  const host = form.host.trim();
  const port = form.port.trim();
  const database = form.database.trim();
  if (host && host !== '127.0.0.1' && host !== 'localhost') return false;
  if (port && port !== '6379') return false;
  if (database && database !== '0') return false;
  return true;
}

export function parseRedisConnectionClipboard(text: string): ConnectionClipboardFill | null {
  const parsed = parseRedisClipboard(text);
  if (!parsed || !looksLikeRedisClipboard(text, parsed)) return null;
  return toConnectionClipboardFill(parsed);
}

export function applyRedisClipboardToForm(
  form: RedisClipboardForm,
  parsed: ParsedRedisClipboard,
): void {
  if (parsed.host) form.setHost(parsed.host);
  if (parsed.port) form.setPort(parsed.port);
  if (parsed.database !== undefined) form.setDatabase(parsed.database);
  if (parsed.username !== undefined) form.setUsername(parsed.username);
  if (parsed.password !== undefined) form.setPassword(parsed.password);

  const patch: Partial<RedisConnectionOptions> = {
    topology: parsed.topology,
    clusterNodes: parsed.topology === 'cluster' ? parsed.clusterNodes ?? [] : [],
    sentinelNodes: parsed.topology === 'sentinel' ? parsed.sentinelNodes ?? [] : [],
    sentinelMasterName: parsed.topology === 'sentinel' ? parsed.sentinelMasterName ?? '' : '',
  };
  if (parsed.sentinelNodePassword !== undefined) {
    patch.sentinelNodePassword = parsed.sentinelNodePassword;
  }
  if (parsed.tlsEnabled) {
    patch.tls = { enabled: true };
  }
  form.setOptions(mergeRedisOptions(form.options, patch));

  if (parsed.tlsEnabled) {
    form.setSslMode?.('require');
    form.setShowAdvanced?.(true);
  }

  if (!form.name.trim()) {
    const suggested = suggestConnectionName(parsed);
    if (suggested) form.setName(suggested);
  }
}

function normalizeClipboard(raw: string): string {
  return raw.replace(/^\uFEFF/, '').trim().replace(/^['"]+|['"]+$/g, '').trim();
}

function stripEnvPrefix(text: string): string {
  const matched = text.match(/^(?:export\s+)?(?:REDIS_URL|URL)\s*=\s*(.+)$/i);
  return matched?.[1]?.trim() ?? text;
}

function mergeStandaloneUrls(urls: string[]): ParsedRedisClipboard | null {
  const parsed = urls.map(parseRedisUrl).filter((item): item is ParsedRedisClipboard => item != null);
  if (parsed.length === 0) return null;
  if (parsed.some((item) => item.topology === 'sentinel')) {
    return parsed.find((item) => item.topology === 'sentinel') ?? null;
  }
  if (parsed.length === 1) return parsed[0];

  const nodes = uniqueNodes(
    parsed.flatMap((item) => {
      if (item.clusterNodes?.length) return item.clusterNodes;
      if (item.host && item.port) return [`${item.host}:${item.port}`];
      return [];
    }),
  );
  if (nodes.length === 0) return parsed[0];
  const first = splitHostPort(nodes[0]);
  const base = parsed[0];
  return omitUndefined({
    topology: 'cluster',
    host: first?.host ?? base.host,
    port: first?.port ?? base.port,
    username: parsed.find((item) => item.username)?.username,
    password: parsed.find((item) => item.password)?.password,
    clusterNodes: nodes,
    tlsEnabled: parsed.some((item) => item.tlsEnabled) || undefined,
  });
}

function parseRedisUrl(raw: string): ParsedRedisClipboard | null {
  const matched = raw.trim().match(/^(redis|rediss|redis\+tls|redis-sentinel|sentinel):\/\/(.*)$/i);
  if (!matched) return null;

  const scheme = matched[1].toLowerCase();
  let rest = matched[2];
  const tlsEnabled = scheme === 'rediss' || scheme === 'redis+tls';
  const sentinelScheme = scheme === 'redis-sentinel' || scheme === 'sentinel';

  let query = '';
  const queryIdx = rest.indexOf('?');
  if (queryIdx >= 0) {
    query = rest.slice(queryIdx + 1);
    rest = rest.slice(0, queryIdx);
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
      username = decodeComponent(auth.slice(0, colonIdx));
      password = decodeComponent(auth.slice(colonIdx + 1));
    } else {
      username = decodeComponent(auth);
    }
  }

  const nodes = rest
    .split(',')
    .map((token) => token.trim())
    .filter(Boolean)
    .map((token) => parseAuthorityHost(token, true))
    .filter((node): node is { host: string; port: string } => node != null);
  if (nodes.length === 0) return null;

  const params = new URLSearchParams(query);
  const queryUser = params.get('username');
  const queryPassword = params.get('password');
  const queryDb = params.get('db') ?? params.get('database');
  const queryMaster =
    params.get('name') ??
    params.get('master') ??
    params.get('sentinelMasterId') ??
    params.get('sentinelMasterName');
  const querySentinelPassword =
    params.get('sentinelPassword') ?? params.get('sentinel_password');
  if (queryUser) username = queryUser;
  if (queryPassword != null) password = queryPassword;

  const pathParts = path.split('/').filter(Boolean);
  let database: string | undefined;
  let masterName = queryMaster?.trim() || undefined;
  for (const part of pathParts) {
    if (/^\d+$/.test(part)) {
      database = part;
    } else if (!masterName) {
      masterName = decodeComponent(part);
    }
  }
  if (queryDb && /^\d+$/.test(queryDb)) database = queryDb;

  const first = nodes[0];
  const nodeAddrs = nodes.map((node) => `${node.host}:${node.port}`);

  if (sentinelScheme) {
    return omitUndefined({
      topology: 'sentinel',
      host: first.host,
      port: first.port,
      database,
      username: username || undefined,
      password,
      sentinelNodes: nodeAddrs,
      sentinelMasterName: masterName,
      sentinelNodePassword: querySentinelPassword ?? (queryPassword == null ? password : undefined),
      tlsEnabled: tlsEnabled || undefined,
    });
  }

  if (nodes.length > 1) {
    return omitUndefined({
      topology: 'cluster',
      host: first.host,
      port: first.port,
      username: username || undefined,
      password,
      clusterNodes: nodeAddrs,
      tlsEnabled: tlsEnabled || undefined,
    });
  }

  return omitUndefined({
    topology: 'standalone',
    host: first.host,
    port: first.port,
    database,
    username: username || undefined,
    password,
    tlsEnabled: tlsEnabled || undefined,
  });
}

function parseHostPortList(text: string): ParsedRedisClipboard | null {
  const tokens = text
    .split(/[\n,;]+/)
    .map((token) => token.trim())
    .filter(Boolean);
  if (tokens.length === 0) return null;

  if (tokens.length === 1) {
    return parseStandaloneToken(tokens[0]);
  }

  const nodes = tokens.map((token) => parseAuthorityHost(token, false));
  if (nodes.some((node) => node == null)) return null;
  const addrs = uniqueNodes(nodes.map((node) => `${node!.host}:${node!.port}`));
  if (addrs.length < 2) return null;
  const first = nodes[0]!;
  const sentinelish = nodes.every((node) => Number(node!.port) === 26379);
  if (sentinelish) {
    return {
      topology: 'sentinel',
      host: first.host,
      port: first.port,
      sentinelNodes: addrs,
    };
  }
  return {
    topology: 'cluster',
    host: first.host,
    port: first.port,
    clusterNodes: addrs,
  };
}

function parseStandaloneToken(token: string): ParsedRedisClipboard | null {
  if (token.startsWith('[')) {
    const hostPort = parseAuthorityHost(token, false);
    if (!hostPort) return null;
    return { topology: 'standalone', host: hostPort.host, port: hostPort.port };
  }

  const parts = token.split(':');
  if (parts.length < 2 || parts.length > 4) return null;
  const port = parsePort(parts[1]);
  if (!parts[0] || port == null) return null;

  const parsed: ParsedRedisClipboard = {
    topology: 'standalone',
    host: parts[0],
    port: String(port),
  };
  if (parts.length === 3) {
    if (/^\d{1,2}$/.test(parts[2]) && Number(parts[2]) <= 15) {
      parsed.database = parts[2];
    } else {
      parsed.password = parts[2];
    }
  } else if (parts.length === 4) {
    if (!/^\d+$/.test(parts[2])) return null;
    parsed.database = parts[2];
    parsed.password = parts[3];
  }
  return parsed;
}

function parseAuthorityHost(
  token: string,
  allowHostOnly: boolean,
): { host: string; port: string } | null {
  const trimmed = token.trim();
  if (!trimmed) return null;
  if (trimmed.startsWith('[')) {
    const end = trimmed.indexOf(']');
    if (end < 2) return null;
    const host = trimmed.slice(0, end + 1);
    const rest = trimmed.slice(end + 1);
    if (!rest) {
      return allowHostOnly ? { host, port: '6379' } : null;
    }
    if (!rest.startsWith(':')) return null;
    const port = parsePort(rest.slice(1));
    if (port == null) return null;
    return { host, port: String(port) };
  }
  const idx = trimmed.lastIndexOf(':');
  if (idx <= 0) {
    if (!allowHostOnly || trimmed.includes(':')) return null;
    return { host: trimmed, port: '6379' };
  }
  const host = trimmed.slice(0, idx);
  const port = parsePort(trimmed.slice(idx + 1));
  if (!host || port == null) return null;
  return { host, port: String(port) };
}

function parsePort(raw: string): number | null {
  if (!raw) return 6379;
  const value = Number(raw);
  if (!Number.isInteger(value) || value <= 0 || value > 65535) return null;
  return value;
}

function decodeComponent(value: string): string {
  try {
    return decodeURIComponent(value);
  } catch {
    return value;
  }
}

function uniqueNodes(nodes: string[]): string[] {
  const seen = new Set<string>();
  const out: string[] = [];
  for (const node of nodes) {
    if (seen.has(node)) continue;
    seen.add(node);
    out.push(node);
  }
  return out;
}

function splitHostPort(addr: string): { host: string; port: string } | null {
  return parseAuthorityHost(addr, false);
}

function suggestConnectionName(parsed: ParsedRedisClipboard): string {
  if (parsed.topology === 'sentinel' && parsed.sentinelMasterName) {
    return parsed.sentinelMasterName;
  }
  if (parsed.host && parsed.port) return `${parsed.host}:${parsed.port}`;
  return parsed.host ?? parsed.clusterNodes?.[0] ?? parsed.sentinelNodes?.[0] ?? '';
}

function toConnectionClipboardFill(parsed: ParsedRedisClipboard): ConnectionClipboardFill {
  const options: Record<string, unknown> = { topology: parsed.topology };
  if (parsed.topology === 'cluster') {
    options.clusterNodes = parsed.clusterNodes ?? [];
  }
  if (parsed.topology === 'sentinel') {
    options.sentinelNodes = parsed.sentinelNodes ?? [];
    if (parsed.sentinelMasterName) options.sentinelMasterName = parsed.sentinelMasterName;
    if (parsed.sentinelNodePassword) options.sentinelNodePassword = parsed.sentinelNodePassword;
  }
  if (parsed.tlsEnabled) options.tls = { enabled: true };
  return {
    host: parsed.host,
    port: parsed.port,
    database: parsed.database,
    username: parsed.username,
    password: parsed.password,
    name: suggestConnectionName(parsed) || undefined,
    sslMode: parsed.tlsEnabled ? 'require' : undefined,
    options,
    expandAdvanced: Boolean(parsed.tlsEnabled),
  };
}

function omitUndefined(obj: ParsedRedisClipboard): ParsedRedisClipboard {
  return Object.fromEntries(
    Object.entries(obj).filter(([, value]) => value !== undefined),
  ) as ParsedRedisClipboard;
}
