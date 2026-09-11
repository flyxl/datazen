/**
 * Helpers for extension stash / pre-commit fixture repos.
 */
import { mkdirSync, writeFileSync, readFileSync, existsSync, rmSync } from 'fs';
import { join, dirname } from 'path';
import { MANAGED_FILES } from '../driver-file-stash.mjs';

export const CLEAN_CONTENTS: Record<string, string> = {
  'Cargo.toml': `[workspace]\n\n# <<driver-patches>>\n# <</driver-patches>>\n`,
  'src-tauri/Cargo.toml': `[package]\nname = "datazen"\n\n# <<driver-dependencies>>\n# <</driver-dependencies>>\n\n# <<driver-features>>\n# <</driver-features>>\n`,
  'src-tauri/src/driver_init.rs': `// AUTO-GENERATED\n// Ensures driver crates are linked into the binary (extern crate)\n\nuse tauri::Runtime;\n\npub fn register_drivers<R: Runtime>(builder: tauri::Builder<R>) -> tauri::Builder<R> {\n    let builder = builder;\n    // No plugins with Tauri commands enabled\n    builder\n}\n`,
  'src/extensions/generated.ts': `export type DatabaseType = never;\n\nexport const DRIVER_COMMANDS = [];\n`,
  'src/extensions/generated-locales.ts': `export type DriverTranslationKey = never;\n\nexport const DRIVER_LOCALES = {\n  en: {},\n};\n`,
};

export const INJECTED_CONTENTS: Record<string, string> = {
  'Cargo.toml': `[workspace]\n\n# <<driver-patches>>\n\n[patch."https://example.com/kiwi.git"]\ndatazen-plugin-kiwi = { path = "packages/drivers/kiwi" }\n# <</driver-patches>>\n`,
  'src-tauri/Cargo.toml': `[package]\nname = "datazen"\n\n# <<driver-dependencies>>\ndatazen-plugin-kiwi = { path = "../packages/drivers/kiwi", optional = true, features = ["tauri-plugin"] }\n# <</driver-dependencies>>\n\n# <<driver-features>>\ndriver-kiwi = ["dep:datazen-plugin-kiwi"]\n# <</driver-features>>\n`,
  'src-tauri/src/driver_init.rs': `// AUTO-GENERATED\n// Ensures driver crates are linked into the binary (extern crate)\n\n#[cfg(feature = "driver-kiwi")]\nextern crate datazen_plugin_kiwi;\nuse tauri::Runtime;\n\npub fn register_drivers<R: Runtime>(builder: tauri::Builder<R>) -> tauri::Builder<R> {\n    let builder = builder;\n    #[cfg(feature = "driver-kiwi")]\n    let builder = builder.plugin(datazen_plugin_kiwi::init());\n    builder\n}\n`,
  'src/extensions/generated.ts': `export type DatabaseType = 'kiwi' | 'superset';\n\nexport const DRIVER_COMMANDS = [\n  { driverId: 'kiwi', commands: ['login', 'list_instances'] },\n];\n`,
  'src/extensions/generated-locales.ts': `export type DriverTranslationKey = 'redis.items';\n\nexport const DRIVER_LOCALES = {\n  en: { 'redis.items': 'Items' },\n};\n`,
};

export function writeManagedFiles(root: string, contents: Record<string, string> = CLEAN_CONTENTS) {
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
