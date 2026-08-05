#!/usr/bin/env node
/**
 * resolve-plugins.mjs
 *
 * Pre-build script that resolves which database driver plugins to include
 * in the DataZen build. Controlled via:
 *   --plugins="kiwi,olap"       (use preset registry names)
 *   --plugins="all"             (include all available plugins)
 *   --plugins="none"            (only built-in drivers: pg, mysql, sqlite, redis)
 *
 * Environment variable alternative: DATAZEN_PLUGINS="kiwi,olap"
 *
 * This script:
 * 1. Clones/updates plugin repos into .plugins/
 * 2. Generates Cargo feature flags for the Rust build
 * 3. Generates src/plugins/generated.ts for frontend integration
 */

import { readFileSync, writeFileSync, mkdirSync, existsSync } from 'fs';
import { resolve, dirname } from 'path';
import { fileURLToPath } from 'url';
import { execSync } from 'child_process';

const __dirname = dirname(fileURLToPath(import.meta.url));
const ROOT = resolve(__dirname, '..');
const PLUGINS_DIR = resolve(ROOT, '.plugins');

function loadRegistry() {
  const raw = readFileSync(resolve(ROOT, 'plugins-registry.json'), 'utf-8');
  const registry = JSON.parse(raw);

  // Allow local development overrides via .plugins-dev.json (gitignored)
  const devOverridePath = resolve(ROOT, '.plugins-dev.json');
  if (existsSync(devOverridePath)) {
    const overrides = JSON.parse(readFileSync(devOverridePath, 'utf-8'));
    for (const [name, override] of Object.entries(overrides)) {
      if (registry[name]) {
        Object.assign(registry[name], override);
        console.log(`[resolve-plugins] dev override: ${name} → ${override.path || override.git}`);
      }
    }
  }

  return registry;
}

function parseArgs() {
  const args = process.argv.slice(2);
  let plugins = null;

  for (const arg of args) {
    if (arg.startsWith('--plugins=')) {
      plugins = arg.slice('--plugins='.length);
    }
  }

  if (!plugins && process.env.DATAZEN_PLUGINS) {
    plugins = process.env.DATAZEN_PLUGINS;
  }

  // Default: all plugins
  if (!plugins) {
    plugins = 'all';
  }

  return plugins;
}

function resolvePlugins(pluginsArg, registry) {
  if (pluginsArg === 'none') return [];
  if (pluginsArg === 'all') {
    return Object.entries(registry)
      .filter(([_, meta]) => meta.source !== 'builtin')
      .map(([id]) => id);
  }

  const requested = pluginsArg.split(',').map(s => s.trim()).filter(Boolean);
  const resolved = [];

  for (const name of requested) {
    if (registry[name]) {
      if (registry[name].source === 'builtin') {
        console.log(`  [skip] "${name}" is always included (builtin)`);
      } else {
        resolved.push(name);
      }
    } else {
      console.warn(`  [warn] Unknown plugin "${name}" — not in registry`);
    }
  }

  return resolved;
}

function generateCargoFeatures(plugins, registry) {
  return plugins
    .map(name => registry[name]?.feature)
    .filter(Boolean);
}

/**
 * Frontend plugin configuration.
 *
 * Each plugin declares:
 * - dbTypes: array of { id, metaExport } — database types provided by this plugin
 * - metaPath: path to the file exporting meta objects
 * - connectionForm: { component, path, formVariant } — custom connection form (optional)
 * - sqlDialects: array of { family, export, path } — SQL dialect strategies (optional)
 */
const FRONTEND_PLUGIN_CONFIG = {
  kiwi: {
    dbTypes: [{ id: 'kiwi', metaExport: 'kiwiMeta' }],
    metaPath: '../../.plugins/kiwi/ui/plugin-meta',
    connectionForm: {
      component: 'KiwiConnectionFields',
      path: '../../.plugins/kiwi/ui/KiwiConnectionFields',
      formVariant: 'kiwi',
    },
    sqlDialects: [],
  },
  olap: {
    dbTypes: [
      { id: 'presto', metaExport: 'prestoMeta' },
      { id: 'trino', metaExport: 'trinoMeta' },
    ],
    metaPath: '../../.plugins/olap/ui/plugin-meta',
    connectionForm: {
      component: 'CatalogConnectionFields',
      path: '../../.plugins/olap/ui/CatalogConnectionFields',
      formVariant: 'catalog',
    },
    sqlDialects: [
      { family: 'trino', export: 'trinoDialect', path: '../../.plugins/olap/ui/trinoDialect' },
    ],
  },
  superset: {
    dbTypes: [{ id: 'superset', metaExport: 'supersetMeta' }],
    metaPath: '../../.plugins/superset/ui/plugin-meta',
    connectionForm: {
      component: 'SupersetConnectionFields',
      path: '../../.plugins/superset/ui/SupersetConnectionFields',
      formVariant: 'superset',
      validator: { export: 'supersetValidate' },
    },
    schemaTree: {
      component: 'SupersetSchemaTree',
      path: '../../.plugins/superset/ui/SupersetSchemaTree',
      dbType: 'superset',
    },
    sqlDialects: [],
  },
};

