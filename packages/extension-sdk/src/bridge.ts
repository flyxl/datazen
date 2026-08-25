/**
 * Plugin-side typed RPC client over the host postMessage bridge (PRD §3).
 *
 * Runs inside the sandboxed iframe: no Tauri APIs, zero runtime dependencies.
 * Every request is a `{ch:'datazen-extension', type, reqId, target:'host', payload?}`
 * envelope posted to `window.parent` with a `'*'` targetOrigin (the frame has
 * an opaque origin, so nothing stricter can be expressed); responses are
 * trusted only when `event.source === parent` and are correlated by the
 * echoed `reqId`. Requests time out after {@link REQUEST_TIMEOUT_MS}; `.err`
 * responses become {@link ExtensionError}s carrying the wire error code.
 */
export const BRIDGE_CHANNEL = 'datazen-extension';

/** Must match `EXTENSION_API_VERSION` on the host (`src/types/plugin.ts`). */
export const EXTENSION_API_VERSION = 2;

/** Mirrors the host router deadline (`extensionBridge.ts`). */
export const REQUEST_TIMEOUT_MS = 30_000;

/** Wire error codes mirrored from the host router; never leak stack traces. */
export const BRIDGE_ERROR = {
  PERMISSION: 'E_PERMISSION',
  NOT_FOUND: 'E_NOT_FOUND',
  TIMEOUT: 'E_TIMEOUT',
  RATE_LIMIT: 'E_RATE_LIMIT',
  PLUGIN_DISABLED: 'E_PLUGIN_DISABLED',
  BAD_REQUEST: 'E_BAD_REQUEST',
  NOT_IMPLEMENTED: 'E_NOT_IMPLEMENTED',
  INTERNAL: 'E_INTERNAL',
} as const;

export type BridgeErrorCode = (typeof BRIDGE_ERROR)[keyof typeof BRIDGE_ERROR];

/** SDK-local failure codes that never appear on the wire. */
export const SDK_ERROR = {
  VERSION_MISMATCH: 'EXTENSION_VERSION_MISMATCH',
  DETACHED: 'EXTENSION_DETACHED',
} as const;

export type SdkErrorCode = (typeof SDK_ERROR)[keyof typeof SDK_ERROR];

export type ExtensionErrorCode = BridgeErrorCode | SdkErrorCode;

/** Typed rejection for every failed bridge interaction. */
export class ExtensionError extends Error {
  readonly code: ExtensionErrorCode;

  constructor(code: ExtensionErrorCode, message: string) {
    super(message);
    this.name = 'ExtensionError';
    this.code = code;
  }
}

interface RequestEnvelope<P = unknown> {
  ch: typeof BRIDGE_CHANNEL;
  type: string;
  reqId?: string;
  target: 'host';
  payload?: P;
}

interface OkEnvelope<P = unknown> {
  ch: typeof BRIDGE_CHANNEL;
  type: string;
  reqId?: string;
  target: 'host';
  ok: true;
  payload?: P;
}

interface ErrEnvelope {
  ch: typeof BRIDGE_CHANNEL;
  type: string;
  reqId?: string;
  target: 'host';
  ok: false;
  payload?: { code: string; message: string };
}

type ResponseEnvelope<P = unknown> = OkEnvelope<P> | ErrEnvelope;

/** Connection summary visible to plugins — whitelisted fields only. */
export interface ConnectionSummary {
  id: string;
  name: string;
  dbType: string;
}

export interface CommandInvokeRequest {
  /** Persistent connection id the command targets. */
  connectionId: string;
  command: string;
  args?: Record<string, unknown>;
}

export interface NotifyRequest {
  title: string;
  body?: string;
}

/** Resolved handshake state reported by the host in `host.ready`. */
export interface HostContext {
  apiVersion: number;
  locale: string;
  dark: boolean;
  tokens: Record<string, string>;
}

export interface ExtensionClient {
  /**
   * Perform the `plugin.ready` → `host.ready` handshake and resolve with the
   * host context. Rejects with `EXTENSION_VERSION_MISMATCH` when the host
   * speaks another protocol version, `E_TIMEOUT` when it never answers.
   * Idempotent after success.
   */
  ready(): Promise<HostContext>;

  /** Host requires the `context:connections` permission for these. */
  readonly context: {
    getConnections(): Promise<ConnectionSummary[]>;
    getActiveConnection(): Promise<ConnectionSummary | null>;
  };

  /** Driver Command API passthrough; result shape depends on the command. */
  readonly command: {
    invoke(request: CommandInvokeRequest): Promise<unknown>;
  };

  /** Host-side per-plugin KV storage (host requires `storage:local`). */
  readonly storage: {
    get<T = unknown>(key: string): Promise<T | null>;
    set(key: string, value: unknown): Promise<void>;
    remove(key: string): Promise<void>;
  };

