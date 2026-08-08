/**
 * Helpers for plugin stash / pre-commit fixture repos.
 */
import {
  mkdirSync,
  writeFileSync,
  readFileSync,
  existsSync,
  rmSync,
} from 'fs';
import { join, dirname } from 'path';
import { MANAGED_FILES } from '../plugin-file-stash.mjs';

export const CLEAN_CONTENTS: Record<string, string> = {
  'Cargo.toml': `[workspace]\n\n# <<plugin-patches>>\n# <</plugin-patches>>\n`,
  'src-tauri/Cargo.toml': `[package]\nname = "datazen"\n\n# <<plugin-dependencies>>\n# <</plugin-dependencies>>\n\n# <<plugin-features>>\n# <</plugin-features>>\n`,
  'src-tauri/src/plugin_init.rs': `// AUTO-GENERATED\n// Ensures plugin crates are linked into the binary (extern crate)\n\nuse tauri::Runtime;\n\npub fn register_plugins<R: Runtime>(builder: tauri::Builder<R>) -> tauri::Builder<R> {\n    let builder = builder;\n    // No plugins with Tauri commands enabled\n    builder\n}\n`,
  'src/plugins/generated.ts': `export type DatabaseType = never;\nexport type PluginDatabaseType = DatabaseType;\n\nexport const PLUGIN_COMMANDS = [];\n`,
  'src-tauri/capabilities/default.json': `{\n  "identifier": "default",\n  "description": "Default capability set for DataZen",\n  "windows": ["main", "connection-*"],\n  "permissions": [\n    "core:default",\n    "core:window:allow-set-decorations"\n  ]\n}\n`,
};

export const INJECTED_CONTENTS: Record<string, string> = {
  'Cargo.toml': `[workspace]\n\n# <<plugin-patches>>\n\n[patch."https://example.com/kiwi.git"]\ndatazen-plugin-kiwi = { path = ".plugins/kiwi" }\n# <</plugin-patches>>\n`,
  'src-tauri/Cargo.toml': `[package]\nname = "datazen"\n\n# <<plugin-dependencies>>\ndatazen-plugin-kiwi = { path = "../.plugins/kiwi", optional = true, features = ["tauri-plugin"] }\n# <</plugin-dependencies>>\n\n# <<plugin-features>>\nplugin-kiwi = ["dep:datazen-plugin-kiwi"]\n# <</plugin-features>>\n`,
  'src-tauri/src/plugin_init.rs': `// AUTO-GENERATED\n// Ensures plugin crates are linked into the binary (extern crate)\n\n#[cfg(feature = "plugin-kiwi")]\nextern crate datazen_plugin_kiwi;\nuse tauri::Runtime;\n\npub fn register_plugins<R: Runtime>(builder: tauri::Builder<R>) -> tauri::Builder<R> {\n    let builder = builder;\n    #[cfg(feature = "plugin-kiwi")]\n    let builder = builder.plugin(datazen_plugin_kiwi::init());\n    builder\n}\n`,
  'src/plugins/generated.ts': `export type DatabaseType = 'kiwi' | 'superset';\nexport type PluginDatabaseType = DatabaseType;\n\nexport const PLUGIN_COMMANDS = [\n  { pluginId: 'kiwi', commands: ['login', 'list_instances'] },\n];\n`,
  'src-tauri/capabilities/default.json': `{\n  "identifier": "default",\n  "description": "Default capability set for DataZen",\n  "windows": ["main", "connection-*"],\n  "permissions": [\n    "core:default",\n    "core:window:allow-set-decorations",\n    "kiwi:default"\n  ]\n}\n`,
};

export function writeManagedFiles(
  root: string,
  contents: Record<string, string> = CLEAN_CONTENTS,
) {
  for (const rel of MANAGED_FILES) {
    const full = join(root, rel);
    mkdirSync(dirname(full), { recursive: true });
    writeFileSync(full, contents[rel] ?? CLEAN_CONTENTS[rel]);
  }
}

export function readManaged(root: string, rel: string): string {
  return readFileSync(join(root, rel), 'utf-8');
}

export function managedExists(root: string, rel: string): boolean {
  return existsSync(join(root, rel));
}

export function resetDir(dir: string) {
  rmSync(dir, { recursive: true, force: true });
  mkdirSync(dir, { recursive: true });
}