function generateFrontendRegistry(plugins) {
  const importLines = [];
  const dbEntryLines = [];
  const formEntryLines = [];
  const validatorEntryLines = [];
  const dialectEntryLines = [];
  const schemaTreeEntryLines = [];
  const pluginDbTypes = [];

  for (const id of plugins) {
    const cfg = FRONTEND_PLUGIN_CONFIG[id];
    if (!cfg) continue;

    // Import meta exports
    const metaExports = cfg.dbTypes.map(dt => dt.metaExport).join(', ');
    importLines.push(`import { ${metaExports} } from '${cfg.metaPath}';`);

    // DB type entries
    for (const dt of cfg.dbTypes) {
      pluginDbTypes.push(dt.id);
      dbEntryLines.push(`  ${dt.id}: ${dt.metaExport},`);
    }

    // Connection form
    if (cfg.connectionForm) {
      const formImports = [cfg.connectionForm.component];
      if (cfg.connectionForm.validator) {
        formImports.push(cfg.connectionForm.validator.export);
        validatorEntryLines.push(
          `  ${cfg.connectionForm.formVariant}: ${cfg.connectionForm.validator.export},`
        );
      }
      importLines.push(
        `import { ${formImports.join(', ')} } from '${cfg.connectionForm.path}';`
      );
      formEntryLines.push(
        `  { formVariant: '${cfg.connectionForm.formVariant}', component: ${cfg.connectionForm.component} },`
      );
    }

    // Schema tree
    if (cfg.schemaTree) {
      importLines.push(
        `import { ${cfg.schemaTree.component} } from '${cfg.schemaTree.path}';`
      );
      schemaTreeEntryLines.push(
        `  { dbType: '${cfg.schemaTree.dbType}', component: ${cfg.schemaTree.component} },`
      );
    }

    // SQL dialects
    for (const dial of cfg.sqlDialects || []) {
      importLines.push(`import { ${dial.export} } from '${dial.path}';`);
      dialectEntryLines.push(`  ${dial.family}: ${dial.export},`);
    }
  }

  const typeUnion = pluginDbTypes.length > 0
    ? pluginDbTypes.map(t => `'${t}'`).join(' | ')
    : 'never';

  // Plugin commands registry
  const pluginCommandLines = [];
  for (const id of plugins) {
    const cfg = FRONTEND_PLUGIN_CONFIG[id];
    if (!cfg) continue;
    const registryEntry = JSON.parse(readFileSync(resolve(ROOT, 'plugins-registry.json'), 'utf-8'));
    const meta = registryEntry[id];
    if (meta?.tauriPlugin?.commands?.length > 0) {
      const cmds = meta.tauriPlugin.commands.map(c => `'${c}'`).join(', ');
      pluginCommandLines.push(
        `  { pluginId: '${meta.tauriPlugin.id}', commands: [${cmds}] },`
      );
    }
  }

  const content = `/**
 * AUTO-GENERATED by resolve-plugins.mjs — DO NOT EDIT MANUALLY
 *
 * This file registers frontend components and metadata for active plugins.
 * Regenerated every time the build runs with different --plugins args.
 */
${importLines.length > 0 ? importLines.join('\n') + '\n' : ''}import { invoke } from '@tauri-apps/api/core';
import type { DatabaseTypeMeta } from '@datazen/plugin-sdk';
import type { SqlDialectStrategy } from '@datazen/plugin-sdk';
import type { PluginFormValidator } from '@datazen/plugin-sdk';
import type { ComponentType } from 'react';

/**
 * Frontend plugin protocol version.
 * Must match the version expected by the main app.
 * Bump when making breaking changes to plugin interfaces.
 */
export const PLUGIN_PROTOCOL_VERSION = 1;

/** Database types contributed by active plugins. */
export type PluginDatabaseType = ${typeUnion};

/** Plugin DB metadata entries (merged into DB_REGISTRY at runtime). */
export const PLUGIN_DB_ENTRIES: Record<string, DatabaseTypeMeta> = {
${dbEntryLines.join('\n')}
};

/** Plugin-provided SQL dialect strategies (merged into DIALECTS). */
export const PLUGIN_SQL_DIALECTS: Record<string, SqlDialectStrategy> = {
${dialectEntryLines.join('\n')}
};

/** Plugin-provided connection form components. */
interface PluginFormEntry {
  formVariant: string;
  component: ComponentType<any>;
}

const PLUGIN_FORMS: PluginFormEntry[] = [
${formEntryLines.join('\n')}
];

/** Lookup plugin-provided connection form by form variant (e.g. 'kiwi', 'catalog'). */
export function getPluginConnectionForm(formVariant: string): ComponentType<any> | undefined {
  for (const entry of PLUGIN_FORMS) {
    if (entry.formVariant === formVariant) {
      return entry.component;
    }
  }
  return undefined;
}

/** Plugin-provided form validators, keyed by form variant. */
const PLUGIN_VALIDATORS: Record<string, PluginFormValidator> = {
${validatorEntryLines.join('\n')}
};

/** Lookup plugin-provided form validator by form variant. */
export function getPluginValidator(formVariant: string): PluginFormValidator | undefined {
  return PLUGIN_VALIDATORS[formVariant];
}

// ===== Plugin Schema Trees =====

interface PluginSchemaTreeEntry {
  dbType: string;
  component: ComponentType<any>;
}

const PLUGIN_SCHEMA_TREES: PluginSchemaTreeEntry[] = [
${schemaTreeEntryLines.join('\n')}
];

/** Lookup plugin-provided schema tree by database type. */
export function getPluginSchemaTree(dbType: string): ComponentType<any> | undefined {
  for (const entry of PLUGIN_SCHEMA_TREES) {
    if (entry.dbType === dbType) {
      return entry.component;
    }
  }
  return undefined;
}

// ===== Plugin Commands =====

export interface PluginCommandMeta {
  pluginId: string;
  commands: string[];
}

/** Commands registered by active plugins via Tauri Plugin system. */
export const PLUGIN_COMMANDS: PluginCommandMeta[] = [
${pluginCommandLines.join('\n')}
];

/** Invoke a plugin-provided Tauri command using plugin:id|command format. */
export async function pluginInvoke<T = unknown>(
  pluginId: string,
  command: string,
  args?: Record<string, unknown>,
): Promise<T> {
  return invoke<T>(\`plugin:\${pluginId}|\${command}\`, args ?? {});
}

/** Check whether a specific plugin command is available in this build. */
export function hasPluginCommand(pluginId: string, command: string): boolean {
  return PLUGIN_COMMANDS.some(
    (p) => p.pluginId === pluginId && p.commands.includes(command),
  );
}
`;

  const outPath = resolve(ROOT, 'src/plugins/generated.ts');
  mkdirSync(dirname(outPath), { recursive: true });
  writeFileSync(outPath, content);
  console.log(`[resolve-plugins] wrote ${outPath}`);
}

