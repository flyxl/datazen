/**
 * Host-side postMessage bridge for sandboxed UI wapp iframes (PRD §3).
 *
 * Envelope (both directions):
 *   { ch:'datazen-wapp', type, reqId?, target:'host', payload? }
 * Responses suffix the request type with `.ok` / `.err` and echo `reqId`.
 *
 * Security posture:
 * - Only messages whose `event.source === iframe.contentWindow` are trusted.
 * - Deny-by-default permissions: every routed API declares a required
 *   permission; a manifest without it is answered `E_PERMISSION` before any
 *   business logic runs.
 * - Per-iframe concurrency cap (E_RATE_LIMIT) and 30s timeout (E_TIMEOUT)
 *   around the host-internal promise.
 */
import { invoke } from '@tauri-apps/api/core';
import type { WappPermission } from '../types/wapp';
import { WAPP_API_VERSION } from '../types/wapp';
import { wappCommands } from '../commands/wapps';
import { driverCommands } from '../commands/driver';
import { connectionCommands } from '../commands/connection';
import { resolveWappString } from './wappI18n';
import { useConnectionStore } from '../stores/connectionStore';
import { useActiveConnectionStore } from '../stores/activeConnectionStore';
import { buildThemeSnapshot } from './themeTokens';

export const BRIDGE_CHANNEL = 'datazen-wapp';

export const REQUEST_TIMEOUT_MS = 30_000;
export const MAX_INFLIGHT_REQUESTS = 20;
export const NOTIFY_MIN_INTERVAL_MS = 5_000;

/** Wire error codes; never leak Rust error stacks — message only. */
export const BRIDGE_ERROR = {
  PERMISSION: 'E_PERMISSION',
  NOT_FOUND: 'E_NOT_FOUND',
  TIMEOUT: 'E_TIMEOUT',
  RATE_LIMIT: 'E_RATE_LIMIT',
  WAPP_DISABLED: 'E_WAPP_DISABLED',
  BAD_REQUEST: 'E_BAD_REQUEST',
  NOT_IMPLEMENTED: 'E_NOT_IMPLEMENTED',
  INTERNAL: 'E_INTERNAL',
} as const;

export type BridgeErrorCode = (typeof BRIDGE_ERROR)[keyof typeof BRIDGE_ERROR];

export interface BridgeErrorPayload {
  code: BridgeErrorCode;
  message: string;
}

export interface WappRequestEnvelope<P = unknown> {
  ch: string;
  type: string;
  reqId?: string;
  target: 'host';
  payload?: P;
}

/** Response for `{type}` requests: `{type}.ok` with echoed reqId. */
export interface WappOkEnvelope<P = unknown> {
  ch: string;
  type: string;
  reqId?: string;
  target: 'host';
  ok: true;
  payload?: P;
}

/** Response for `{type}` requests: `{type}.err` with `{code,message}`. */
export interface WappErrEnvelope {
  ch: string;
  type: string;
  reqId?: string;
  target: 'host';
  ok: false;
  payload: BridgeErrorPayload;
}

export type WappResponseEnvelope<P = unknown> = WappOkEnvelope<P> | WappErrEnvelope;

export type PluginRequestEnvelope<P = unknown> = WappRequestEnvelope<P>;
export type PluginOkEnvelope<P = unknown> = WappOkEnvelope<P>;
export type PluginErrEnvelope = WappErrEnvelope;
export type PluginResponseEnvelope<P = unknown> = WappResponseEnvelope<P>;

/**
 * Connection summary visible to wapps/extensions. Deliberately whitelisted — host,
 * port, username, password and every other credential-bearing field of
 * `ConnectionConfig` are physically absent from this shape.
 */
export interface BridgeConnectionSummary {
  id: string;
  name: string;
  dbType: string;
}

function toPublicConnection(config: {
  id: string;
  name: string;
  databaseType: string;
}): BridgeConnectionSummary {
  return { id: config.id, name: config.name, dbType: config.databaseType };
}

function okEnvelope<P>(ch: string, reqId: string | undefined, payload?: P): WappOkEnvelope<P> {
  return { ch, type: '', target: 'host', ok: true, reqId, payload };
}

function errEnvelope(
  ch: string,
  reqId: string | undefined,
  code: BridgeErrorCode,
  message: string,
): WappErrEnvelope {
  return {
    ch,
    type: '',
    target: 'host',
    ok: false,
    reqId,
    payload: { code, message },
  };
}

/** Attach `.ok`/`.err` suffix to the request type for the response envelope. */
function responseTypeOf(requestType: string, ok: boolean): string {
  return `${requestType}.${ok ? 'ok' : 'err'}`;
}

// ---------------------------------------------------------------------------
// Route table: API type → required permission (null = no permission needed).
// Deny-by-default: unknown types are not routable at all (E_NOT_FOUND).
// ---------------------------------------------------------------------------

