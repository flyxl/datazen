/**
 * Desktop `PlatformServices` implementation (form-split F2, absorbed from
 * `src/platform/tauriBackendTransport.ts` where it previously lived).
 *
 * Dialog and clipboard IPC is the only Tauri surface here; a web form
 * implements the same port with file inputs / Blob downloads / the async
 * clipboard API.
 */

import { invoke } from '@tauri-apps/api/core';
import type {
  OpenDirectoryInput,
  OpenExternalInput,
  OpenedDirectory,
  OpenedTextFile,
  OpenTextInput,
  PlatformServices,
  SaveBinaryInput,
  SaveTextInput,
} from '@datazen/backend-client';

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

    async openExternal(input: OpenExternalInput): Promise<void> {
      // `open_path` already handles both URLs (system browser) and local
      // directory paths — it is what the legacy `settingsCommands.openPath`
      // routed through.
      const target = input.url ?? input.path;
      if (!target) return;
      await invoke<void>('open_path', { path: target });
    },
  };
}