/**
 * Clone or update plugin repositories into .plugins/<name>/.
 * Supports git (clone from remote) and local (symlink from local path).
 */
function clonePlugins(plugins, registry) {
  mkdirSync(PLUGINS_DIR, { recursive: true });

  for (const name of plugins) {
    const meta = registry[name];
    if (meta.source === 'builtin') continue;

    const pluginDir = resolve(PLUGINS_DIR, name);

    if (meta.source === 'local' && meta.path) {
      const localPath = resolve(ROOT, meta.path);
      if (!existsSync(localPath)) {
        console.error(`[resolve-plugins] local plugin path not found: ${localPath}`);
        continue;
      }
      // Create symlink for local development
      if (existsSync(pluginDir)) {
        execSync(`rm -rf ${pluginDir}`, { stdio: 'pipe' });
      }
      execSync(`ln -s ${localPath} ${pluginDir}`, { stdio: 'pipe' });
      console.log(`[resolve-plugins] linked ${name} → ${localPath}`);
    } else if (meta.source === 'git' && meta.git) {
      if (existsSync(resolve(pluginDir, '.git'))) {
        console.log(`[resolve-plugins] updating ${name} ...`);
        try {
          execSync('git pull --ff-only', { cwd: pluginDir, stdio: 'pipe' });
        } catch {
          console.warn(`  [warn] git pull failed for "${name}", using existing checkout`);
        }
      } else if (existsSync(resolve(pluginDir, 'Cargo.toml'))) {
        // Local development: directory exists with source but no .git — keep as-is
        console.log(`[resolve-plugins] using local ${name} (no .git, has Cargo.toml)`);
      } else {
        // Remove stale symlink or directory if source type changed
        if (existsSync(pluginDir)) {
          execSync(`rm -rf ${pluginDir}`, { stdio: 'pipe' });
        }
        console.log(`[resolve-plugins] cloning ${name} from ${meta.git} ...`);
        execSync(`git clone --depth 1 ${meta.git} ${pluginDir}`, { stdio: 'pipe' });
      }
    }
  }
}

