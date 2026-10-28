/**
 * Web platform stub (form-split F5, plan §5.5 item 3).
 *
 * The compile-time same-shape guarantee: a web assembly **typechecks** against
 * every port the desktop assembly binds — `BackendTransport`,
 * `PlatformServices`, `WindowRouter`, `EventChannel`, `HostSettingsPort`,
 * `TablesExporterPort`, `PlatformCapabilities` — with zero host imports. The
 * day a port signature drifts in a way a web form cannot satisfy, this file
 * fails `typecheck` and the drift is caught before any browser build exists.
 *
 * Behavior here is deliberately **throw / inert / browser-native**:
 * - the backend transport throws `CapabilityUnsupported` on every method —
 *   there is no web backend yet, and a silent empty answer would be a lie;
 * - host capabilities are the inert ports (the F4 web shape);
 * - `openExternal` degrades to `window.open`, which is the port's documented
 *   web semantics (§7.1), not a probe;
 * - the export flow binds the Blob half of the F4 dual-flow demonstration.
 *
 * `assembleWebPlatformStub()` is the future `apps/web` assembly root's
 * skeleton: when the web form lands, the throws become a real HTTP transport
 * and the inert ports stay exactly as they are.
 */

import { ApiError, type BackendTransport, setPlatformServices } from '@datazen/backend-client';
import { configurePlatform, inertHostSettingsPort, type PlatformBindings } from '../contracts';
import { WEB_CAPABILITIES } from '../capabilities';
import { createBlobTablesExporter } from '../blobTablesExporter';

/** The all-throw backend transport: no web backend exists yet. */
export function createWebBackendTransportStub(): BackendTransport {
  return {
    async call(method) {
      throw new ApiError(
        'CapabilityUnsupported',
        `${String(method)} is not served by the web backend stub (form-split F5).`,
      );
    },
  };
}

/** Browser-native `PlatformServices`: §7.1's documented web degradations. */
export function createWebPlatformServices() {
  return {
    async saveTextWithDialog() {
      // A browser form composes its own download UI; the dialog port is inert
      // until then.
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
    async writeClipboard(text: string) {
      await navigator.clipboard.writeText(text);
    },
    async readClipboard() {
      return navigator.clipboard.readText();
    },
    async openExternal(input: { url?: string; path?: string }) {
      if (input.url) window.open(input.url, '_blank', 'noopener');
      // A local path has no browser semantics; the §7.1 contract makes this
      // the desktop-only half.
    },
  };
}

/** The web-shaped window router: no child windows, no chrome. */
export function createWebWindowRouter(): PlatformBindings['windows'] {
  return {
    openSingletonWindow() {
      // No multi-window topology in a browser tab.
    },
    openMultiInstanceWindow() {
      // No multi-window topology in a browser tab.
    },
    async focusMainWindow() {
      window.focus();
    },
    async hasOpenChildWindows() {
      return false;
    },
    windowChrome: {
      async isFullscreen() {
        return Boolean(document.fullscreenElement);
      },
      async onFullscreenChanged(cb: (fs: boolean) => void) {
        const handler = () => cb(Boolean(document.fullscreenElement));
        document.addEventListener('fullscreenchange', handler);
        return () => document.removeEventListener('fullscreenchange', handler);
      },
      async setBackgroundRootColor() {
        // The page background is CSS; the port is a no-op here.
      },
      async onCloseRequested() {
        // The browser owns the close button.
        return () => undefined;
      },
      async minimizeCurrent() {
        // No minimize semantics in a tab.
      },
      async closeCurrent() {
        window.close();
      },
    },
  };
}

/** Cross-window events: inert until the real web assembly wires a rail. */
export function createWebEventChannel(): PlatformBindings['events'] {
  return {
    async emitCrossWindow(event, payload) {
      // No receivers in the stub form; BroadcastChannel wiring lands with the
      // real web assembly.
      void event;
      void payload;
    },
    async listenCrossWindow(_event, _handler) {
      return () => undefined;
    },
  };
}

/**
 * Assemble the web platform stub: same ports, same capabilities table, same
 * bindings call the desktop assembly makes — with the web halves.
 */
export function assembleWebPlatformStub(): PlatformBindings {
  const services = createWebPlatformServices();
  setPlatformServices(services);

  const bindings: PlatformBindings = {
    services,
    windows: createWebWindowRouter(),
    events: createWebEventChannel(),
    hostSettings: inertHostSettingsPort(),
    tablesExporter: createBlobTablesExporter(async () => {
      throw new ApiError(
        'CapabilityUnsupported',
        'The web stub has no table reader; bind the backend transport first (form-split F5).',
      );
    }),
    capabilities: WEB_CAPABILITIES,
  };
  configurePlatform(bindings);
  return bindings;
}
