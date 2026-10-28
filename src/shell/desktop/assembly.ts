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
import { IS_DESKTOP_BUILD } from '../../platform/desktop/env';
import { setPlatformServices } from '@datazen/backend-client';
import { configurePlatform, type DesktopHostServices } from '../../platform/contracts';
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
    events: createDesktopEventChannel(),
    // capabilities: DESKTOP_CAPABILITIES — lands in F5.
  });
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
