/**
 * Thin re-export shim (form-split F2). The implementation moved to
 * `src/platform/desktop/windowRouter.ts` (the desktop `WindowRouter` port);
 * this bridge is sanctioned by gate R7 and dies in F6 when `src/` becomes
 * `packages/frontend/` and the window surface is consumed via the port only.
 */
export {
  WINDOW_CAPABILITY_LABEL_SAMPLES,
  hasOpenChildWindows,
  openBackupWindow,
  openConnectionWindow,
  openDashboardWindow,
  openDataSyncWindow,
  openDataTransferWindow,
  openDocsWindow,
  openNewConnectionDialog,
  openSchemaDiffWindow,
  openSettingsWindow,
  openWorkflowWindow,
  emitMenuAction,
  PENDING_CONNECTION_KEY,
  POST_ONBOARDING_SAMPLE_KEY,
  type MigrationWindowPrefill,
  type PostOnboardingSample,
} from '../platform/desktop/windowRouter';
