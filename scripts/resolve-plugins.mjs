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
};

function generateFrontendRegistry(plugins) {
  const importLines = [];
  const dbEntryLines = [];
  const formEntryLines = [];
  const dialectEntryLines = [];
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
      importLines.push(
        `import { ${cfg.connectionForm.component} } from '${cfg.connectionForm.path}';`
      );
      formEntryLines.push(
        `  { formVariant: '${cfg.connectionForm.formVariant}', component: ${cfg.connectionForm.component} },`
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

  const content = `/**
 * AUTO-GENERATED by resolve-plugins.mjs — DO NOT EDIT MANUALLY
 *
 * This file registers frontend components and metadata for active plugins.
 * Regenerated every time the build runs with different --plugins args.
 */
${importLines.length > 0 ? importLines.join('\n') + '\n' : ''}import type { DatabaseTypeMeta } from '@datazen/plugin-sdk';
import type { SqlDialectStrategy } from '@datazen/plugin-sdk';
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

  // Also output to stdout for scripts that pipe this
  console.log(`\nCargo build command:`);
  if (features.length > 0) {
    console.log(`  cargo build --features "${features.join(',')}"`);
  } else {
    console.log(`  cargo build`);
  }
}

main();