  /** System notification; host rate limits to one shot per 5s. */
  notify(request: NotifyRequest): Promise<void>;

  /** Look up a string from the plugin's own locales bundle. */
  readonly i18n: {
    getString(key: string): Promise<string | null>;
  };

  /**
   * Remove the window listener, abort every pending request/handshake with
   * `EXTENSION_DETACHED` and put the client into a terminal detached state.
   */
  detach(): void;
}

export interface CreateClientOptions {
  /** Per-request deadline; defaults to {@link REQUEST_TIMEOUT_MS}. */
  timeoutMs?: number;
  /** Override for tests; defaults to `window.parent` per the sandbox contract. */
  parentWindow?: Window | null;
}

interface PendingEntry {
  type: string;
  resolve(value: unknown): void;
  reject(error: unknown): void;
  timer: ReturnType<typeof setTimeout>;
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}

function isResponseEnvelope(data: unknown): data is ResponseEnvelope {
  return (
    isRecord(data) &&
    data.ch === BRIDGE_CHANNEL &&
    typeof data.type === 'string' &&
    (data.ok === true || data.ok === false)
  );
}

/**
 * Create the bridge client. Attach once per plugin page:
 *
 * ```ts
 * const dz = createClient();
 * await dz.ready();
 * const rows = await dz.command.invoke({ connectionId, command: 'query', args: { sql } });
 * ```
 */
