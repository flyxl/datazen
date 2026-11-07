import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import type { Dispatch, SetStateAction } from 'react';
import { tunnelCommands } from '../../commands/tunnel';
import { useTunnelStore, newTunnelId } from '../../stores/tunnelStore';
import { useI18n } from '../../hooks/useI18n';
import type {
  HttpProxyTunnelConfig,
  SavedTunnel,
  SavedTunnelSshConfig,
  SavedTunnelSummary,
  SshAuthMethod,
  TunnelKind,
  TunnelSource,
  WebSocketTunnelConfig,
} from '../../types';

/**
 * Tunnel slice of the connection form: one three-state machine plus every
 * inline tunnel field it owns.
 *
 * | state   | enter                        | inside                                | exit                       |
 * |---------|------------------------------|---------------------------------------|----------------------------|
 * | none    | pick "None (direct)"         | tunnel fields hidden                  | saved / inline             |
 * | saved   | pick a `SavedTunnelSummary`  | read-only name + kind, fields folded  | unbind → inline / none     |
 * | inline  | pick "Inline configuration"  | kind sub-selector + inline fields     | none / save-as → saved     |
 *
 * `tunnelId` is only cleared by an explicit transition (`none`, `unbind`),
 * never as a side effect of changing the tunnel kind.
 */
export interface UseTunnelFormStateOptions {
  /** Whether the selected driver supports an SSH tunnel (`DB_REGISTRY[...].supportsSSH`). */
  supportsSSH: boolean;
}

export interface TunnelFormSlice {
  tunnelSource: TunnelSource;
  setTunnelSource: (source: TunnelSource) => void;
  tunnelKind: TunnelKind;
  setTunnelKind: (kind: TunnelKind) => void;
  tunnelId: string | null;
  setTunnelId: (id: string | null) => void;
  savedTunnels: SavedTunnelSummary[];
  savedTunnel: SavedTunnelSummary | null;
  tunnelRefMissing: boolean;
  tunnelInlineValid: boolean;
  tunnelBusy: boolean;
  tunnelError: string | null;
  effectiveTunnelKind: TunnelKind;
  unbindTunnel: () => Promise<void>;
  saveAsTunnel: (name: string) => Promise<SavedTunnel | null>;
  hydrateTunnelRef: (id: string | null, kind: TunnelKind) => void;
  resetTunnel: () => void;

  sshEnabled: boolean;
  setSshEnabled: (v: boolean) => void;
  sshHost: string;
  setSshHost: Dispatch<SetStateAction<string>>;
  sshPort: string;
  setSshPort: Dispatch<SetStateAction<string>>;
  sshUsername: string;
  setSshUsername: Dispatch<SetStateAction<string>>;
  sshAuthMethod: SshAuthMethod;
  setSshAuthMethod: Dispatch<SetStateAction<SshAuthMethod>>;
  sshPassword: string;
  setSshPassword: Dispatch<SetStateAction<string>>;
  sshKeyPath: string;
  setSshKeyPath: Dispatch<SetStateAction<string>>;
  sshPassphrase: string;
  setSshPassphrase: Dispatch<SetStateAction<string>>;
  sshJumpEnabled: boolean;
  setSshJumpEnabled: Dispatch<SetStateAction<boolean>>;
  sshJumpHost: string;
  setSshJumpHost: Dispatch<SetStateAction<string>>;
  sshJumpPort: string;
  setSshJumpPort: Dispatch<SetStateAction<string>>;
  sshJumpUsername: string;
  setSshJumpUsername: Dispatch<SetStateAction<string>>;
  sshJumpAuthMethod: SshAuthMethod;
  setSshJumpAuthMethod: Dispatch<SetStateAction<SshAuthMethod>>;
  sshJumpPassword: string;
  setSshJumpPassword: Dispatch<SetStateAction<string>>;
  sshJumpKeyPath: string;
  setSshJumpKeyPath: Dispatch<SetStateAction<string>>;
  sshJumpPassphrase: string;
  setSshJumpPassphrase: Dispatch<SetStateAction<string>>;