/**
 * Generate src-tauri/src/plugin_init.rs — Rust plugin Tauri-plugin registration.
 *
 * For each plugin that declares a `tauriPlugin` block in the registry,
 * generates a #[cfg(feature = "...")] guarded .plugin() call.
 */
function generateRustPluginInit(plugins, registry) {
  const externCrateLines = [];
  const pluginInitLines = [];

  for (const name of plugins) {
    const meta = registry[name];
    if (!meta.feature) continue;

    const feature = meta.feature;
    const crateName = `datazen_plugin_${name}`;

    externCrateLines.push(`#[cfg(feature = "${feature}")]`);
    externCrateLines.push(`extern crate ${crateName};`);

    if (meta.tauriPlugin) {
      const initFn = meta.tauriPlugin.initFn;
      pluginInitLines.push(`    #[cfg(feature = "${feature}")]`);
      pluginInitLines.push(`    let builder = builder.plugin(${initFn}());`);
      pluginInitLines.push('');
    }
  }

  const externBlock = externCrateLines.length > 0
    ? externCrateLines.join('\n') + '\n'
    : '';

  const body = pluginInitLines.length > 0
    ? pluginInitLines.join('\n')
    : '    // No plugins with Tauri commands enabled';

  const content = `// AUTO-GENERATED by resolve-plugins.mjs — DO NOT EDIT MANUALLY
//
// Ensures plugin crates are linked into the binary (extern crate)
// so that inventory-based driver registration takes effect.
// Also registers Tauri plugins for crates that declare a tauriPlugin block.

${externBlock}use tauri::Runtime;

pub fn register_plugins<R: Runtime>(builder: tauri::Builder<R>) -> tauri::Builder<R> {
    let builder = builder;

${body}

    builder
}
`;

  const outPath = resolve(ROOT, 'src-tauri/src/plugin_init.rs');
  writeFileSync(outPath, content);
  console.log(`[resolve-plugins] wrote ${outPath}`);
}

function main() {
  const registry = loadRegistry();
  const pluginsArg = parseArgs();

  console.log(`[resolve-plugins] plugins arg: "${pluginsArg}"`);

  const plugins = resolvePlugins(pluginsArg, registry);

  console.log(`[resolve-plugins] resolved plugins: [${plugins.join(', ')}]`);

  // Clone/update plugin repos
  clonePlugins(plugins, registry);

  const features = generateCargoFeatures(plugins, registry);

  console.log(`[resolve-plugins] cargo features: [${features.join(', ')}]`);

  // Show plugin sources
  for (const name of plugins) {
    const meta = registry[name];
    if (meta.source === 'git') {
      console.log(`  ${name}: ${meta.git}`);
    } else if (meta.source === 'workspace') {
      console.log(`  ${name}: local (${meta.path})`);
    }
  }

  // Write the features file for the build system to consume
  const output = {
    plugins,
    features,
    cargoArgs: features.length > 0
      ? `--features "${features.join(',')}"`
      : '',
  };

  const outPath = resolve(ROOT, '.plugin-features.json');
  writeFileSync(outPath, JSON.stringify(output, null, 2) + '\n');
  console.log(`[resolve-plugins] wrote ${outPath}`);

  generateFrontendRegistry(plugins);
  generateRustPluginInit(plugins, registry);

  // Also output to stdout for scripts that pipe this
  console.log(`\nCargo build command:`);
  if (features.length > 0) {
    console.log(`  cargo build --features "${features.join(',')}"`);
  } else {
    console.log(`  cargo build`);
  }
}

main();
