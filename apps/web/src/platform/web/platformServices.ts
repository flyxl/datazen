/**
 * Web `PlatformServices` (WR-E1 assembly + WR-E3 real halves) —
 * browser-native implementations.
 *
 * Mirrors `packages/frontend/src/platform/web/stub.ts`
 * (`createWebPlatformServices`) — §7.1's documented web degradations. The
 * stub in the shared package is not publicly exported (public-entry gate),
 * so the production web assembly owns this file, as the desktop form owns
 * `apps/desktop/src/platform/desktop/platformServices.ts`.
 *
 * WR-E3 rulings, all compile-time by construction (this file only exists in
 * the web host; nothing here probes the form factor):
 *
 * - **Save dialogs → Blob + `<a download>`** — the browser's own "save"
 *   semantics. There is no path prompt to compose: the download manager owns
 *   where the bytes land, and its cancel is a download-manager concern, not
 *   the dialog port's. Firing the anchor download therefore resolves `true`
 *   (the save was handed to the user); the port's `false` half exists for a
 *   cancelled path dialog this form never shows.
 * - **Open text → `<input type="file">` + FileReader** — the picker is the
 *   browser's open dialog; `change` (with a file) resolves the contents,
 *   the picker's `cancel` event resolves `null` (the port's cancelled half).
 *   A browser file handle is a Blob, never a host path, so `OpenedTextFile`
 *   stays path-free by construction.
 * - **Open directory / pickPath → `null`** — no browser API yields a host
 *   *path* (`showDirectoryPicker` yields a handle, and the port's
 *   `OpenedDirectory.path` is the one place a host path legitimately
 *   exists). Cancel semantics, per the port's documented web answer.
 * - **Clipboard → async Clipboard API**, legacy `execCommand` fallback when
 *   the API is absent or refuses; when both fail the error is the §13
 *   vocabulary's `CapabilityUnsupported` — never a silent no-op.
 * - Export does NOT live here: the `TablesExporterPort` is bound in
 *   `assembly.ts` to the shared Blob half (`createBlobTablesExporter`).
 *
 * One deliberate nuance, registered rather than hidden: `getOsPlatform`
 * sniffs the user agent. That is the port's *documented web semantics* for
 * the OS dimension (macos/windows/linux — orthogonal to the form factor,
 * `@datazen/backend-client` transport.ts), not a form-factor probe.
 */

import { ApiError, type PlatformServices } from '@datazen/backend-client';
import { downloadBlob } from '@datazen/frontend';

/** Decode a base64 payload into bytes (the web half of `SaveBinaryInput`). */
function base64ToBytes(dataBase64: string): Uint8Array<ArrayBuffer> {
  const binary = atob(dataBase64);
  const bytes = new Uint8Array(binary.length);
  for (let i = 0; i < binary.length; i += 1) {
    bytes[i] = binary.charCodeAt(i);
  }
  return bytes;
}

/** The legacy half of the clipboard write (execCommand needs a selection). */
function legacyCopy(text: string): boolean {
  const textarea = document.createElement('textarea');
  textarea.value = text;
  textarea.setAttribute('readonly', '');
  textarea.style.position = 'fixed';
  textarea.style.opacity = '0';
  document.body.appendChild(textarea);
  textarea.select();
  const copied = document.execCommand('copy');
  textarea.remove();
  return copied;
}