  httpProxyHost: string;
  setHttpProxyHost: Dispatch<SetStateAction<string>>;
  httpProxyPort: string;
  setHttpProxyPort: Dispatch<SetStateAction<string>>;
  httpProxyScheme: 'http' | 'https';
  setHttpProxyScheme: Dispatch<SetStateAction<'http' | 'https'>>;
  httpProxyUsername: string;
  setHttpProxyUsername: Dispatch<SetStateAction<string>>;
  httpProxyPassword: string;
  setHttpProxyPassword: Dispatch<SetStateAction<string>>;
  httpProxyTimeout: string;
  setHttpProxyTimeout: Dispatch<SetStateAction<string>>;

  wsUrl: string;
  setWsUrl: Dispatch<SetStateAction<string>>;
  wsMode: 'datazen_v1' | 'raw_binary';
  setWsMode: Dispatch<SetStateAction<'datazen_v1' | 'raw_binary'>>;
  wsAuthToken: string;
  setWsAuthToken: Dispatch<SetStateAction<string>>;
  wsTimeout: string;
  setWsTimeout: Dispatch<SetStateAction<string>>;
}

function errorMessage(e: unknown): string {
  if (typeof e === 'string' && e) return e;
  if (e instanceof Error && e.message) return e.message;
  return '';
}

function isPositivePort(value: string): boolean {
  const port = Number(value);
  return value.trim().length > 0 && Number.isFinite(port) && port > 0;
}

