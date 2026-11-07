/** Tunnel configuration types for connection forms and IPC. */

export type TunnelKind = 'none' | 'ssh' | 'httpProxy' | 'websocket';

/**
 * SSH authentication method. Canonical definition lives here (tunnel domain)
 * and is re-exported from `src/types/index.ts` for existing imports.
 */
export type SshAuthMethod = 'password' | 'private_key' | 'agent';

/**
 * Which control owns the tunnel reference in the connection form.
 *
 * Single source of truth replacing the previous "kind select + saved select"
 * pair that overwrote each other:
 * - `none`   → direct connection, no tunnel fields
 * - `saved`  → references a `SavedTunnel` by `tunnelId`
 * - `inline` → one-off tunnel configured in the form itself
 */
export type TunnelSource = 'none' | 'saved' | 'inline';

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

/** SSH leg of a saved tunnel; same shape as `SshTunnelConfig`, `jump` is recursive. */
export interface SavedTunnelSshConfig {
  enabled: boolean;
  host: string;
  port: number;
  username: string;
  authMethod: SshAuthMethod;
  password?: string;
  privateKeyPath?: string;
  passphrase?: string;
  jump?: SavedTunnelSshConfig;
}

/** Independently stored tunnel entity (`tunnels.json`). Connections reference via `tunnelId`. */
export interface SavedTunnel {
  id: string;
  name: string;
  kind: Exclude<TunnelKind, 'none'>;
  ssh?: SavedTunnelSshConfig;
  httpProxy?: HttpProxyTunnelConfig;
  websocket?: WebSocketTunnelConfig;
}

/**
 * Secret-free projection of `SavedTunnel` returned by `get_tunnel_summaries`.
 * Lists and pickers only ever need name + kind, so decrypted credentials never
 * reach the webview for rendering (G9).
 */
export interface SavedTunnelSummary {
  id: string;
  name: string;
  kind: Exclude<TunnelKind, 'none'>;
}

/** Connections referencing a saved tunnel (`get_tunnel_usage`). */
export interface TunnelUsage {
  connectionIds: string[];
  connectionNames: string[];
}
