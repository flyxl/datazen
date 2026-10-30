/**
 * Desktop platform zone (form-split F2 isolation area — the advance party of
 * the future `apps/desktop/src/platform/`).
 *
 * This is the ONLY `src/` directory (besides `src/shell/desktop/`) allowed to
 * import `@tauri-apps/*` (gate R6). After the F6 prep shim burn, only the
 * assembly root, the app entry and zone-local tests import anything from
 * here (gate R7) — the legacy re-export surface (windowManager's named
 * helpers, os, pathPicker, the transport fallback shim) moved back to the
 * shared modules that own it.
 */

export { IS_DESKTOP_BUILD } from '@datazen/frontend';
export { createDesktopPlatformServices } from './platformServices';
export { createDesktopEventChannel } from './eventChannel';
export {
  createDesktopWindowRouter,
  windowChrome,
  openSingletonWindow,
  openMultiInstanceWindow,
  hasOpenChildWindows,
} from './windowRouter';
export { createDesktopHostServices } from './hostServices';
export {
  bindDesktopBackend,
  createDesktopBackendTransport,
  DESKTOP_BACKEND_ID,
} from './tauriBackendTransport';