export function useTunnelFormState({ supportsSSH }: UseTunnelFormStateOptions): TunnelFormSlice {
  const { t } = useI18n();
  const summaries = useTunnelStore((s) => s.summaries);
  const summariesLoaded = useTunnelStore((s) => s.loaded);
  const loadTunnels = useTunnelStore((s) => s.load);
  const createTunnel = useTunnelStore((s) => s.create);
  const summariesError = useTunnelStore((s) => s.error);

  const [tunnelSource, setTunnelSourceState] = useState<TunnelSource>('none');
  const [tunnelId, setTunnelIdState] = useState<string | null>(null);
  const [tunnelKind, setTunnelKindState] = useState<TunnelKind>('none');
  const [tunnelBusy, setTunnelBusy] = useState(false);
  const [tunnelError, setTunnelError] = useState<string | null>(null);

  /** Remembers the last inline kind so `none → inline` restores the user's choice. */
  const lastInlineKindRef = useRef<TunnelKind>(supportsSSH ? 'ssh' : 'httpProxy');

  const [sshEnabled, setSshEnabledState] = useState(false);
  const [sshHost, setSshHost] = useState('');
  const [sshPort, setSshPort] = useState('22');
  const [sshUsername, setSshUsername] = useState('');
  const [sshAuthMethod, setSshAuthMethod] = useState<SshAuthMethod>('password');
  const [sshPassword, setSshPassword] = useState('');
  const [sshKeyPath, setSshKeyPath] = useState('');
  const [sshPassphrase, setSshPassphrase] = useState('');
  const [sshJumpEnabled, setSshJumpEnabled] = useState(false);
  const [sshJumpHost, setSshJumpHost] = useState('');
  const [sshJumpPort, setSshJumpPort] = useState('22');
  const [sshJumpUsername, setSshJumpUsername] = useState('');
  const [sshJumpAuthMethod, setSshJumpAuthMethod] = useState<SshAuthMethod>('password');
  const [sshJumpPassword, setSshJumpPassword] = useState('');
  const [sshJumpKeyPath, setSshJumpKeyPath] = useState('');
  const [sshJumpPassphrase, setSshJumpPassphrase] = useState('');

  const [httpProxyHost, setHttpProxyHost] = useState('');
  const [httpProxyPort, setHttpProxyPort] = useState('8080');
  const [httpProxyScheme, setHttpProxyScheme] = useState<'http' | 'https'>('http');
  const [httpProxyUsername, setHttpProxyUsername] = useState('');
  const [httpProxyPassword, setHttpProxyPassword] = useState('');
  const [httpProxyTimeout, setHttpProxyTimeout] = useState('30');

  const [wsUrl, setWsUrl] = useState('');
  const [wsMode, setWsMode] = useState<'datazen_v1' | 'raw_binary'>('datazen_v1');
  const [wsAuthToken, setWsAuthToken] = useState('');
  const [wsTimeout, setWsTimeout] = useState('30');

  // App-wide store: the first consumer loads, everyone else reuses (no stale list).
  useEffect(() => {
    void loadTunnels();
  }, [loadTunnels]);

  const savedTunnel = useMemo(
    () => (tunnelId ? (summaries.find((s) => s.id === tunnelId) ?? null) : null),
    [summaries, tunnelId],
  );
  /**
   * A dangling reference must be surfaced, not silently repaired — but only
   * once the collection actually loaded: a failed load proves nothing, and
   * flagging it would block saving valid connections (三维影响度自查 #2).
   */
  const tunnelRefMissing =
    tunnelId !== null && summariesLoaded && summariesError === null && savedTunnel === null;

  /**
   * Kind actually in effect, derived per state:
   * - `saved`  → the referenced entity's kind (authoritative over a stale hint)
   * - `inline` → the sub-selector, except SSH with its toggle off, which carries
   *              no config and must not be emitted as `tunnelKind: 'ssh'`
   * - `none`   → nothing
   */
  const effectiveTunnelKind: TunnelKind =
    tunnelSource === 'saved' && savedTunnel
      ? savedTunnel.kind
      : tunnelSource === 'inline' && tunnelKind === 'ssh' && !sshEnabled
        ? 'none'
        : tunnelKind;

  const tunnelInlineValid = useMemo(() => {
    if (tunnelSource !== 'inline') return false;
    switch (tunnelKind) {
      case 'ssh':
        return sshEnabled && sshHost.trim().length > 0 && sshUsername.trim().length > 0;
      case 'httpProxy':
        return httpProxyHost.trim().length > 0 && isPositivePort(httpProxyPort);
      case 'websocket':
        return wsUrl.trim().length > 0;
      default:
        return false;
    }
  }, [
    httpProxyHost,
    httpProxyPort,
    sshEnabled,
    sshHost,
    sshUsername,
    tunnelKind,
    tunnelSource,
    wsUrl,
  ]);

  const buildInlineTunnel = useCallback(
    (name: string): SavedTunnel | null => {
      if (tunnelKind === 'none') return null;
      const base = { id: newTunnelId(), name: name.trim(), kind: tunnelKind };
      if (tunnelKind === 'ssh') {
        const ssh: SavedTunnelSshConfig = {
          enabled: true,
          host: sshHost,
          port: Number(sshPort) || 22,
          username: sshUsername,
          authMethod: sshAuthMethod,
          password: sshAuthMethod === 'password' ? sshPassword || undefined : undefined,
          privateKeyPath: sshAuthMethod === 'private_key' ? sshKeyPath || undefined : undefined,
          passphrase: sshAuthMethod === 'private_key' ? sshPassphrase || undefined : undefined,
          jump: sshJumpEnabled
            ? {
                enabled: true,
                host: sshJumpHost,
                port: Number(sshJumpPort) || 22,
                username: sshJumpUsername,
                authMethod: sshJumpAuthMethod,
                password:
                  sshJumpAuthMethod === 'password' ? sshJumpPassword || undefined : undefined,
                privateKeyPath:
                  sshJumpAuthMethod === 'private_key' ? sshJumpKeyPath || undefined : undefined,
                passphrase:
                  sshJumpAuthMethod === 'private_key' ? sshJumpPassphrase || undefined : undefined,
              }
            : undefined,
        };
        return { ...base, ssh };
      }
      if (tunnelKind === 'httpProxy') {
        const httpProxy: HttpProxyTunnelConfig = {
          enabled: true,
          host: httpProxyHost,
          port: Number(httpProxyPort) || 8080,
          scheme: httpProxyScheme,
          username: httpProxyUsername || undefined,
          password: httpProxyPassword || undefined,
          connectTimeoutSecs: Number(httpProxyTimeout) || 30,
        };
        return { ...base, httpProxy };
      }
      const websocket: WebSocketTunnelConfig = {
        enabled: true,
        url: wsUrl,
        mode: wsMode,
        authToken: wsAuthToken || undefined,
        connectTimeoutSecs: Number(wsTimeout) || 30,
      };
      return { ...base, websocket };
    },
    [
      httpProxyHost,
      httpProxyPassword,
      httpProxyPort,
      httpProxyScheme,
      httpProxyTimeout,
      httpProxyUsername,
      sshAuthMethod,
      sshHost,
      sshJumpAuthMethod,
      sshJumpEnabled,
      sshJumpHost,
      sshJumpKeyPath,
      sshJumpPassphrase,
      sshJumpPassword,
      sshJumpPort,
      sshJumpUsername,
      sshKeyPath,
      sshPassphrase,
      sshPassword,
      sshPort,
      sshUsername,
      tunnelKind,
      wsAuthToken,
      wsMode,
      wsTimeout,
      wsUrl,
    ],
  );

  /** Project a saved entity back onto the inline fields (unbind must not lose parameters). */
  const applyTunnelToInline = useCallback((tunnel: SavedTunnel) => {
    lastInlineKindRef.current = tunnel.kind;
    setTunnelKindState(tunnel.kind);
    setSshEnabledState(tunnel.kind === 'ssh');

    if (tunnel.ssh) {
      setSshHost(tunnel.ssh.host);
      setSshPort(String(tunnel.ssh.port));
      setSshUsername(tunnel.ssh.username);
      setSshAuthMethod(tunnel.ssh.authMethod);
      setSshPassword(tunnel.ssh.password ?? '');
      setSshKeyPath(tunnel.ssh.privateKeyPath ?? '');
      setSshPassphrase(tunnel.ssh.passphrase ?? '');
      const jump = tunnel.ssh.jump;
      setSshJumpEnabled(!!jump?.enabled);
      setSshJumpHost(jump?.host ?? '');
      setSshJumpPort(jump ? String(jump.port) : '22');
      setSshJumpUsername(jump?.username ?? '');
      setSshJumpAuthMethod(jump?.authMethod ?? 'password');
      setSshJumpPassword(jump?.password ?? '');
      setSshJumpKeyPath(jump?.privateKeyPath ?? '');
      setSshJumpPassphrase(jump?.passphrase ?? '');
    }

    if (tunnel.httpProxy) {
      setHttpProxyHost(tunnel.httpProxy.host);
      setHttpProxyPort(String(tunnel.httpProxy.port));
      setHttpProxyScheme(tunnel.httpProxy.scheme === 'https' ? 'https' : 'http');
      setHttpProxyUsername(tunnel.httpProxy.username ?? '');
      setHttpProxyPassword(tunnel.httpProxy.password ?? '');
      setHttpProxyTimeout(String(tunnel.httpProxy.connectTimeoutSecs ?? 30));
    }

    if (tunnel.websocket) {
      setWsUrl(tunnel.websocket.url);
      setWsMode(tunnel.websocket.mode === 'raw_binary' ? 'raw_binary' : 'datazen_v1');
      setWsAuthToken(tunnel.websocket.authToken ?? '');
      setWsTimeout(String(tunnel.websocket.connectTimeoutSecs ?? 30));
    }
  }, []);

  /** saved → inline. Returns false when the entity could not be read (reference kept). */
  const refillFromSaved = useCallback(async (): Promise<boolean> => {
    const id = tunnelId;
    if (!id) return true;
    setTunnelBusy(true);
    setTunnelError(null);
    try {
      const tunnel = await tunnelCommands.getTunnel(id);
      if (!tunnel) {
        setTunnelError(t('newConn.tunnelUnbindKept'));
        return false;
      }
      applyTunnelToInline(tunnel);
      setTunnelIdState(null);
      setTunnelSourceState('inline');
      return true;
    } catch (e) {
      setTunnelError(errorMessage(e) || t('newConn.tunnelUnbindKept'));
      return false;
    } finally {
      setTunnelBusy(false);
    }
  }, [applyTunnelToInline, t, tunnelId]);

  const unbindTunnel = useCallback(async (): Promise<void> => {
    if (!tunnelId) {
      setTunnelSourceState('inline');
      return;
    }
    await refillFromSaved();
  }, [refillFromSaved, tunnelId]);

  const setTunnelSource = useCallback(
    (source: TunnelSource) => {
      if (source === 'none') {
        setTunnelSourceState('none');
        setTunnelIdState(null);
        setTunnelKindState('none');
        setSshEnabledState(false);
        return;
      }

      if (source === 'saved') {
        // Entering `saved` requires a candidate; an empty collection keeps `none`.
        if (summaries.length === 0) return;
        const target =
          (tunnelId ? summaries.find((s) => s.id === tunnelId) : undefined) ?? summaries[0];
        setTunnelIdState(target.id);
        setTunnelKindState(target.kind);
        setSshEnabledState(target.kind === 'ssh');
        setTunnelSourceState('saved');
        return;
      }

      // inline
      if (tunnelSource === 'saved' && tunnelId) {
        void refillFromSaved();
        return;
      }
      const kind = lastInlineKindRef.current;
      setTunnelKindState(kind);
      setSshEnabledState(kind === 'ssh');
      setTunnelSourceState('inline');
    },
    [refillFromSaved, summaries, tunnelId, tunnelSource],
  );

  const setTunnelKind = useCallback(
    (kind: TunnelKind) => {
      if (kind === 'none') {
        setTunnelSource('none');
        return;
      }
      if (tunnelSource === 'saved' && tunnelId) {
        // Explicit exit: refill from the entity first, then switch the sub-kind.
        void (async () => {
          const ok = await refillFromSaved();
          if (!ok) return;
          lastInlineKindRef.current = kind;
          setTunnelKindState(kind);
          setSshEnabledState(kind === 'ssh');
        })();
        return;
      }
      lastInlineKindRef.current = kind;
      setTunnelKindState(kind);
      setSshEnabledState(kind === 'ssh');
      setTunnelSourceState('inline');
    },
    [refillFromSaved, setTunnelSource, tunnelId, tunnelSource],
  );

  const setTunnelId = useCallback(
    (id: string | null) => {
      if (id) {
        const summary = summaries.find((s) => s.id === id);
        setTunnelIdState(id);
        if (summary) {
          setTunnelKindState(summary.kind);
          setSshEnabledState(summary.kind === 'ssh');
        }
        setTunnelSourceState('saved');
        return;
      }
      // Clearing the reference is an explicit transition: refill, never drop.
      if (tunnelId) {
        void refillFromSaved();
        return;
      }
      setTunnelIdState(null);
    },
    [refillFromSaved, summaries, tunnelId],
  );

  const hydrateTunnelRef = useCallback((id: string | null, kind: TunnelKind) => {
    if (id) {
      setTunnelIdState(id);
      setTunnelSourceState('saved');
      setTunnelKindState(kind);
      setSshEnabledState(kind === 'ssh');
      return;
    }
    setTunnelIdState(null);
    if (kind === 'none') {
      setTunnelSourceState('none');
      setTunnelKindState('none');
      setSshEnabledState(false);
      return;
    }
    lastInlineKindRef.current = kind;
    setTunnelSourceState('inline');
    setTunnelKindState(kind);
    setSshEnabledState(kind === 'ssh');
  }, []);

  const saveAsTunnel = useCallback(
    async (name: string): Promise<SavedTunnel | null> => {
      const input = buildInlineTunnel(name);
      if (!input || !tunnelInlineValid) return null;
      setTunnelBusy(true);
      setTunnelError(null);
      try {
        const tunnel = await createTunnel(input);
        lastInlineKindRef.current = tunnel.kind;
        setTunnelIdState(tunnel.id);
        setTunnelKindState(tunnel.kind);
        setSshEnabledState(tunnel.kind === 'ssh');
        setTunnelSourceState('saved');
        return tunnel;
      } catch (e) {
        setTunnelError(errorMessage(e) || t('newConn.tunnelSaveFailed'));
        return null;
      } finally {
        setTunnelBusy(false);
      }
    },
    [buildInlineTunnel, createTunnel, t, tunnelInlineValid],
  );

  const setSshEnabled = useCallback((v: boolean) => {
    setSshEnabledState(v);
    if (!v) return;
    lastInlineKindRef.current = 'ssh';
    setTunnelKindState('ssh');
    setTunnelSourceState((prev) => (prev === 'saved' ? prev : 'inline'));
  }, []);

  const resetTunnel = useCallback(() => {
    setTunnelSourceState('none');
    setTunnelIdState(null);
    setTunnelKindState('none');
    setTunnelError(null);
    setSshEnabledState(false);
    setSshHost('');
    setSshPort('22');
    setSshUsername('');
    setSshAuthMethod('password');
    setSshPassword('');
    setSshKeyPath('');
    setSshPassphrase('');
    setSshJumpEnabled(false);
    setSshJumpHost('');
    setSshJumpPort('22');
    setSshJumpUsername('');
    setSshJumpAuthMethod('password');
    setSshJumpPassword('');
    setSshJumpKeyPath('');
    setSshJumpPassphrase('');
    setHttpProxyHost('');
    setHttpProxyPort('8080');
    setHttpProxyScheme('http');
    setHttpProxyUsername('');
    setHttpProxyPassword('');
    setHttpProxyTimeout('30');
    setWsUrl('');
    setWsMode('datazen_v1');
    setWsAuthToken('');
    setWsTimeout('30');
  }, []);

  return {
    tunnelSource,
    setTunnelSource,
    tunnelKind,
    setTunnelKind,
    tunnelId,
    setTunnelId,
    savedTunnels: summaries,
    savedTunnel,
    tunnelRefMissing,
    tunnelInlineValid,
    tunnelBusy,
    tunnelError,
    effectiveTunnelKind,
    unbindTunnel,
    saveAsTunnel,
    hydrateTunnelRef,
    resetTunnel,
    sshEnabled,
    setSshEnabled,
    sshHost,
    setSshHost,
    sshPort,
    setSshPort,
    sshUsername,
    setSshUsername,
    sshAuthMethod,
    setSshAuthMethod,
    sshPassword,
    setSshPassword,
    sshKeyPath,
    setSshKeyPath,
    sshPassphrase,
    setSshPassphrase,
    sshJumpEnabled,
    setSshJumpEnabled,
    sshJumpHost,
    setSshJumpHost,
    sshJumpPort,
    setSshJumpPort,
    sshJumpUsername,
    setSshJumpUsername,
    sshJumpAuthMethod,
    setSshJumpAuthMethod,
    sshJumpPassword,
    setSshJumpPassword,
    sshJumpKeyPath,
    setSshJumpKeyPath,
    sshJumpPassphrase,
    setSshJumpPassphrase,
    httpProxyHost,
    setHttpProxyHost,
    httpProxyPort,
    setHttpProxyPort,
    httpProxyScheme,
    setHttpProxyScheme,
    httpProxyUsername,
    setHttpProxyUsername,
    httpProxyPassword,
    setHttpProxyPassword,
    httpProxyTimeout,
    setHttpProxyTimeout,
    wsUrl,
    setWsUrl,
    wsMode,
    setWsMode,
    wsAuthToken,
    setWsAuthToken,
    wsTimeout,
    setWsTimeout,
  };
}