const API_ROUTES: Record<string, WappPermission | null> = {
  'context.getConnections': 'context:connections',
  'context.getActiveConnection': 'context:connections',
  'command.invoke': 'command:invoke',
  'storage.get': 'storage:local',
  'storage.set': 'storage:local',
  'storage.remove': 'storage:local',
  'ui.notify': null,
  'i18n.getString': null,
};

/**
 * Own-property route lookup (BUG-F6-01): prototype members like
 * `constructor`/`toString` must be treated as unknown APIs (E_NOT_FOUND),
 * never resolve through the record's prototype chain to E_PERMISSION.
 */
function routeFor(type: string): WappPermission | null | undefined {
  return Object.prototype.hasOwnProperty.call(API_ROUTES, type) ? API_ROUTES[type] : undefined;
}

/** Driver commands extensions/wapps must never invoke even with `command:invoke`. */
export const WAPP_COMMAND_DENYLIST = new Set([
  'execute',
  'create_database',
  'drop_database',
  'create_schema',
  'drop_schema',
  'create_user',
  'drop_user',
  'grant_privileges',
  'revoke_privileges',
]);

/** Returns false when the command id is blocked for wapp invocation. */
export function isWappCommandAllowed(command: string): boolean {
  return !WAPP_COMMAND_DENYLIST.has(command);
}

interface CommandInvokePayload {
  /** Persistent connection id (wapp-visible protocol key). */
  connectionId: string;
  command: string;
  args?: Record<string, unknown>;
}

interface NotifyPayload {
  title: string;
  body?: string;
}

function asString(value: unknown): string | null {
  return typeof value === 'string' && value.length > 0 ? value : null;
}

/** Typed failure that maps 1:1 onto an `.err` response payload. */
class BridgeApiError extends Error {
  constructor(
    readonly code: BridgeErrorCode,
    message: string,
  ) {
    super(message);
  }
}

/**
 * Connections source for context APIs. Prefers the connection store cache
 * (`ConnectionConfig[]`, loaded on app start); falls back to a fresh IPC
 * fetch when the store has not completed its first load. Either way only
 * whitelisted fields ever reach the wapp.
 */
function loadConnections() {
  const state = useConnectionStore.getState();
  if (state.connectionsLoaded && !state.loading) {
    return Promise.resolve(state.connections);
  }
  return connectionCommands.getConnections();
}

async function handleGetConnections() {
  const configs = await loadConnections();
  return { connections: configs.map(toPublicConnection) };
}

async function handleGetActiveConnection() {
  const connected = Object.values(useActiveConnectionStore.getState().connections).find(
    (entry) => entry.status === 'connected',
  );
  if (!connected) return { connection: null };
  const configs = await loadConnections();
  const config = configs.find((c) => c.id === connected.connectionId);
  return { connection: config ? toPublicConnection(config) : null };
}

async function handleCommandInvoke(wappId: string, payload: unknown) {
  const p = (payload ?? {}) as Partial<CommandInvokePayload>;
  const connectionId = asString(p.connectionId);
  const command = asString(p.command);
  if (!connectionId || !command || (typeof p.args !== 'undefined' && typeof p.args !== 'object')) {
    throw new BridgeApiError(
      BRIDGE_ERROR.BAD_REQUEST,
      'command.invoke requires {connectionId, command, args?}',
    );
  }
  if (!isWappCommandAllowed(command)) {
    throw new BridgeApiError(
      BRIDGE_ERROR.PERMISSION,
      `Command '${command}' is not permitted for extensions`,
    );
  }
  // Audit trail without leaking argument contents into logs. The same line
  // lands in {dataDir}/logs/datazen.log via the wapp_audit_log / extension_audit_log command so
  // the webview console is not the only durable record.
  console.info(`[wapp:${wappId}] command.invoke ${command} via ${connectionId}`);
  wappCommands.auditLog(wappId, 'command.invoke', `${command} via ${connectionId}`);
  // Resolve the live db session for the persistent connection id.
  const entry = useActiveConnectionStore.getState().connections[connectionId];
  const dbSessionId = asString(entry?.dbSessionId);
  if (!dbSessionId || entry?.status !== 'connected') {
    throw new BridgeApiError(
      BRIDGE_ERROR.NOT_FOUND,
      `No active DB session for connection '${connectionId}' (connect in the host first)`,
    );
  }
  try {
    const result = await driverCommands.execute({
      dbSessionId,
      command,
      input: (p.args ?? {}) as Record<string, unknown>,
    });
    return { result: result.data };
  } catch (error) {
    // Unknown connectionId/command surface from the backend as rejection text.
    const message = error instanceof Error ? error.message : String(error ?? '');
    if (/not found|no such|unknown/i.test(message)) {
      throw new BridgeApiError(BRIDGE_ERROR.NOT_FOUND, message);
    }
    throw new Error(message);
  }
}

