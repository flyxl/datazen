/**
 * UI plugin host types.
 *
 * Mirrors `src-tauri/src/plugins/manifest.rs` (serde camelCase) and the
 * `PluginSummary` payload returned by the `list_plugins` IPC command
 * (`src-tauri/src/commands/plugins.rs`).
 */

/** API version handshake; must match `PLUGIN_API_VERSION` on the Rust side. */
export const EXTENSION_API_VERSION = 2;

/** Serialized strings of the Rust `Permission` enum (serde renames). */
export type PluginPermission =
  | 'context:connections'
  | 'command:invoke'
  | 'storage:local'
  | 'ui:notify';

/** Page contribution from a plugin manifest. */
export interface PageContribution {
  id: string;
  title: string;
  icon?: string;
  /** Only `workspace` is supported by this host. */
  showIn: string;
}

/** Theme contribution from a plugin manifest. */
export interface ThemeContribution {
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

export interface Contributions {
  pages: PageContribution[];
  themes: ThemeContribution[];
}

/** Full manifest as returned by `get_plugin_manifest`. */
export interface PluginManifest {
  id: string;
  name: string;
  version: string;
  apiVersion: number;
  author?: string | null;
  description?: string | null;
  /** Optional package-level icon (square brand image) shown in plugin lists. */
  icon?: string | null;
  entry?: string | null;
  contributes: Contributions;
  permissions: PluginPermission[];
  /** Reserved for P2 backend plugins; must be null/absent in v1. */
  backend?: unknown | null;
}

/** Page entry inside a `PluginSummary` payload. */
export interface PluginPageSummary {
  id: string;
  title: string;
  icon?: string;
}

/** Theme entry inside a `PluginSummary` payload. */
export interface PluginThemeSummary {
  id: string;
  name: string;
  modes: string[];
}

/** Installed-plugin row as returned by `list_plugins`. */
export interface PluginSummary {
  id: string;
  name: string;
  version: string;
  apiVersion: number;
  author?: string;
  description?: string;
  /** Optional package-level icon path (mirrors PluginManifest.icon). */
  icon?: string;
  enabled: boolean;
  permissions: PluginPermission[];
  pages: PluginPageSummary[];
  themes: PluginThemeSummary[];
}
