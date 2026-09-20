import { DB_REGISTRY, normalizeIndexDatabaseField } from './databaseTypes';
import { getDriverConnectionForm } from '../extensions/generated';
import type {
  ConnectionConfig,
  DatabaseType,
  HttpProxyTunnelConfig,
  SslMode,
  SshTunnelConfig,
  TunnelKind,
  WebSocketTunnelConfig,
} from '../types';

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
  tunnelKind?: TunnelKind;
  httpProxyTunnel?: HttpProxyTunnelConfig;
  websocketTunnel?: WebSocketTunnelConfig;
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
    readOnly: meta?.readOnly === true || input.readOnly || undefined,
  };

  if (input.sshTunnel) {
    base.sshTunnel = cloneSshTunnel(input.sshTunnel);
  }
  if (input.tunnelKind && input.tunnelKind !== 'none') {
    base.tunnelKind = input.tunnelKind;
  }
  if (input.httpProxyTunnel?.enabled) {
    base.httpProxyTunnel = { ...input.httpProxyTunnel };
  }
  if (input.websocketTunnel?.enabled) {
    base.websocketTunnel = { ...input.websocketTunnel };
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

  if (meta.connectionIncludesSchema) {
    conn.schema = input.schema || undefined;
  }
  if (meta.hasUsername !== false && input.username) {
    conn.username = input.username;
  }
  if (Object.keys(input.connectionOptions).length > 0) {
    conn.options = { ...input.connectionOptions };
  }

  return conn;
}

export function coerceConnectionGroup(value: unknown): string {
  if (typeof value !== 'string') return '';
  return value.trim();
}

export function sanitizeConnectionOptions(
  options: Record<string, unknown> | undefined | null,
): Record<string, unknown> {
  if (!options || typeof options !== 'object' || Array.isArray(options)) return {};
  return { ...options };
}

export function clonePlainJson<T>(value: T): T {
  return JSON.parse(JSON.stringify(value)) as T;
}

export function assertPlainConnectionConfig(config: ConnectionConfig): void {
  if (config == null || typeof config !== 'object') {
    throw new Error('ConnectionConfig must be a plain object');
  }
}
