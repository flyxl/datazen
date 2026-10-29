import type {
  HttpProxyTunnelConfig,
  SavedTunnel,
  SavedTunnelSshConfig,
  SshAuthMethod,
  TunnelKind,
  WebSocketTunnelConfig,
} from '../types';

/**
 * String-field draft model for the settings tunnel editor.
 *
 * Ports and timeouts stay strings while editing so a controlled `<Input>` can be
 * cleared and retyped (converting to a number on every keystroke makes the field
 * impossible to empty); they are parsed once, on save. Everything here is pure
 * so the settings dialogs stay presentational and the conversions are unit-testable.
 */

export type SavedTunnelKind = Exclude<TunnelKind, 'none'>;

export const SAVED_TUNNEL_KINDS: readonly SavedTunnelKind[] = ['ssh', 'httpProxy', 'websocket'];

export const DEFAULT_SSH_PORT = '22';
export const DEFAULT_HTTP_PROXY_PORT = '8080';
export const DEFAULT_TUNNEL_TIMEOUT = '30';

export interface SshJumpDraft {
  enabled: boolean;
  host: string;
  port: string;
  username: string;
  authMethod: SshAuthMethod;
  password: string;
  keyPath: string;
  passphrase: string;
}

export interface SshDraft {
  host: string;
  port: string;
  username: string;
  authMethod: SshAuthMethod;
  password: string;
  keyPath: string;
  passphrase: string;
  jump: SshJumpDraft;
}

export interface HttpProxyDraft {
  host: string;
  port: string;
  scheme: 'http' | 'https';
  username: string;
  password: string;
  timeout: string;
}

export interface WebSocketDraft {
  url: string;
  mode: 'datazen_v1' | 'raw_binary';
  authToken: string;
  timeout: string;
}

export interface TunnelDraft {
  /** Id of the entity being edited; `null` while creating a new one. */
  id: string | null;
  name: string;
  kind: SavedTunnelKind;
  ssh: SshDraft;
  httpProxy: HttpProxyDraft;
  websocket: WebSocketDraft;
}

/**
 * Why a draft cannot be saved, as a translation key. `null` means valid.
 * Keys are listed here (not in the dialog) so both the guard and the message
 * stay in sync with the validation rules below.
 */
export const TUNNEL_DRAFT_PROBLEM_KEYS = {
  name: 'settings.tunnels.validation.name',
  sshHost: 'settings.tunnels.validation.sshHost',
  sshPort: 'settings.tunnels.validation.sshPort',
  sshUsername: 'settings.tunnels.validation.sshUsername',
  sshJumpHost: 'settings.tunnels.validation.sshJumpHost',
  sshJumpPort: 'settings.tunnels.validation.sshJumpPort',
  sshJumpUsername: 'settings.tunnels.validation.sshJumpUsername',
  httpProxyHost: 'settings.tunnels.validation.httpProxyHost',
  httpProxyPort: 'settings.tunnels.validation.httpProxyPort',
  wsUrl: 'settings.tunnels.validation.wsUrl',
} as const;

export type TunnelDraftProblem = keyof typeof TUNNEL_DRAFT_PROBLEM_KEYS;

function emptySshJumpDraft(): SshJumpDraft {
  return {
    enabled: false,
    host: '',
    port: DEFAULT_SSH_PORT,
    username: '',
    authMethod: 'password',
    password: '',
    keyPath: '',
    passphrase: '',
  };
}

function emptySshDraft(): SshDraft {
  return {
    host: '',
    port: DEFAULT_SSH_PORT,
    username: '',
    authMethod: 'password',
    password: '',
    keyPath: '',
    passphrase: '',
    jump: emptySshJumpDraft(),
  };
}

export function emptyTunnelDraft(): TunnelDraft {
  return {
    id: null,
    name: '',
    kind: 'ssh',
    ssh: emptySshDraft(),
    httpProxy: {
      host: '',
      port: DEFAULT_HTTP_PROXY_PORT,
      scheme: 'http',
      username: '',
      password: '',
      timeout: DEFAULT_TUNNEL_TIMEOUT,
    },
    websocket: {
      url: '',
      mode: 'datazen_v1',
      authToken: '',
      timeout: DEFAULT_TUNNEL_TIMEOUT,
    },
  };
}

function timeoutText(value: number | undefined): string {
  return value === undefined ? DEFAULT_TUNNEL_TIMEOUT : String(value);
}

function sshDraftFromConfig(config: SavedTunnelSshConfig | undefined): SshDraft {
  if (!config) return emptySshDraft();
  const jump = config.jump;
  return {
    host: config.host,
    port: String(config.port),
    username: config.username,
    authMethod: config.authMethod,
    password: config.password ?? '',
    keyPath: config.privateKeyPath ?? '',
    passphrase: config.passphrase ?? '',
    jump: jump
      ? {
          enabled: jump.enabled,
          host: jump.host,
          port: String(jump.port),
          username: jump.username,
          authMethod: jump.authMethod,
          password: jump.password ?? '',
          keyPath: jump.privateKeyPath ?? '',
          passphrase: jump.passphrase ?? '',
        }
      : emptySshJumpDraft(),
  };
}

