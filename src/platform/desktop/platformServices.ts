/**
 * Desktop `PlatformServices` implementation (form-split F2, absorbed from
 * `src/platform/tauriBackendTransport.ts` where it previously lived).
 *
 * Dialog and clipboard IPC is the only Tauri surface here; a web form
 * implements the same port with file inputs / Blob downloads / the async
 * clipboard API.
 */

import { invoke } from '@tauri-apps/api/core';
import { getCurrentWindow } from '@tauri-apps/api/window';
import type {
  OpenDirectoryInput,
  OpenExternalInput,
  OpenedDirectory,
  OpenedTextFile,
  OpenTextInput,
  OsPlatform,
  PickPathInput,
  PlatformServices,
  SaveBinaryInput,
  SaveTextInput,
  ShowNotificationInput,
  WriteClipboardHtmlInput,
} from '@datazen/backend-client';

/** User-agent fallback, unchanged from the original os.ts probe failure leg. */
function platformFromUserAgent(): OsPlatform {
  const ua = navigator.userAgent.toLowerCase();
  if (ua.includes('mac')) return 'macos';
  if (ua.includes('win')) return 'windows';
  if (ua.includes('linux')) return 'linux';
  return 'unknown';
}

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

    async writeClipboardHtml(input: WriteClipboardHtmlInput): Promise<void> {
      await invoke<void>('write_clipboard_html', {
        html: input.html,
        altText: input.altText,
      });
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

    async toggleFullscreen(): Promise<void> {
      // The zone-local window handle: fullscreen is a window-control
      // capability, and the desktop zone is where `@tauri-apps/api/window`
      // may be imported (ruling #5 — compile-time form determination).
      const win = getCurrentWindow();
      const isFullscreen = await win.isFullscreen();
      await win.setFullscreen(!isFullscreen);
    },

    async showNotification(input: ShowNotificationInput): Promise<void> {
      // The notification plugin command surface — the bridge used to invoke
      // `plugin:notification|notify` directly; the port keeps that wire shape
      // while moving the import boundary into the zone.
      await invoke<void>('plugin:notification|notify', {
        options: { title: input.title, body: input.body },
      });
    },

    async pickPath(input?: PickPathInput): Promise<string | null> {
      // The host's PathPicker (absorbed from the F2 zone-local pathPicker.ts):
      // the single place in the app that opens a native path dialog. The
      // `{ multiple: false, ...options }` default matches what `PathInput`
      // did when the call still lived inside the design system.
      const { open } = await import('@tauri-apps/plugin-dialog');
      const selected = await open({ multiple: false, ...(input ?? {}) });
      return typeof selected === 'string' ? selected : null;
    },

    async getOsPlatform(): Promise<OsPlatform> {
      // The plugin-os probe (absorbed from os.ts); the user-agent fallback
      // matches the original failure leg so a missing plugin degrades, not
      // breaks.
      try {
        const { platform } = await import('@tauri-apps/plugin-os');
        const p = platform();
        if (p === 'macos') return 'macos';
        if (p === 'windows') return 'windows';
        if (p === 'linux') return 'linux';
        return 'unknown';
      } catch {
        // plugin-os not available, fall back to user agent
        return platformFromUserAgent();
      }
    },
  };
}
