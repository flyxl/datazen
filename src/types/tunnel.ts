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

export interface SavedTunnel {
  id: string;
  name: string;
  kind: TunnelKind;
  sshTunnel?: import('./connection').SshTunnelConfig;
  httpProxyTunnel?: HttpProxyTunnelConfig;
  websocketTunnel?: WebSocketTunnelConfig;
  createdAt?: string;
  updatedAt?: string;
}
