/**
 * Runtime Workspace App (Wapp) host types.
 *
 * Mirrors `src-tauri/src/wapps/manifest.rs` (serde camelCase) and the
 * `WappSummary` payload returned by the `list_wapps` IPC command
 * (`src-tauri/src/commands/wapps.rs`).
 */

/** API version handshake; must match `WAPP_API_VERSION` on the Rust side. */
export const WAPP_API_VERSION = 2;

/** Serialized strings of the Rust `Permission` enum (serde renames). */
export type WappPermission =
  | 'context:connections'
  | 'command:invoke'
  | 'storage:local'
  | 'ui:notify';

/** Page contribution from a wapp manifest. */
export interface WappPageContribution {
  id: string;
  title: string;
  icon?: string;
  /** Only `workspace` is supported by this host. */
  showIn: string;
}

/** Theme contribution from a wapp manifest. */
export interface WappThemeContribution {
  id: string;
  name: string;
  /** Package-relative path to the tokens CSS file. */
  tokensCss: string;
  /** Non-empty subset of `light` / `dark`. */
  modes: string[];
  previewImage?: string;
  /** Optional CodeMirror color overlay (legacy ThemePack `editor.json`). */
  editorJson?: string;
  /** Optional chart series palette (legacy `charts.json`). */
  chartsJson?: string;
  /**
   * Optional directory of semantic icon overrides named
   * `<semanticId>.svg|.webp|.png` (legacy `icons/`).
   */
  iconsDir?: string;
}

export interface WappContributions {
  pages: WappPageContribution[];
  themes: WappThemeContribution[];
}

/** Full manifest as returned by `get_wapp_manifest`. */
export interface WappManifest {
  id: string;
  name: string;
  version: string;
  apiVersion: number;
  author?: string | null;
  description?: string | null;
  /** Optional package-level icon (square brand image) shown in wapp lists. */
  icon?: string | null;
  entry?: string | null;
  contributes: WappContributions;
  permissions: WappPermission[];
  /** Reserved for P2 backend extensions; must be null/absent in v1. */
  backend?: unknown | null;
}

/** Page entry inside a `WappSummary` payload. */
export interface WappPageSummary {
  id: string;
  title: string;
  icon?: string;
}

/** Theme entry inside a `WappSummary` payload. */
export interface WappThemeSummary {
  id: string;
  name: string;
  modes: string[];
}

/** Installed wapp row as returned by `list_wapps`. */
export interface WappSummary {
  id: string;
  name: string;
  version: string;
  apiVersion: number;
  author?: string;
  description?: string;
  /** Optional package-level icon path (mirrors WappManifest.icon). */
  icon?: string;
  enabled: boolean;
  permissions: WappPermission[];
  pages: WappPageSummary[];
  themes: WappThemeSummary[];
}

// ---------------------------------------------------------------------------
// Backward-compatible aliases for legacy Extension naming
// ---------------------------------------------------------------------------
export const EXTENSION_API_VERSION = WAPP_API_VERSION;
export type ExtensionPermission = WappPermission;
export type PageContribution = WappPageContribution;
export type ThemeContribution = WappThemeContribution;
export type Contributions = WappContributions;
export type ExtensionManifest = WappManifest;
export type ExtensionPageSummary = WappPageSummary;
export type ExtensionThemeSummary = WappThemeSummary;
export type ExtensionSummary = WappSummary;
