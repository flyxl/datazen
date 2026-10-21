/**
 * Desktop adapter for the `@datazen/backend-client` ports.
 *
 * This file, and only this file, is where `@tauri-apps/api/core` meets the
 * backend boundary. `packages/backend-client` is transport-agnostic by design
 * (architecture guard F-07), so the `invoke` calls, the `Channel` wiring and
 * the snake_case command names all live on the host side of the port.
 *
 * Nothing here decides anything. Each `MethodMap` key maps to a Tauri command
 * by name; payload and response shapes come from `MethodMap` in
 * `packages/backend-client/src/transport.ts`. That is the whole point — a
 * second backend (HTTP, in-memory) replaces this file and nothing above it.
 */

import { invoke, Channel } from '@tauri-apps/api/core';
import {
  createBackendClient,
  setBackendClient,
  type BackendClient,
  type EventEnvelope,
  type ConnectionEvent,
  type ApiErrorPayload,
  deserializeApiError,
  type BackendTransport,
  type MethodMap,
  type OpenDirectoryInput,
  type OpenedDirectory,
  type OpenedTextFile,
  type OpenTextInput,
  type PlatformServices,
  type SaveBinaryInput,
  type SaveTextInput,
  type UnsupportedMethodKind,
} from '@datazen/backend-client';

/**
 * The `backendId` every desktop build reports.
 *
 * It is a label on the facade, not a routing argument: `useBackendClient()`
 * resolves the same instance whichever backend was selected, and a browser
 * backend would register under its own id without changing a single call site.
 */
export const DESKTOP_BACKEND_ID = 'desktop';

/**
 * Map a `MethodMap` key to its Tauri command name.
 *
 * Keys are written camelCase for the future kernel surface and snake_case for
 * the legacy driver-command gateway that already exists on the Rust side, so
 * only the former is converted. Tauri maps the names in both directions, so
 * this is a naming convention and not a lookup table.
 */
function toCommandName(method: keyof MethodMap): string {
  const commandOverrides: Partial<Record<keyof MethodMap, string>> = {
    getSettings: 'get_shared_settings',
    saveSettings: 'save_shared_settings',
    listConnections: 'platform_list_profiles',
    createConnection: 'platform_create_profile',
    updateConnection: 'platform_update_profile',
    disableConnection: 'platform_disable_profile',
    readArtifact: 'read_artifact_chunk',
    // The table read is the *target-bound* command: it carries its own
    // context and manual handle instead of borrowing a session. The
    // session-scoped `get_table_data` still exists on the Rust side, but the
    // grid reads through the target-bound one, and the port has to name the
    // command the grid actually uses.
    getTableData: 'get_table_data_at_target',
  };
  if (commandOverrides[method]) return commandOverrides[method];
  return method.includes('_')
    ? method
    : method.replace(/[A-Z]/g, (letter) => `_${letter.toLowerCase()}`);
}

/**
 * Methods whose payload *is* the command's argument bag.
 *
 * The kernel-surface commands take a single `request` argument, so their
 * payload is nested under `request`. The commands below take **named**
 * arguments instead (`context`, `changes`, `plan`, `scope`, ...), so their
 * payload is the bag itself and nesting it would make the Rust side look for
 * a `request` field that does not exist.
 */
const FLAT_ARGUMENT_METHODS: ReadonlySet<keyof MethodMap> = new Set<keyof MethodMap>([
  'previewPendingChanges',
  'commitPendingChanges',
  'estimateTableRows',
  'listNamedViews',
  'saveNamedView',
  'deleteNamedView',
]);

/**
 * The table-data methods the desktop backend does not serve **yet**.
 *
 * The port registers every method the product has, so a method landing later
 * cannot slip past it unnoticed. This list is the other half of that
 * arrangement: it says which of them the *desktop* cannot serve today, so a
 * caller gets an explicit `CapabilityUnsupported` — with the method name and
 * the backend — instead of Tauri's bare "command not found".
 *
 * The names are the Rust commands of 07/08/09, which are not registered in
 * `src-tauri/src/bootstrap/run.rs` yet. That file registers exactly five
 * table-data commands: `get_table_data`, `get_table_data_at_target`,
 * `commit_query_result_cell`, `commit_pending_changes` and
 * `preview_pending_changes`. `src/platform/__tests__/tauriBackendTransport.tableData.test.ts`
 * cross-checks this list against that registration, so the day one of these
 * commands is registered, the test fails and asks for this entry to go.
 *
 * `notImplemented` — not `capability`: the desktop *can* serve these methods
 * and does not yet. A browser build's named views are a different fact (a
 * form factor with no host store), and the two must not be reported as one.
 */
const DESKTOP_UNSUPPORTED_METHODS: Partial<Record<keyof MethodMap, UnsupportedMethodKind>> = {
  estimateTableRows: 'notImplemented',
  resolveForeignKeyTargets: 'notImplemented',
  listNamedViews: 'notImplemented',
  saveNamedView: 'notImplemented',
  deleteNamedView: 'notImplemented',
};

/**
 * Build the desktop transport.
 *
 * The request object *is* the Tauri argument bag: its own fields are passed
 * through as the command's named arguments. That is what keeps every migrated
 * call byte-identical to the `invoke` it replaced — `call('execute_driver_command',
 * { request })` reaches Rust as `{ request }`, not as `{ payload: { request } }`,
 * because Tauri deserializes argument names, not an envelope.
 *
 * Channel deliveries are adapted to an iterator with independent subscription cleanup.
 */
