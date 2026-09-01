import type { ConnectionConfig, SshTunnelConfig } from '../types';
import {
  assertPlainConnectionConfig,
  buildConnectionConfig,
  clonePlainJson,
} from './connectionFormModel';

export type { BuildConnectionConfigInput, ConnectionFormSnapshot } from './connectionFormModel';
export {
  assertPlainConnectionConfig,
  buildConnectionConfig,
  buildConnectionConfig as buildConnectionConfigForIpc,
  clonePlainJson,
  sanitizeConnectionOptions,
} from './connectionFormModel';

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

/** Defensive clone for configs loaded from store before IPC (already plain, but immutable). */
export function cloneConnectionConfigForIpc(config: ConnectionConfig): ConnectionConfig {
  assertPlainConnectionConfig(config);
  const out: ConnectionConfig = {
    id: config.id,
    name: config.name,
    databaseType: config.databaseType,
    sslMode: config.sslMode,
  };
  if (config.host !== undefined) out.host = config.host;
  if (config.port !== undefined) out.port = config.port;
  if (config.database !== undefined) out.database = config.database;
  if (config.schema !== undefined) out.schema = config.schema;
  if (config.username !== undefined) out.username = config.username;
  if (config.password !== undefined) out.password = config.password;
  if (config.connectionTimeout !== undefined) out.connectionTimeout = config.connectionTimeout;
  if (config.maxPoolSize !== undefined) out.maxPoolSize = config.maxPoolSize;
  if (config.colorTag !== undefined) out.colorTag = config.colorTag;
  if (config.group !== undefined) out.group = config.group;
  if (config.lastConnectedAt !== undefined) out.lastConnectedAt = config.lastConnectedAt;
  if (config.serverVersion !== undefined) out.serverVersion = config.serverVersion;
  if (config.readOnly !== undefined) out.readOnly = config.readOnly;
  if (config.pinned !== undefined) out.pinned = config.pinned;
  if (config.sshTunnel) out.sshTunnel = cloneSshTunnel(config.sshTunnel);
  if (config.options && Object.keys(config.options).length > 0) {
    out.options = clonePlainJson(config.options);
  }
  return out;
}

/** IPC entry: validate then pass a fresh plain payload to Tauri. */
export function toIpcConnectionConfig(config: ConnectionConfig): ConnectionConfig {
  assertPlainConnectionConfig(config);
  return cloneConnectionConfigForIpc(config);
}