export function createWebPlatformServices(): PlatformServices {
  return {
    async saveTextWithDialog(input) {
      // The browser's save dialog IS the download flow (see the module
      // header): compose the text Blob and fire the anchor. `true` = the
      // save was handed to the user, the same shape the desktop dialog's
      // "saved" answer takes.
      downloadBlob(input.contents, input.defaultFileName, 'text/plain');
      return true;
    },
    async saveBinaryWithDialog(input) {
      downloadBlob(
        base64ToBytes(input.dataBase64),
        input.defaultFileName,
        'application/octet-stream',
      );
      return true;
    },
    async openTextWithDialog(input) {
      // `<input type="file">` is the browser's open dialog. The accept list
      // takes dotted or bare extensions; normalize to dotted.
      return new Promise((resolve, reject) => {
        const picker = document.createElement('input');
        picker.type = 'file';
        picker.accept = input.extensions
          .map((extension) => (extension.startsWith('.') ? extension : `.${extension}`))
          .join(',');
        let settled = false;
        const settle = (value: { fileName: string; content: string } | null) => {
          if (settled) return;
          settled = true;
          picker.remove();
          resolve(value);
        };
        picker.addEventListener('change', () => {
          const file = picker.files?.[0];
          if (!file) return settle(null);
          const reader = new FileReader();
          reader.onload = () =>
            settle({ fileName: file.name, content: String(reader.result ?? '') });
          reader.onerror = () => {
            if (settled) return;
            settled = true;
            picker.remove();
            // A read failure is not a cancel: surface the real error.
            reject(reader.error ?? new Error('openTextWithDialog: the file could not be read.'));
          };
          reader.readAsText(file);
        });
        // The picker's `cancel` event (Chromium/Firefox) — the port's
        // cancelled half: the user dismissed the open dialog.
        picker.addEventListener('cancel', () => settle(null));
        picker.click();
      });
    },
    async openDirectoryWithDialog() {
      // No host path exists in a browser tab (see the module header): the
      // port's cancelled half is the documented web answer.
      return null;
    },
    async writeClipboard(text) {
      // The async Clipboard API first; the legacy execCommand half when it is
      // absent or refuses; `CapabilityUnsupported` when both fail (§7.4 —
      // never a silent no-op).
      const clipboard = (navigator as Navigator | undefined)?.clipboard;
      if (clipboard) {
        try {
          await clipboard.writeText(text);
          return;
        } catch {
          // NotAllowedError without a user gesture — try the legacy half.
        }
      }
      if (!legacyCopy(text)) {
        throw new ApiError(
          'CapabilityUnsupported',
          'writeClipboard: the Clipboard API is unavailable and the execCommand fallback failed.',
          { unsupportedKind: 'capability' },
        );
      }
    },
    async writeClipboardHtml(input) {
      // The documented web degradation: `ClipboardItem` when the browser
      // offers it, plain text otherwise — never a silent no-op.
      type ClipboardItemCtor = new (item: Record<string, string>) => ClipboardItem;
      const ctor = (globalThis as { ClipboardItem?: ClipboardItemCtor }).ClipboardItem;
      if (ctor && typeof ClipboardItem !== 'undefined') {
        try {
          await navigator.clipboard.write([
            new ctor({ 'text/html': input.html, 'text/plain': input.altText }),
          ]);
          return;
        } catch {
          // NotAllowedError without a user gesture — fall through to plain text.
        }
      }
      await navigator.clipboard.writeText(input.altText);
    },
    async readClipboard() {
      // Reading has no legacy half (no execCommand for paste): absent API ⇒
      // an explicit capability gap, honest by §7.4.
      const clipboard = (navigator as Navigator | undefined)?.clipboard;
      if (!clipboard) {
        throw new ApiError(
          'CapabilityUnsupported',
          'readClipboard: the async Clipboard API is unavailable in this browser.',
          { unsupportedKind: 'capability' },
        );
      }
      return clipboard.readText();
    },
    async openExternal(input) {
      if (input.url) window.open(input.url, '_blank', 'noopener');
      // A local path has no browser semantics; the §7.1 contract makes this
      // the desktop-only half.
    },
    async toggleFullscreen() {
      // The document fullscreen API on the root element — the port's
      // documented web semantics (the desktop form toggles the OS window).
      if (document.fullscreenElement) {
        await document.exitFullscreen();
      } else {
        await document.documentElement.requestFullscreen();
      }
    },
    async showNotification(input) {
      // The Notification API is the browser-native degradation of the
      // desktop notification plugin. Permission denials reject — callers
      // already treat notification failure as best-effort.
      if (typeof Notification === 'undefined') {
        throw new Error('showNotification: the Notification API is unavailable in this browser.');
      }
      if (Notification.permission === 'default') {
        await Notification.requestPermission();
      }
      if (Notification.permission !== 'granted') {
        throw new Error('showNotification: notification permission was not granted.');
      }
      new Notification(input.title, { body: input.body });
    },
    async pickPath() {
      // No native path picker in a browser tab: cancel semantics (null), the
      // port's documented web answer. Callers gate the browse affordance on
      // capability, never on a probe.
      return null;
    },
    async getOsPlatform() {
      // The OS dimension still exists in a browser — the user agent answers
      // (documented web semantics of the port, not a form probe; see the
      // module header).
      const ua = navigator.userAgent.toLowerCase();
      if (ua.includes('mac')) return 'macos';
      if (ua.includes('win')) return 'windows';
      if (ua.includes('linux')) return 'linux';
      return 'unknown';
    },
  };
}
