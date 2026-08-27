/**
 * Host-side postMessage bridge for sandboxed UI plugin iframes (PRD §3).
 *
 * Envelope (both directions):
 *   { ch:'datazen-extension', type, reqId?, target:'host', payload? }
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
import type { ExtensionPermission } from '../types/extension';
import { EXTENSION_API_VERSION } from '../types/extension';
import { extensionCommands } from '../commands/extensions';
import { driverCommands } from '../commands/driver';
import { connectionCommands } from '../commands/connection';
import { resolvePluginString } from './extensionI18n';
import { useConnectionStore } from '../stores/connectionStore';
import { useActiveConnectionStore } from '../stores/activeConnectionStore';
import { buildThemeSnapshot } from './themeTokens';

export const BRIDGE_CHANNEL = 'datazen-extension';

export const REQUEST_TIMEOUT_MS = 30_000;
export const MAX_INFLIGHT_REQUESTS = 20;
export const NOTIFY_MIN_INTERVAL_MS = 5_000;

/** Wire error codes; never leak Rust error stacks — message only. */
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

export interface BridgeErrorPayload {
  code: BridgeErrorCode;
  message: string;
}

export interface PluginRequestEnvelope<P = unknown> {
  ch: typeof BRIDGE_CHANNEL;
  type: string;
  reqId?: string;
  target: 'host';
  payload?: P;
}

/** Response for `{type}` requests: `{type}.ok` with echoed reqId. */
export interface PluginOkEnvelope<P = unknown> {
  ch: typeof BRIDGE_CHANNEL;
  type: string;
  reqId?: string;
  target: 'host';
  ok: true;
  payload?: P;
}

/** Response for `{type}` requests: `{type}.err` with `{code,message}`. */
export interface PluginErrEnvelope {
  ch: typeof BRIDGE_CHANNEL;
  type: string;
  reqId?: string;
  target: 'host';
  ok: false;
  payload: BridgeErrorPayload;
}

export type PluginResponseEnvelope<P = unknown> = PluginOkEnvelope<P> | PluginErrEnvelope;

/**
 * Connection summary visible to plugins. Deliberately whitelisted — host,
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

function okEnvelope<P>(reqId: string | undefined, payload?: P): PluginOkEnvelope<P> {
  return { ch: BRIDGE_CHANNEL, type: '', target: 'host', ok: true, reqId, payload };
}

function errEnvelope(
  reqId: string | undefined,
  code: BridgeErrorCode,
  message: string,
): PluginErrEnvelope {
  return {
    ch: BRIDGE_CHANNEL,
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

const API_ROUTES: Record<string, ExtensionPermission | null> = {
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
function routeFor(type: string): ExtensionPermission | null | undefined {
  return Object.prototype.hasOwnProperty.call(API_ROUTES, type) ? API_ROUTES[type] : undefined;
}

interface CommandInvokePayload {
  /** Persistent connection id (plugin-visible protocol key). */
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
 * whitelisted fields ever reach the plugin.
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

