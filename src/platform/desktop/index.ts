/**
 * Desktop platform zone (form-split F2 isolation area — the advance party of
 * the future `apps/desktop/src/platform/`).
 *
 * This is the ONLY `src/` directory (besides `src/shell/desktop/`) allowed to
 * import `@tauri-apps/*` (gate R6) — and only the assembly root and the
 * sanctioned thin re-export shims may import anything from here (gate R7).
 */

export { IS_DESKTOP_BUILD } from './env';
export { createDesktopPlatformServices } from './platformServices';
export { createDesktopEventChannel, emitCrossWindow, listenCrossWindow } from './eventChannel';
export {
  createDesktopWindowRouter,
  windowChrome,
  // Legacy surface (re-exported through the src/lib/windowManager shim).
  WINDOW_CAPABILITY_LABEL_SAMPLES,
  hasOpenChildWindows,
  openBackupWindow,
  openConnectionWindow,
  openDashboardWindow,
  openDataSyncWindow,
  openDataTransferWindow,
  openDocsWindow,
  openMultiInstanceWindow,
  openNewConnectionDialog,
  openSchemaDiffWindow,
  openSettingsWindow,
  openSingletonWindow,
  openWorkflowWindow,
  emitMenuAction,
  PENDING_CONNECTION_KEY,
  POST_ONBOARDING_SAMPLE_KEY,
  type MigrationWindowPrefill,
  type PostOnboardingSample,
} from './windowRouter';
export { createDesktopHostServices } from './hostServices';
export { pickPath } from './pathPicker';
export {
  bindDesktopBackend,
  createDesktopBackendTransport,
  DESKTOP_BACKEND_ID,
} from './tauriBackendTransport';
export { usePlatform, getPlatformSync, type Platform } from './os';