export function createDesktopBackendTransport(): BackendTransport {
  return {
    unsupportedMethods: DESKTOP_UNSUPPORTED_METHODS,
    async call<K extends keyof MethodMap>(
      method: K,
      payload: MethodMap[K]['request'],
    ): Promise<MethodMap[K]['response']> {
      const command = toCommandName(method);
      const args =
        method === 'getPlatformIdentity'
          ? {}
          : method.includes('_') ||
              method === 'getJob' ||
              method === 'listJobs' ||
              FLAT_ARGUMENT_METHODS.has(method)
            ? payload
            : { request: payload ?? {} };
      // The request's own fields are the command's named arguments, so this
      // cast is a widening to Tauri's argument bag, not a shape change: the
      // object handed to `invoke` is the same object `call` received.
      const response = await invoke(command, args as Record<string, unknown>);
      if (method === 'readArtifact' && response && typeof response === 'object') {
        const chunk = response as { bytes?: number[] | Uint8Array };
        if (Array.isArray(chunk.bytes)) chunk.bytes = new Uint8Array(chunk.bytes);
      }
      return response as MethodMap[K]['response'];
    },
    subscribe(_method, request) {
      return {
        [Symbol.asyncIterator]() {
          const subscriptionId = crypto.randomUUID();
          type Message =
            | { kind: 'event'; event: EventEnvelope<ConnectionEvent> }
            | { kind: 'closed' }
            | { kind: 'error'; error: ApiErrorPayload };
          const channel = new Channel<Message>();
          const queue: EventEnvelope<ConnectionEvent>[] = [];
          let finished = false;
          let failure: unknown = null;
          let wake: (() => void) | null = null;
          const signal = () => {
            wake?.();
            wake = null;
          };
          channel.onmessage = (message) => {
            if (finished) return;
            if (message.kind === 'event') {
              if (queue.length >= 2048) {
                failure = new Error('Event buffer overflow; restore execution and reconnect.');
                finished = true;
              } else queue.push(message.event);
            } else {
              finished = true;
              if (message.kind === 'error') failure = deserializeApiError(message.error);
            }
            signal();
          };
          const started = invoke<void>('subscribe_events', {
            request,
            subscriptionId,
            onEvent: channel,
          }).catch((error: unknown) => {
            failure = error;
            finished = true;
            signal();
          });
          let stopped = false;
          return {
            async next(): Promise<IteratorResult<EventEnvelope<ConnectionEvent>>> {
              while (!queue.length && !finished)
                await new Promise<void>((resolve) => {
                  wake = resolve;
                });
              if (failure) throw failure;
              const value = queue.shift();
              return value ? { value, done: false } : { value: undefined, done: true };
            },
            async return(): Promise<IteratorResult<EventEnvelope<ConnectionEvent>>> {
              finished = true;
              queue.length = 0;
              signal();
              if (stopped) return { value: undefined, done: true };
              stopped = true;
              await started;
              await invoke<void>('stop_event_subscription', { subscriptionId });
              return { value: undefined, done: true };
            },
          };
        },
      };
    },
  };
}

/** Build the desktop `PlatformServices`, delegating to the Rust dialog/clipboard commands. */
export function createDesktopPlatformServices(): PlatformServices {
  return {
    saveTextWithDialog(input: SaveTextInput): Promise<boolean> {
      return invoke<boolean>('save_text_with_dialog', {
        contents: input.contents,
        defaultFileName: input.defaultFileName,
        filterName: input.filterName,
        extensions: input.extensions,
      });
    },

    saveBinaryWithDialog(input: SaveBinaryInput): Promise<boolean> {
      return invoke<boolean>('save_base64_with_dialog', {
        dataBase64: input.dataBase64,
        defaultFileName: input.defaultFileName,
        filterName: input.filterName,
        extensions: input.extensions,
      });
    },

    openTextWithDialog(input: OpenTextInput): Promise<OpenedTextFile | null> {
      return invoke<OpenedTextFile | null>('open_text_with_dialog', {
        filterName: input.filterName,
        extensions: input.extensions,
      });
    },

    openDirectoryWithDialog(_input: OpenDirectoryInput): Promise<OpenedDirectory | null> {
      // The capability is declared here, but no `open_directory_with_dialog`
      // command is registered on the Rust side yet. It is declared and not
      // implemented rather than quietly omitted, because the contract is what
      // future backends bind against; and it throws rather than invoking a
      // command that does not exist, so the gap surfaces at the call site
      // instead of as a runtime "command not found".
      return Promise.reject(
        new Error(
          'openDirectoryWithDialog is declared by PlatformServices but the desktop backend has not registered it yet.',
        ),
      );
    },

    async writeClipboard(text: string): Promise<void> {
      await invoke<void>('write_clipboard', { text });
    },

    readClipboard(): Promise<string> {
      return invoke<string>('read_clipboard');
    },
  };
}

/**
 * Bind the desktop backend and select it.
 *
 * Called by the app entry before first paint, and lazily by the
 * transitional driver-sdk path. Idempotent by construction: a second call
 * replaces the binding, so tests can rebind freely.
 */
export function bindDesktopBackend(backendId: string = DESKTOP_BACKEND_ID): BackendClient {
  const client = createBackendClient(backendId, createDesktopBackendTransport());
  setBackendClient(backendId, client);
  return client;
}