async function handleCommandInvoke(pluginId: string, payload: unknown) {
  const p = (payload ?? {}) as Partial<CommandInvokePayload>;
  const connectionId = asString(p.connectionId);
  const command = asString(p.command);
  if (!connectionId || !command || (typeof p.args !== 'undefined' && typeof p.args !== 'object')) {
    throw new BridgeApiError(
      BRIDGE_ERROR.BAD_REQUEST,
      'command.invoke requires {connectionId, command, args?}',
    );
  }
  // Audit trail without leaking argument contents into logs. The same line
  // lands in {dataDir}/logs/datazen.log via the extension_audit_log command so
  // the webview console is not the only durable record.
  console.info(`[extension:${pluginId}] command.invoke ${command} via ${connectionId}`);
  extensionCommands.auditLog(pluginId, 'command.invoke', `${command} via ${connectionId}`);
  // Resolve the live db session for the persistent connection id; when the
  // host has not tracked that connection, fall through with the raw id — the
  // backend's resolve_session accepts both shapes (dual-mode).
  const entry = useActiveConnectionStore.getState().connections[connectionId];
  const dbSessionId = asString(entry?.dbSessionId) ?? connectionId;
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

async function handleStorage(pluginId: string, type: string, payload: unknown) {
  const p = (payload ?? {}) as { key?: unknown; value?: unknown };
  const key = asString(p.key);
  if (!key) {
    throw new BridgeApiError(BRIDGE_ERROR.BAD_REQUEST, `${type} requires a non-empty string key`);
  }
  switch (type) {
    case 'storage.get': {
      const value = await extensionCommands.extensionStorageGet(pluginId, key);
      return { value: value ?? null };
    }
    case 'storage.set':
      await extensionCommands.extensionStorageSet(pluginId, key, p.value);
      return {};
    default:
      await extensionCommands.extensionStorageRemove(pluginId, key);
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
  pluginId: string;
  /** Manifest-declared permissions; deny-by-default for anything missing. */
  permissions: ExtensionPermission[];
  /** Locale reported in the handshake snapshot. */
  locale?: string;
  apiVersion?: number;
  timeoutMs?: number;
  maxInflight?: number;
  notifyCooldownMs?: number;
}

export interface ExtensionBridgeHandle {
  detach(): void;
  /** Push a fresh theme.apply snapshot to the plugin iframe. */
  pushThemeSnapshot(): void;
}

function isPluginEnvelope(data: unknown): data is PluginRequestEnvelope {
  return (
    typeof data === 'object' &&
    data !== null &&
    (data as { ch?: unknown }).ch === BRIDGE_CHANNEL &&
    (data as { target?: unknown }).target === 'host' &&
    typeof (data as { type?: unknown }).type === 'string'
  );
}

/**
 * Attach the RPC bridge to a plugin iframe. Returns a handle whose `detach`
 * removes the window listener; call it when the shell unmounts or reloads
 * the frame.
 */
export function attachBridge(
  iframe: HTMLIFrameElement,
  opts: AttachBridgeOptions,
): ExtensionBridgeHandle {
  const {
    pluginId,
    permissions,
    locale = typeof navigator !== 'undefined' ? navigator.language : 'en',
    apiVersion = EXTENSION_API_VERSION,
    timeoutMs = REQUEST_TIMEOUT_MS,
    maxInflight = MAX_INFLIGHT_REQUESTS,
    notifyCooldownMs = NOTIFY_MIN_INTERVAL_MS,
  } = opts;

  const granted = new Set(permissions);
  let inflight = 0;
  let lastNotifyAt = Number.NEGATIVE_INFINITY;
  const pendingTimers = new Set<ReturnType<typeof setTimeout>>();

  function respond(request: PluginRequestEnvelope, response: PluginResponseEnvelope): void {
    const contentWindow = iframe.contentWindow;
    if (!contentWindow) return;
    contentWindow.postMessage(
      {
        ...response,
        type: responseTypeOf(request.type, response.ok),
        reqId: request.reqId,
      },
      '*',
    );
  }

  function runHandler(
    request: PluginRequestEnvelope,
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
          respond(request, okEnvelope(request.reqId, payload));
        })
        .catch((error: unknown) => {
          if (settled) return;
          settled = true;
          clearTimeout(timer);
          pendingTimers.delete(timer);
          if (error instanceof BridgeApiError) {
            respond(request, errEnvelope(request.reqId, error.code, error.message));
          } else {
            const message =
              error instanceof Error ? error.message : String(error ?? 'internal error');
            respond(
              request,
              errEnvelope(
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

  async function dispatch(request: PluginRequestEnvelope): Promise<void> {
    // Deny-by-default gate first: permission and routability decisions never
    // consume concurrency quota and take precedence over rate limiting.
    const requiredPermission = routeFor(request.type);
    if (requiredPermission === undefined) {
      respond(
        request,
        errEnvelope(request.reqId, BRIDGE_ERROR.NOT_FOUND, `unknown api "${request.type}"`),
      );
      return;
    }
    if (requiredPermission !== null && !granted.has(requiredPermission)) {
      respond(
        request,
        errEnvelope(
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
          await runHandler(request, () => handleCommandInvoke(pluginId, request.payload));
          break;
        case 'storage.get':
        case 'storage.set':
        case 'storage.remove':
          await runHandler(request, () => handleStorage(pluginId, request.type, request.payload));
          break;
        case 'ui.notify': {
          const p = (request.payload ?? {}) as Partial<NotifyPayload>;
          const title = asString(p.title);
          if (!title || (typeof p.body !== 'undefined' && typeof p.body !== 'string')) {
            respond(
              request,
              errEnvelope(
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
              errEnvelope(request.reqId, BRIDGE_ERROR.BAD_REQUEST, 'i18n.getString requires {key}'),
            );
            return;
          }
          await runHandler(request, () =>
            resolvePluginString(pluginId, key, locale).then((value) => ({ key, value })),
          );
          break;
        }
        default:
          respond(
            request,
            errEnvelope(request.reqId, BRIDGE_ERROR.NOT_FOUND, `unknown api "${request.type}"`),
          );
      }
    } finally {
      inflight -= 1;
    }
  }

  function onMessage(event: MessageEvent): void {
    if (event.source !== iframe.contentWindow) return;
    const data: unknown = event.data;
    if (!isPluginEnvelope(data)) return;

    if (data.type === 'plugin.ready') {
      const contentWindow = iframe.contentWindow;
      if (!contentWindow) return;
      const snapshot = buildThemeSnapshot();
      contentWindow.postMessage(
        {
          ch: BRIDGE_CHANNEL,
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
