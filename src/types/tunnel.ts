/** Tunnel configuration types for connection forms and IPC. */

export type TunnelKind = 'none' | 'ssh' | 'httpProxy' | 'websocket';

export interface HttpProxyTunnelConfig {
  enabled: boolean;
  host: string;
  port: number;
  scheme: 'http' | 'https';
  username?: string;
  password?: string;
  connectTimeoutSecs?: number;
}

export interface WebSocketTunnelConfig {
  enabled: boolean;
  url: string;
  mode: 'datazen_v1' | 'raw_binary';
  authToken?: string;
  connectTimeoutSecs?: number;
}

/** SSH tunnel fields nested under a SavedTunnel (mirrors SshTunnelConfig). */
export interface SavedTunnelSshConfig {
  enabled: boolean;
  host: string;
  port: number;
  username: string;
  authMethod: 'password' | 'private_key' | 'agent';
  password?: string;
  privateKeyPath?: string;
  passphrase?: string;
  jump?: SavedTunnelSshConfig;
}

export interface SavedTunnel {
  id: string;
  name: string;
  kind: TunnelKind;
  /** Backend field name is `ssh`; frontend may also see camelCase. */
  ssh?: SavedTunnelSshConfig;
  sshTunnel?: SavedTunnelSshConfig;
  httpProxy?: HttpProxyTunnelConfig;
  httpProxyTunnel?: HttpProxyTunnelConfig;
  websocket?: WebSocketTunnelConfig;
  websocketTunnel?: WebSocketTunnelConfig;
  createdAt?: string;
  updatedAt?: string;
}