async function handleStorage(wappId: string, type: string, payload: unknown) {
  const p = (payload ?? {}) as { key?: unknown; value?: unknown };
  const key = asString(p.key);
  if (!key) {
    throw new BridgeApiError(BRIDGE_ERROR.BAD_REQUEST, `${type} requires a non-empty string key`);
  }
  switch (type) {
    case 'storage.get': {
      const value = await wappCommands.wappStorageGet(wappId, key);
      return { value: value ?? null };
    }
    case 'storage.set':
      await wappCommands.wappStorageSet(wappId, key, p.value);
      return {};
    default:
      await wappCommands.wappStorageRemove(wappId, key);
      return {};
  }
}

/**
 * System notification through the already-registered Tauri notification
 * plugin (`notification:default` capability). Rate limited per iframe to one
 * shot per {@link NOTIFY_MIN_INTERVAL_MS}.
 */
function showNotification(title: string, body?: string): Promise<void> {
  return invoke<void>('plugin:notification|notify', { options: { title, body } });
}

export interface AttachBridgeOptions {
  wappId?: string;
  /** Manifest-declared permissions; deny-by-default for anything missing. */
  permissions: WappPermission[];
  /** Locale reported in the handshake snapshot. */
  locale?: string;
  apiVersion?: number;
  timeoutMs?: number;
  maxInflight?: number;
  notifyCooldownMs?: number;
}

export type WappBridgeOptions = AttachBridgeOptions;

export interface WappBridgeHandle {
  detach(): void;
  /** Push a fresh theme.apply snapshot to the wapp iframe. */
  pushThemeSnapshot(): void;
}

function isEnvelope(data: unknown): data is WappRequestEnvelope {
  return (
    typeof data === 'object' &&
    data !== null &&
    (data as { ch?: unknown }).ch === BRIDGE_CHANNEL &&
    (data as { target?: unknown }).target === 'host' &&
    typeof (data as { type?: unknown }).type === 'string'
  );
}

export const isWappEnvelope = isEnvelope;

/**
 * Attach the RPC bridge to a wapp iframe. Returns a handle whose `detach`
 * removes the window listener; call it when the shell unmounts or reloads
 * the frame.
 */
