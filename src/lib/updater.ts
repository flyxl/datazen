/**
 * Thin re-export shim (form-split F2). The implementation moved to
 * `src/platform/desktop/updater.ts`; sanctioned bridge (gate R7), dies in F6
 * when the updater is app-only.
 */
export {
  checkForUpdates,
  currentVariant,
  downloadAndInstallUpdate,
  getUpdateChannel,
  hasUpdaterChannel,
  isUpdaterSupported,
  manifestBelongsToBuild,
  maybeCheckOnStartup,
  readManifestVariant,
  type UpdateChannel,
  type UpdateCheckResult,
  type UpdateProgress,
} from '../platform/desktop/updater';
