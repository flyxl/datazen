/**
 * @datazen/extension-sdk — public entry point.
 *
 * - `createClient()` — typed postMessage RPC against the host bridge.
 * - Theme utilities — apply/observe host `theme.apply` snapshots (no React).
 * - Types — wire shapes shared with `src/lib/extensionBridge.ts`.
 *
 * The optional `useTheme` React hook lives at `@datazen/extension-sdk/react`
 * (react is an optional peer dependency): exporting it from here would force
 * every plugin bundle — including non-React ones — to link against React,
 * breaking the zero-runtime-dependency contract. Its result *type* is
 * re-exported below (type-only imports are erased, so this stays safe).
 */
export {
  BRIDGE_CHANNEL,
  BRIDGE_ERROR,
  REQUEST_TIMEOUT_MS,
  SDK_ERROR,
  EXTENSION_API_VERSION,
  ExtensionError,
  createClient,
} from './bridge';
export type {
  BridgeErrorCode,
  CommandInvokeRequest,
  ConnectionSummary,
  CreateClientOptions,
  HostContext,
  NotifyRequest,
  SdkErrorCode,
  ExtensionClient,
  ExtensionErrorCode,
} from './bridge';

export {
  DEFAULT_THEME_TOKENS,
  THEME_CHANGED_EVENT,
  applyThemeSnapshot,
  getThemeState,
  startThemeListener,
  subscribeTheme,
} from './theme';
export type { StartThemeListenerOptions, ThemeSnapshot, ThemeState } from './theme';

export type { UseThemeResult } from './react';
