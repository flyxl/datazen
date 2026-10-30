/**
 * @datazen/frontend — package entry (form-split F6, host-boundary follow-up F2).
 *
 * The package's sanctioned surface is its narrow entries:
 *
 *   `.`            the light surface — platform ports & capability bits
 *                  (`configurePlatform`, `requirePlatform`, the contract
 *                  types, `DESKTOP_CAPABILITIES`, `WEB_CAPABILITIES`,
 *                  `IS_DESKTOP_BUILD`) plus the host-support helpers the
 *                  desktop adapters consume (updater variant rules, window
 *                  kind, browser event rail)
 *   `./app`        `startSharedApp` — the app bootstrap; owns the whole
 *                  shared app graph (App, stores, CSS, PRO table), kept out
 *                  of `.` so light consumers (and their tests) stay light
 *   `./shell`      the shell-facing slice for desktop chrome components
 *   `./styles.css` the global stylesheet
 *   `./testing/*`  deep test-only mirror of `src/` — allowed exclusively in
 *                  test files, enforced by
 *                  `scripts/check-frontend-public-entry.mjs`
 *
 * The old `./src/*` deep-import surface is gone; the guard script fails any
 * occurrence outside this package. Inside the package, modules import each
 * other relatively.
 */
export { DESKTOP_CAPABILITIES, WEB_CAPABILITIES } from './platform/capabilities';
export { IS_DESKTOP_BUILD } from './platform/env';
export * from './platform/contracts';
export { createBrowserEventChannel } from './lib/crossWindowBus';
export { getWindowKind, type WindowKind } from './lib/windowKind';
export {
  currentVariant,
  getUpdateChannel,
  isUpdaterSupported,
  manifestBelongsToBuild,
  readManifestVariant,
  type UpdateChannel,
} from './lib/updater';
export type { AppSettings } from './types/index';