/** Project a stored entity (credentials included) onto the editable draft. */
export function savedTunnelToDraft(tunnel: SavedTunnel): TunnelDraft {
  const draft = emptyTunnelDraft();
  return {
    ...draft,
    id: tunnel.id,
    name: tunnel.name,
    kind: tunnel.kind,
    ssh: sshDraftFromConfig(tunnel.ssh),
    httpProxy: tunnel.httpProxy
      ? {
          host: tunnel.httpProxy.host,
          port: String(tunnel.httpProxy.port),
          scheme: tunnel.httpProxy.scheme === 'https' ? 'https' : 'http',
          username: tunnel.httpProxy.username ?? '',
          password: tunnel.httpProxy.password ?? '',
          timeout: timeoutText(tunnel.httpProxy.connectTimeoutSecs),
        }
      : draft.httpProxy,
    websocket: tunnel.websocket
      ? {
          url: tunnel.websocket.url,
          mode: tunnel.websocket.mode === 'raw_binary' ? 'raw_binary' : 'datazen_v1',
          authToken: tunnel.websocket.authToken ?? '',
          timeout: timeoutText(tunnel.websocket.connectTimeoutSecs),
        }
      : draft.websocket,
  };
}

function isPositivePort(value: string): boolean {
  const port = Number(value);
  return value.trim().length > 0 && Number.isInteger(port) && port > 0 && port <= 65535;
}

function positivePort(value: string, fallback: number): number {
  const port = Number(value);
  return isPositivePort(value) ? port : fallback;
}

function timeoutSeconds(value: string): number {
  const seconds = Number(value);
  return Number.isFinite(seconds) && seconds > 0 ? seconds : Number(DEFAULT_TUNNEL_TIMEOUT);
}

function sshConfigFromDraft(draft: SshDraft): SavedTunnelSshConfig {
  const jump = draft.jump;
  return {
    enabled: true,
    host: draft.host.trim(),
    port: positivePort(draft.port, Number(DEFAULT_SSH_PORT)),
    username: draft.username.trim(),
    authMethod: draft.authMethod,
    password: draft.authMethod === 'password' ? draft.password || undefined : undefined,
    privateKeyPath: draft.authMethod === 'private_key' ? draft.keyPath || undefined : undefined,
    passphrase: draft.authMethod === 'private_key' ? draft.passphrase || undefined : undefined,
    jump: jump.enabled
      ? {
          enabled: true,
          host: jump.host.trim(),
          port: positivePort(jump.port, Number(DEFAULT_SSH_PORT)),
          username: jump.username.trim(),
          authMethod: jump.authMethod,
          password: jump.authMethod === 'password' ? jump.password || undefined : undefined,
          privateKeyPath: jump.authMethod === 'private_key' ? jump.keyPath || undefined : undefined,
          passphrase: jump.authMethod === 'private_key' ? jump.passphrase || undefined : undefined,
        }
      : undefined,
  };
}

function httpProxyConfigFromDraft(draft: HttpProxyDraft): HttpProxyTunnelConfig {
  return {
    enabled: true,
    host: draft.host.trim(),
    port: positivePort(draft.port, Number(DEFAULT_HTTP_PROXY_PORT)),
    scheme: draft.scheme,
    username: draft.username || undefined,
    password: draft.password || undefined,
    connectTimeoutSecs: timeoutSeconds(draft.timeout),
  };
}

function webSocketConfigFromDraft(draft: WebSocketDraft): WebSocketTunnelConfig {
  return {
    enabled: true,
    url: draft.url.trim(),
    mode: draft.mode,
    authToken: draft.authToken || undefined,
    connectTimeoutSecs: timeoutSeconds(draft.timeout),
  };
}

/**
 * Build the entity to persist. Only the block matching `kind` is emitted, so a
 * tunnel never carries a stale config from a kind the user switched away from.
 */
export function draftToSavedTunnel(draft: TunnelDraft, id: string): SavedTunnel {
  const base: SavedTunnel = { id, name: draft.name.trim(), kind: draft.kind };
  if (draft.kind === 'ssh') return { ...base, ssh: sshConfigFromDraft(draft.ssh) };
  if (draft.kind === 'httpProxy') {
    return { ...base, httpProxy: httpProxyConfigFromDraft(draft.httpProxy) };
  }
  return { ...base, websocket: webSocketConfigFromDraft(draft.websocket) };
}

/** First unmet requirement of the draft, or `null` when it can be saved. */
export function tunnelDraftProblem(draft: TunnelDraft): TunnelDraftProblem | null {
  if (draft.name.trim().length === 0) return 'name';
  switch (draft.kind) {
    case 'ssh': {
      if (draft.ssh.host.trim().length === 0) return 'sshHost';
      if (!isPositivePort(draft.ssh.port)) return 'sshPort';
      if (draft.ssh.username.trim().length === 0) return 'sshUsername';
      if (draft.ssh.jump.enabled) {
        if (draft.ssh.jump.host.trim().length === 0) return 'sshJumpHost';
        if (!isPositivePort(draft.ssh.jump.port)) return 'sshJumpPort';
        if (draft.ssh.jump.username.trim().length === 0) return 'sshJumpUsername';
      }
      return null;
    }
    case 'httpProxy':
      if (draft.httpProxy.host.trim().length === 0) return 'httpProxyHost';
      if (!isPositivePort(draft.httpProxy.port)) return 'httpProxyPort';
      return null;
    case 'websocket':
      if (draft.websocket.url.trim().length === 0) return 'wsUrl';
      return null;
  }
}

export function isTunnelDraftValid(draft: TunnelDraft): boolean {
  return tunnelDraftProblem(draft) === null;
}

/** `Bastion` + ` (copy)` → `Bastion (copy)`. */
export function copyTunnelName(name: string, suffix: string): string {
  return `${name}${suffix}`;
}
