/**
 * Web `PlatformServices` (WR-E1) — browser-native halves.
 *
 * Mirrors `packages/frontend/src/platform/web/stub.ts`
 * (`createWebPlatformServices`) — §7.1's documented web degradations. The
 * stub in the shared package is not publicly exported (public-entry gate),
 * so the production web assembly owns this file, as the desktop form owns
 * `apps/desktop/src/platform/desktop/platformServices.ts`.
 *
 * WR-E3 turns the inert halves (dialogs, export) into real browser-native
 * flows; until then they answer the port's documented "no host" semantics:
 * dialog pairs resolve to cancelled (`false` / `null`), clipboard and
 * external open degrade to the Web APIs, and nothing probes the form factor.
 *
 * One deliberate nuance, registered rather than hidden: `getOsPlatform`
 * sniffs the user agent. That is the port's *documented web semantics* for
 * the OS dimension (macos/windows/linux — orthogonal to the form factor,
 * `@datazen/backend-client` transport.ts), not a form-factor probe: which
 * form this build is was decided at compile time (this file only exists in
 * the web host). Ruling #5 bans form probing; it does not ban the OS
 * dimension's documented web answer.
 */

import type { PlatformServices } from '@datazen/backend-client';

export function createWebPlatformServices(): PlatformServices {
  return {
    async saveTextWithDialog() {
      // A browser form composes its own download UI; the dialog port is inert
      // until then (WR-E3).
      return false;
    },
    async saveBinaryWithDialog() {
      return false;
    },
    async openTextWithDialog() {
      return null;
    },
    async openDirectoryWithDialog() {
      // No directory semantics in a browser.
      return null;
    },
    async writeClipboard(text) {
      await navigator.clipboard.writeText(text);
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
      return navigator.clipboard.readText();
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
