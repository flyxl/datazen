/** Tunnel configuration types for connection forms and IPC. */

import type { SshTunnelConfig } from './index';

export type TunnelKind = 'none' | 'ssh' | 'httpProxy' | 'websocket';

export interface HttpProxyTunnelConfig {
  enabled: boolean;
  host: string;
  port: number;
  /** Transport to the proxy itself: `http` or `https`. */
  scheme: 'http' | 'https';
  username?: string;
  password?: string;
  headers?: Record<string, string>;
  connectTimeoutSecs?: number;
}

export interface WebSocketTunnelConfig {
  enabled: boolean;
  /** Full URL, e.g. `wss://relay.example.com/v1/tunnel`. */
  url: string;
  authToken?: string;
  headers?: Record<string, string>;
  connectTimeoutSecs?: number;
  pingIntervalSecs?: number;
  /** `datazen_v1` (JSON control + binary) or `raw_binary`. */
  mode?: 'datazen_v1' | 'raw_binary';
}

/** Independently stored tunnel entity (`tunnels.json`). Connections reference via `tunnelId`. */
export interface SavedTunnel {
  id: string;
  name: string;
  kind: Exclude<TunnelKind, 'none'>;
  ssh?: SshTunnelConfig;
  httpProxy?: HttpProxyTunnelConfig;
  websocket?: WebSocketTunnelConfig;
}
