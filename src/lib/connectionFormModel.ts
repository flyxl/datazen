import { DB_REGISTRY, normalizeIndexDatabaseField } from './databaseTypes';
import { getPluginConnectionForm } from '../plugins/generated';
import type { ConnectionConfig, DatabaseType, SslMode, SshTunnelConfig } from '../types';

function hasEnabledTlsOption(options: Record<string, unknown>): boolean {
  const tls = options.tls;
  return Boolean(
    tls &&
      typeof tls === 'object' &&
      !Array.isArray(tls) &&
      (tls as { enabled?: unknown }).enabled === true,
  );
}

function cloneSshTunnel(tunnel: SshTunnelConfig): SshTunnelConfig {
  const clone: SshTunnelConfig = {
    enabled: tunnel.enabled,
    host: tunnel.host,
    port: tunnel.port,
    username: tunnel.username,
    authMethod: tunnel.authMethod,
  };
  if (tunnel.password !== undefined) clone.password = tunnel.password;
  if (tunnel.privateKeyPath !== undefined) clone.privateKeyPath = tunnel.privateKeyPath;
  if (tunnel.passphrase !== undefined) clone.passphrase = tunnel.passphrase;
  if (tunnel.jump) clone.jump = cloneSshTunnel(tunnel.jump);
  return clone;
}

/** Scalar fields captured when switching database type in the new-connection form. */
export type ConnectionFormSnapshot = {
  name: string;
  host: string;
  port: string;
  database: string;
  schema: string;
  username: string;
  password: string;
  sslMode: SslMode;
  group: string;
  colorTag: string;
  readOnly: boolean;
  connectionOptions: Record<string, unknown>;
  showAdvanced: boolean;
  sshEnabled: boolean;
  sshHost: string;
  sshPort: string;
  sshUsername: string;
  sshAuthMethod: SshTunnelConfig['authMethod'];
  sshPassword: string;
  sshKeyPath: string;
  sshPassphrase: string;
  sshJumpEnabled: boolean;
  sshJumpHost: string;
  sshJumpPort: string;
  sshJumpUsername: string;
  sshJumpAuthMethod: SshTunnelConfig['authMethod'];
  sshJumpPassword: string;
  sshJumpKeyPath: string;
  sshJumpPassphrase: string;
};

export interface BuildConnectionConfigInput {
  editId?: string | null;
  newId: () => string;
  unnamedLabel: string;
  name: string;
  databaseType: DatabaseType;
  host: string;
  port: string;
  database: string;
  schema: string;
  username: string;
  password: string;
  sslMode: SslMode;
  group: string;
  colorTag: string;
  readOnly: boolean;
  connectionOptions: Record<string, unknown>;
  sshTunnel?: SshTunnelConfig;
}

/**
 * Single factory for ConnectionConfig — the only path from form fields to IPC/persist.
 * Always returns a fresh plain object; never shares references with React state.
 */
export function buildConnectionConfig(input: BuildConnectionConfigInput): ConnectionConfig {
  const meta = DB_REGISTRY[input.databaseType];
  const effectiveSslMode =
    meta?.defaultSslMode === 'disable' && !hasEnabledTlsOption(input.connectionOptions)
      ? 'disable'
      : input.sslMode;

  const base: ConnectionConfig = {
    id: input.editId ?? input.newId(),
    name: input.name || input.unnamedLabel,
    databaseType: input.databaseType,
    sslMode: effectiveSslMode,
    group: coerceConnectionGroup(input.group) || undefined,
    colorTag: input.colorTag || undefined,
    readOnly: input.readOnly || undefined,
  };

  if (input.sshTunnel) {
    base.sshTunnel = cloneSshTunnel(input.sshTunnel);
  }

  if (!meta || meta.connectionMode === 'file') {
    return { ...base, database: input.database };
  }

  const conn: ConnectionConfig = {
    ...base,
    host: input.host || meta.defaultHost || undefined,
    port: Number(input.port) || meta.defaultPort || undefined,
    database:
      meta.databaseFieldType === 'index'
        ? normalizeIndexDatabaseField(input.database, meta.maxDatabaseIndex ?? 15)
        : input.database || undefined,
    password: input.password || undefined,
  };

  if (meta.defaultUser || meta.requiresUsername || getPluginConnectionForm(meta.connectionForm)) {
    conn.username = input.username || meta.defaultUser || undefined;
  }
  if (meta.connectionIncludesSchema) {
    conn.schema = input.schema.trim() || 'default';
  }
  if (Object.keys(input.connectionOptions).length > 0) {
    conn.options = clonePlainJson(input.connectionOptions) as Record<string, unknown>;
  }
  return conn;
}

/** Deep-clone JSON-compatible values (throws on cycle — options must stay acyclic in state). */
export function clonePlainJson<T>(value: T): T {
  return JSON.parse(JSON.stringify(value)) as T;
}

/** Sanitize opaque driver options before storing in React state. */
export function sanitizeConnectionOptions(value: Record<string, unknown>): Record<string, unknown> {
  return clonePlainJson(value);
}

/** Group id must stay a plain string — reject mistaken DOM/event objects from handlers. */
export function coerceConnectionGroup(value: unknown): string {
  return typeof value === 'string' ? value : '';
}

const FORM_STATE_MARKERS = new Set([
  'setName',
  'onTest',
  'onSave',
  'draft',
  'meta',
  'setOptions',
  'handleDatabaseTypeChange',
]);

/**
 * Assert value is a plain ConnectionConfig suitable for Tauri IPC.
 * Rejects ConnectionFormState mistaken for config and any non-JSON values.
 */
export function assertPlainConnectionConfig(value: unknown): asserts value is ConnectionConfig {
  if (!value || typeof value !== 'object' || Array.isArray(value)) {
    throw new Error('ConnectionConfig must be a plain object');
  }
  const root = value as Record<string, unknown>;
  for (const marker of FORM_STATE_MARKERS) {
    if (marker in root) {
      throw new Error(
        `IPC received ConnectionFormState (found "${marker}") — use buildConnectionConfig() instead`,
      );
    }
  }

  const seen = new WeakSet<object>();
  const walk = (current: unknown, path: string): void => {
    if (current === undefined) return;
    if (current === null) return;
    const kind = typeof current;
    if (kind === 'string' || kind === 'number' || kind === 'boolean') return;
    if (kind === 'function' || kind === 'symbol' || kind === 'undefined') {
      throw new Error(`ConnectionConfig contains non-serializable ${kind} at ${path || '(root)'}`);
    }
    if (kind !== 'object') return;
    if (seen.has(current)) {
      throw new Error(`ConnectionConfig contains a cycle at ${path || '(root)'}`);
    }
    seen.add(current);
    if (Array.isArray(current)) {
      current.forEach((item, index) => walk(item, `${path}[${index}]`));
      return;
    }
    for (const [key, nested] of Object.entries(current)) {
      walk(nested, path ? `${path}.${key}` : key);
    }
  };
  walk(root, '');
}
