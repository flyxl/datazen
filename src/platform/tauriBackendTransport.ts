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

import { invoke } from '@tauri-apps/api/core';
import {
  createBackendClient,
  setBackendClient,
  type BackendClient,
  type BackendTransport,
  type MethodMap,
  type OpenDirectoryInput,
  type OpenedDirectory,
  type OpenedTextFile,
  type OpenTextInput,
  type PlatformServices,
  type SaveBinaryInput,
  type SaveTextInput,
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
  return method.includes('_')
    ? method
    : method.replace(/[A-Z]/g, (letter) => `_${letter.toLowerCase()}`);
}

/**
 * Build the desktop transport.
 *
 * The request object *is* the Tauri argument bag: its own fields are passed
 * through as the command's named arguments. That is what keeps every migrated
 * call byte-identical to the `invoke` it replaced — `call('execute_driver_command',
 * { request })` reaches Rust as `{ request }`, not as `{ payload: { request } }`,
 * because Tauri deserializes argument names, not an envelope.
 *
 * `subscribe` is intentionally absent: the current backend pushes events over a
 * Tauri `Channel`, which is not an `AsyncIterable`, and the facade turns a
 * missing `subscribe` into a `CapabilityUnsupported` rather than a `TypeError`.
 */
export function createDesktopBackendTransport(): BackendTransport {
  return {
    async call<K extends keyof MethodMap>(
      method: K,
      payload: MethodMap[K]['request'],
    ): Promise<MethodMap[K]['response']> {
      const command = toCommandName(method);
      // The request's own fields are the command's named arguments, so this
      // cast is a widening to Tauri's argument bag, not a shape change: the
      // object handed to `invoke` is the same object `call` received.
      return (await invoke(
        command,
        payload as Record<string, unknown>,
      )) as MethodMap[K]['response'];
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
      // §7.1 declares this capability, but no `open_directory_with_dialog`
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
 * Called by the app entry before first paint (§7.3), and lazily by the
 * transitional driver-sdk path. Idempotent by construction: a second call
 * replaces the binding, so tests can rebind freely.
 */
export function bindDesktopBackend(backendId: string = DESKTOP_BACKEND_ID): BackendClient {
  const client = createBackendClient(backendId, createDesktopBackendTransport());
  setBackendClient(backendId, client);
  return client;
}
