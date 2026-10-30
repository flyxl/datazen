/**
 * Desktop assembly root (form-split F2 — the advance party of the future
 * `apps/desktop/src/main.tsx` platform part).
 *
 * Runs before the first React paint: binds the desktop backend, injects
 * `PlatformServices` into the backend-client registry (the production line
 * that shared-boundaries-and-ports.md §7.3 documented but was never wired),
 * configures the platform ports and registers the window chrome.
 *
 * In browser dev mode (plain `vite dev`, `TAURI_ENV_*` undefined) the same
 * bundle still runs: the backend stays unbound while the ports degrade
 * through their compile-time browser branches — identical to the legacy
 * `isTauri()` behavior, minus the runtime probing (ruling #5).
 */

import { bindDesktopBackend } from '../../platform/desktop/tauriBackendTransport';
import { createDesktopPlatformServices } from '../../platform/desktop/platformServices';
import { createDesktopWindowRouter } from '../../platform/desktop/windowRouter';
import { createDesktopEventChannel } from '../../platform/desktop/eventChannel';
import { createDesktopHostServices } from '../../platform/desktop/hostServices';
import { createDesktopSettingsClient } from '../../platform/desktop/settingsClient';
import { createDesktopTablesExporter } from '../../platform/desktop/tablesExporter';
import {
  IS_DESKTOP_BUILD,
  DESKTOP_CAPABILITIES,
  createBrowserEventChannel,
} from '@datazen/frontend';
import { setPlatformServices } from '@datazen/backend-client';
import { bindHostEventListener } from '@datazen/driver-sdk';
import { configurePlatform, requirePlatform, type DesktopHostServices } from '@datazen/frontend';
import { registerWindowChrome } from '@datazen/extension-points';
import { TitleBar } from './TitleBar';

/**
 * Desktop host overlay (port 4). Instantiated once here; exported for the
 * F4 settings migration. Shared code has no name for it (ruling #7).
 */
export const desktopHostServices: DesktopHostServices = createDesktopHostServices();

/** Bind backend + inject ports + register chrome. Idempotent per boot. */
export function assembleDesktopPlatform(): void {
  if (IS_DESKTOP_BUILD) {
    bindDesktopBackend();
  }
  const services = createDesktopPlatformServices();
  // The missing production wire (doc §7.3): port was built, never bound.
  setPlatformServices(services);
  configurePlatform({
    services,
    windows: createDesktopWindowRouter(),
    // Browser-dev runs of the desktop bundle bind the shared browser rail
    // (BroadcastChannel) — the desktop channel is now pure Tauri (F6 prep).
    events: IS_DESKTOP_BUILD ? createDesktopEventChannel() : createBrowserEventChannel(),
    // Form-split F4: the host half of the settings surface — the projecting
    // DesktopSettingsClient over the host settings pair. A web build binds
    // `inertHostSettingsPort()` instead (F5's assembly).
    hostSettings: createDesktopSettingsClient(desktopHostServices),
    // Form-split F4 (§3.3 dual-flow demonstration): the desktop export half —
    // Rust-side streaming through the save-dialog token writer. A web build
    // binds `createBlobTablesExporter(...)` under the same signature.
    tablesExporter: createDesktopTablesExporter(),
    // Form-split F5: the static capability table — sections, updater and
    // version source branch on this, never on a form probe.
    capabilities: DESKTOP_CAPABILITIES,
  });
  // Form-split F4 (R8 retirement): driver UI listens to host broadcasts
  // (redis-pubsub-message, ...) through the EventChannel port via the
  // driver-sdk bridge — the driver layer never imports the host runtime.
  bindHostEventListener((event, handler) =>
    requirePlatform().events.listenCrossWindow(event, handler),
  );
  registerWindowChrome(TitleBar);
}

/**
 * Sub-window show handshake: theme + splash were applied in the HTML; show
 * only after that paint, then focus. No-op outside the desktop build.
 */
export function showSubWindowWhenReady(): void {
  if (!IS_DESKTOP_BUILD) return;
  void import('@tauri-apps/api/window').then(({ getCurrentWindow }) => {
    const win = getCurrentWindow();
    if (win.label !== 'main') {
      // Theme + splash already applied in index.html; show only after that paint.
      requestAnimationFrame(() => {
        requestAnimationFrame(() => {
          void win
            .show()
            .then(() => {
              win.setFocus().catch(() => {});
            })
            .catch((e) => {
              console.error(`[bootstrap] failed to show window "${win.label}"`, e);
            });
        });
      });
    }
  });
}