export function attachBridge(
  iframe: HTMLIFrameElement,
  opts: AttachBridgeOptions,
): WappBridgeHandle {
  const wappId = opts.wappId || '';
  const {
    permissions,
    locale = typeof navigator !== 'undefined' ? navigator.language : 'en',
    apiVersion = WAPP_API_VERSION,
    timeoutMs = REQUEST_TIMEOUT_MS,
    maxInflight = MAX_INFLIGHT_REQUESTS,
    notifyCooldownMs = NOTIFY_MIN_INTERVAL_MS,
  } = opts;

  const granted = new Set(permissions);
  let inflight = 0;
  let lastNotifyAt = Number.NEGATIVE_INFINITY;
  const pendingTimers = new Set<ReturnType<typeof setTimeout>>();

  function respond(request: WappRequestEnvelope, response: WappResponseEnvelope): void {
    const contentWindow = iframe.contentWindow;
    if (!contentWindow) return;
    contentWindow.postMessage(
      {
        ...response,
        ch: request.ch || BRIDGE_CHANNEL,
        type: responseTypeOf(request.type, response.ok),
        reqId: request.reqId,
      },
      '*',
    );
  }

  function runHandler(
    request: WappRequestEnvelope,
    handler: () => Promise<unknown>,
  ): Promise<void> {
    return new Promise((resolve) => {
      let settled = false;
      const timer = setTimeout(() => {
        if (settled) return;
        settled = true;
        pendingTimers.delete(timer);
        respond(
          request,
          errEnvelope(
            request.ch,
            request.reqId,
            BRIDGE_ERROR.TIMEOUT,
            `handler timed out after ${timeoutMs}ms`,
          ),
        );
        resolve();
      }, timeoutMs);

      handler()
        .then((payload) => {
          if (settled) return;
          settled = true;
          clearTimeout(timer);
          pendingTimers.delete(timer);
          respond(request, okEnvelope(request.ch, request.reqId, payload));
        })
        .catch((error: unknown) => {
          if (settled) return;
          settled = true;
          clearTimeout(timer);
          pendingTimers.delete(timer);
          if (error instanceof BridgeApiError) {
            respond(request, errEnvelope(request.ch, request.reqId, error.code, error.message));
          } else {
            const message =
              error instanceof Error ? error.message : String(error ?? 'internal error');
            respond(
              request,
              errEnvelope(
                request.ch,
                request.reqId,
                BRIDGE_ERROR.INTERNAL,
                message.length > 500 ? `${message.slice(0, 500)}…` : message,
              ),
            );
          }
        })
        .finally(resolve);
    });
  }

  async function dispatch(request: WappRequestEnvelope): Promise<void> {
    // Deny-by-default gate first: permission and routability decisions never
    // consume concurrency quota and take precedence over rate limiting.
    const requiredPermission = routeFor(request.type);
    if (requiredPermission === undefined) {
      respond(
        request,
        errEnvelope(
          request.ch,
          request.reqId,
          BRIDGE_ERROR.NOT_FOUND,
          `unknown api "${request.type}"`,
        ),
      );
      return;
    }
    if (requiredPermission !== null && !granted.has(requiredPermission)) {
      respond(
        request,
        errEnvelope(
          request.ch,
          request.reqId,
          BRIDGE_ERROR.PERMISSION,
          `"${request.type}" needs permission "${requiredPermission}"`,
        ),
      );
      return;
    }

    if (inflight >= maxInflight) {
      respond(
        request,
        errEnvelope(
          request.ch,
          request.reqId,
          BRIDGE_ERROR.RATE_LIMIT,
          `more than ${maxInflight} unfinished requests`,
        ),
      );
      return;
    }

    inflight += 1;
    try {
      switch (request.type) {
        case 'context.getConnections':
          await runHandler(request, () => handleGetConnections());
          break;
        case 'context.getActiveConnection':
          await runHandler(request, () => handleGetActiveConnection());
          break;
        case 'command.invoke':
          await runHandler(request, () => handleCommandInvoke(wappId, request.payload));
          break;
        case 'storage.get':
        case 'storage.set':
        case 'storage.remove':
          await runHandler(request, () => handleStorage(wappId, request.type, request.payload));
          break;
        case 'ui.notify': {
          const p = (request.payload ?? {}) as Partial<NotifyPayload>;
          const title = asString(p.title);
          if (!title || (typeof p.body !== 'undefined' && typeof p.body !== 'string')) {
            respond(
              request,
              errEnvelope(
                request.ch,
                request.reqId,
                BRIDGE_ERROR.BAD_REQUEST,
                'ui.notify requires {title, body?}',
              ),
            );
            return;
          }
          const now = Date.now();
          if (now - lastNotifyAt < notifyCooldownMs) {
            respond(
              request,
              errEnvelope(
                request.ch,
                request.reqId,
                BRIDGE_ERROR.RATE_LIMIT,
                `notifications limited to one per ${notifyCooldownMs}ms`,
              ),
            );
            return;
          }
          lastNotifyAt = now;
          await runHandler(request, () => showNotification(title, p.body));
          break;
        }
        case 'i18n.getString': {
          const p = (request.payload ?? {}) as { key?: unknown };
          const key = typeof p.key === 'string' ? p.key : '';
          if (!key) {
            respond(
              request,
              errEnvelope(
                request.ch,
                request.reqId,
                BRIDGE_ERROR.BAD_REQUEST,
                'i18n.getString requires {key}',
              ),
            );
            return;
          }
          await runHandler(request, () =>
            resolveWappString(wappId, key, locale).then((value) => ({ key, value })),
          );
          break;
        }
        default:
          respond(
            request,
            errEnvelope(
              request.ch,
              request.reqId,
              BRIDGE_ERROR.NOT_FOUND,
              `unknown api "${request.type}"`,
            ),
          );
      }
    } finally {
      inflight -= 1;
    }
  }

  function onMessage(event: MessageEvent): void {
    if (event.source !== iframe.contentWindow) return;
    const data: unknown = event.data;
    if (!isEnvelope(data)) return;

    if (data.type === 'wapp.ready') {
      const contentWindow = iframe.contentWindow;
      if (!contentWindow) return;
      const snapshot = buildThemeSnapshot();
      contentWindow.postMessage(
        {
          ch: data.ch || BRIDGE_CHANNEL,
          type: 'host.ready',
          target: 'host',
          payload: {
            apiVersion,
            locale,
            dark: snapshot.dark,
            tokens: snapshot.tokens,
          },
        },
        '*',
      );
      return;
    }

    if (!data.reqId) return; // Requests require an id to be answerable.
    void dispatch(data);
  }

  window.addEventListener('message', onMessage);

  return {
    detach() {
      window.removeEventListener('message', onMessage);
      for (const timer of pendingTimers) clearTimeout(timer);
      pendingTimers.clear();
    },
    pushThemeSnapshot() {
      const contentWindow = iframe.contentWindow;
      if (!contentWindow) return;
      const snapshot = buildThemeSnapshot();
      contentWindow.postMessage(
        { ch: BRIDGE_CHANNEL, type: 'theme.apply', target: 'host', payload: snapshot },
        '*',
      );
    },
  };
}