export function createClient(options: CreateClientOptions = {}): ExtensionClient {
  const timeoutMs = options.timeoutMs ?? REQUEST_TIMEOUT_MS;
  const parent =
    options.parentWindow !== undefined
      ? options.parentWindow
      : typeof window === 'undefined'
        ? null
        : window.parent;

  // Per-client nonce keeps reqIds unique across clients sharing one page
  // while staying monotonically increasing within this client.
  const nonce = Math.random().toString(36).slice(2, 10);

  let seq = 0;
  let detached = false;
  let context: HostContext | null = null;

  const pending = new Map<string, PendingEntry>();

  type HandshakeState = 'idle' | 'pending' | 'ready' | 'failed';
  let handshakeState: HandshakeState = 'idle';
  let handshakeTimer: ReturnType<typeof setTimeout> | undefined;
  let resolveReady: ((value: HostContext) => void) | null = null;
  let rejectReady: ((error: unknown) => void) | null = null;
  let handshakeFailure: unknown = null;

  function post(envelope: RequestEnvelope): void {
    // '*' targetOrigin is deliberate: opaque origin frames cannot name their
    // parent more precisely; the host authenticates us via event.source.
    parent?.postMessage(envelope, '*');
  }

  function finishHandshakeTimer(): void {
    if (handshakeTimer !== undefined) clearTimeout(handshakeTimer);
    handshakeTimer = undefined;
  }

  function failHandshake(error: unknown): void {
    if (handshakeState !== 'pending') return;
    finishHandshakeTimer();
    handshakeState = 'failed';
    handshakeFailure = error;
    rejectReady?.(error);
    resolveReady = null;
    rejectReady = null;
  }

  function settleHandshake(data: { payload?: unknown }): void {
    if (handshakeState !== 'pending') return;
    const payload = isRecord(data.payload) ? data.payload : {};
    if (payload.apiVersion !== EXTENSION_API_VERSION) {
      failHandshake(
        new ExtensionError(
          SDK_ERROR.VERSION_MISMATCH,
          `host bridge apiVersion ${String(payload.apiVersion)} is incompatible with SDK ${EXTENSION_API_VERSION}; update @datazen/extension-sdk`,
        ),
      );
      return;
    }
    context = {
      apiVersion: Number(payload.apiVersion),
      locale: typeof payload.locale === 'string' ? payload.locale : '',
      dark: payload.dark === true,
      tokens: isRecord(payload.tokens) ? (payload.tokens as Record<string, string>) : {},
    };
    finishHandshakeTimer();
    handshakeState = 'ready';
    resolveReady?.(context);
    resolveReady = null;
    rejectReady = null;
  }

  function ready(): Promise<HostContext> {
    if (detached) {
      return Promise.reject(new ExtensionError(SDK_ERROR.DETACHED, 'client detached'));
    }
    if (handshakeState === 'ready' && context) return Promise.resolve(context);
    if (handshakeState === 'failed') {
      return Promise.reject(
        handshakeFailure instanceof Error ? handshakeFailure : new Error('handshake failed'),
      );
    }
    if (handshakeState === 'pending') {
      return new Promise<HostContext>((resolve, reject) => {
        const prevResolve = resolveReady;
        const prevReject = rejectReady;
        resolveReady = (value) => {
          prevResolve?.(value);
          resolve(value);
        };
        rejectReady = (error) => {
          prevReject?.(error);
          reject(error);
        };
      });
    }

    handshakeState = 'pending';
    return new Promise<HostContext>((resolve, reject) => {
      resolveReady = resolve;
      rejectReady = reject;
      handshakeTimer = setTimeout(() => {
        failHandshake(
          new ExtensionError(BRIDGE_ERROR.TIMEOUT, `handshake timed out after ${timeoutMs}ms`),
        );
      }, timeoutMs);
      post({
        ch: BRIDGE_CHANNEL,
        type: 'plugin.ready',
        target: 'host',
        payload: { apiVersion: EXTENSION_API_VERSION },
      });
    });
  }

  function request<R>(type: string, payload?: unknown): Promise<R> {
    if (detached) {
      return Promise.reject(new ExtensionError(SDK_ERROR.DETACHED, 'client detached'));
    }
    if (!parent) {
      return Promise.reject(
        new ExtensionError(BRIDGE_ERROR.INTERNAL, `"${type}" unavailable outside an iframe`),
      );
    }
    seq += 1;
    const reqId = `${nonce}-${seq}`;
    return new Promise<R>((resolve, reject) => {
      const timer = setTimeout(() => {
        pending.delete(reqId);
        reject(
          new ExtensionError(BRIDGE_ERROR.TIMEOUT, `"${type}" timed out after ${timeoutMs}ms`),
        );
      }, timeoutMs);
      pending.set(reqId, { type, resolve, reject, timer });
      post({
        ch: BRIDGE_CHANNEL,
        type,
        reqId,
        target: 'host',
        ...(payload === undefined ? {} : { payload }),
      });
    });
  }

  function onMessage(event: MessageEvent): void {
    if (event.source !== parent) return; // anti-spoofing: host frames only
    const data: unknown = event.data;

    if (isRecord(data) && data.ch === BRIDGE_CHANNEL && data.type === 'host.ready') {
      settleHandshake(data);
      return;
    }

    if (!isResponseEnvelope(data)) return;
    const reqId = typeof data.reqId === 'string' ? data.reqId : '';
    const entry = pending.get(reqId);
    if (!entry) return; // late / duplicate / foreign response — ignore
    pending.delete(reqId);
    clearTimeout(entry.timer);
    if (data.ok) {
      entry.resolve(data.payload);
    } else {
      // Malformed host frames may omit or null the err payload entirely
      // (BUG-F8-01); read defensively so the listener can never throw and
      // the pending request always settles as ExtensionError(E_INTERNAL).
      const payload: { code?: unknown; message?: unknown } = isRecord(data.payload)
        ? data.payload
        : {};
      const code = payload.code;
      entry.reject(
        new ExtensionError(
          (typeof code === 'string' ? code : BRIDGE_ERROR.INTERNAL) as ExtensionErrorCode,
          (typeof payload.message === 'string' && payload.message) || entry.type,
        ),
      );
    }
  }

  if (typeof window !== 'undefined') {
    window.addEventListener('message', onMessage);
  }

  const client: ExtensionClient = {
    ready,

    context: {
      async getConnections() {
        const payload = await request<{ connections?: ConnectionSummary[] }>(
          'context.getConnections',
        );
        return Array.isArray(payload?.connections) ? payload.connections : [];
      },
      async getActiveConnection() {
        const payload = await request<{ connection?: ConnectionSummary | null }>(
          'context.getActiveConnection',
        );
        return payload?.connection ?? null;
      },
    },

    command: {
      invoke(req: CommandInvokeRequest) {
        // Host answers `{result: <execute_driver_command data>}`.
        return request<{ result?: unknown }>('command.invoke', req).then((r) => r?.result);
      },
    },

    storage: {
      async get<T>(key: string) {
        const payload = await request<{ value?: T | null }>('storage.get', { key });
        return payload?.value ?? null;
      },
      set(key, value) {
        return request('storage.set', { key, value }).then(() => undefined);
      },
      remove(key) {
        return request('storage.remove', { key }).then(() => undefined);
      },
    },

    notify(req: NotifyRequest) {
      return request('ui.notify', req).then(() => undefined);
    },

    i18n: {
      async getString(key: string) {
        const payload = await request<{ value?: string | null }>('i18n.getString', { key });
        return typeof payload?.value === 'string' ? payload.value : null;
      },
    },

    detach() {
      if (detached) return;
      detached = true;
      if (typeof window !== 'undefined') {
        window.removeEventListener('message', onMessage);
      }
      for (const [, entry] of pending) {
        clearTimeout(entry.timer);
        entry.reject(new ExtensionError(SDK_ERROR.DETACHED, `"${entry.type}" aborted: detached`));
      }
      pending.clear();
      failHandshake(new ExtensionError(SDK_ERROR.DETACHED, 'handshake aborted: detached'));
    },
  };

  return client;
}
