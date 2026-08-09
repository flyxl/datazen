#!/usr/bin/env node
/**
 * resolve-drivers.mjs
 *
 * Pre-build script that resolves which database drivers to include.
 * Controlled via:
 *   --drivers="postgres,mysql"   (explicit registry names)
 *   --drivers="basic"            (postgres, mysql, sqlite, redis) — default when omitted
 *   --drivers="all"              (all path drivers only; excludes git drivers)
 *   --drivers="all,kiwi,superset"   (all / :all expands to all path drivers, then adds listed ids)
 *   --drivers="stub"             (empty selection; git-safe generated.ts / plugin_init)
 *   --drivers=                   (same as stub — explicit empty value)
 *   --drivers="postgres,mongodb,kiwi"  (explicit list; use this for custom SKUs)
 *   --restore                    (restore stashed clean managed files and exit)
 *
 * Environment variable: DATAZEN_DRIVERS="basic" (overrides default when no --drivers flag)
 *
 * Hard cutover: --plugins / DATAZEN_PLUGINS / presets none|core are rejected.
 * Custom release SKUs (e.g. akulaku) must pass an explicit comma list in CI —
 * do not add more named presets here.
 *
 * Managed files are copied into `.plugin-file-stash/` before injection,
 * then restored with `node scripts/plugin-file-stash.mjs restore` after build.
 */

import { readFileSync, writeFileSync, mkdirSync, existsSync } from 'fs';
import { resolve, dirname, join, relative } from 'path';
import { fileURLToPath, pathToFileURL } from 'url';
import { execSync } from 'child_process';
import {
  stashManagedFiles,
  restoreManagedFiles,
  managedReadPath,
  workPath,
  allStashed,
  ROOT as STASH_ROOT,
} from './plugin-file-stash.mjs';

const __dirname = dirname(fileURLToPath(import.meta.url));
const ROOT = STASH_ROOT;
const PLUGINS_DIR = resolve(ROOT, '.plugins');

function loadRegistry() {
  const raw = readFileSync(resolve(ROOT, 'drivers-registry.json'), 'utf-8');
  const registry = JSON.parse(raw);

  // Allow local development overrides via .drivers-dev.json (gitignored)
  const devOverridePath = resolve(ROOT, '.drivers-dev.json');
  if (existsSync(devOverridePath)) {
    const overrides = JSON.parse(readFileSync(devOverridePath, 'utf-8'));
    for (const [name, override] of Object.entries(overrides)) {
      if (registry[name]) {
        Object.assign(registry[name], override);
        console.log(`[resolve-drivers] dev override: ${name} → ${override.path || override.git}`);
      }
    }
  }

  return registry;
}

function parseArgs() {
  const args = process.argv.slice(2);

  const hasOldFlag = args.some((a) => a === '--plugins' || a.startsWith('--plugins='));
  if (hasOldFlag || process.env.DATAZEN_PLUGINS) {
    console.error(
      '[resolve-drivers] --plugins / DATAZEN_PLUGINS are no longer supported. Use --drivers=... or DATAZEN_DRIVERS.',
    );
    process.exit(1);
  }

  let drivers = null;
  let driversFlagSeen = false;
  for (const arg of args) {
    if (arg.startsWith('--drivers=')) {
      drivers = arg.slice('--drivers='.length);
      driversFlagSeen = true;
    }
  }

  if (!driversFlagSeen && process.env.DATAZEN_DRIVERS) {
    drivers = process.env.DATAZEN_DRIVERS;
  }

  // Default: basic (four core path drivers). Explicit `--drivers=` (empty) is stub.
  if (!driversFlagSeen && (drivers == null || drivers === '')) {
    drivers = 'basic';
  }
  if (driversFlagSeen && drivers === '') {
    drivers = 'stub';
  }

  return drivers;
}

const BASIC_DRIVERS = ['postgres', 'mysql', 'sqlite', 'redis'];

function pathDriverIds(registry) {
  return Object.entries(registry)
    .filter(([, entry]) => entry?.source === 'path')
    .map(([name]) => name);
}

/**
 * Resolve a --drivers / DATAZEN_DRIVERS value to registry ids.
 * Supports presets `basic` | `all` | `stub`, comma lists, and expanders `all` / `:all`
 * (all path drivers) so e.g. `all,kiwi,superset` includes git drivers without
 * baking them into the bare `all` preset alone.
 */
export function resolveDrivers(driversArg, registry) {
  if (driversArg === 'none' || driversArg === 'core') {
    console.error(
      `[resolve-drivers] preset "${driversArg}" is no longer supported. Use --drivers=basic for the four core drivers, or --drivers=stub for an empty git baseline.`,
    );
    process.exit(1);
  }

  // Empty / stub: commit-safe generated.ts (DatabaseType = never). Do not use as a runtime SKU.
  if (driversArg === 'stub' || driversArg === '') {
    return [];
  }

  if (driversArg === 'basic') {
    return [...BASIC_DRIVERS];
  }

  // Bare `all` remains path-only (same as expander below without extra ids).
  if (driversArg === 'all' || driversArg === ':all') {
    return pathDriverIds(registry);
  }

  const requested = driversArg.split(',').map((x) => x.trim()).filter(Boolean);
  const resolved = [];
  const seen = new Set();

  for (const token of requested) {
    let names;
    if (token === 'all' || token === ':all') {
      names = pathDriverIds(registry);
    } else if (token.startsWith(':')) {
      console.error(
        `[resolve-drivers] Unknown expander "${token}". Supported expanders: all, :all`,
      );
      process.exit(1);
    } else if (!registry[token]) {
      console.error(`[resolve-drivers] Unknown driver "${token}" — not in drivers-registry.json`);
      process.exit(1);
    } else {
      names = [token];
    }
    for (const name of names) {
      if (seen.has(name)) continue;
      seen.add(name);
      resolved.push(name);
    }
  }

  return resolved;
}

function generateCargoFeatures(drivers, registry) {
  return drivers
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
/** Protocol-reuse dbTypes → parent dbType used for composite badges when own SVG is missing. */
const DRIVER_ICON_PARENT = {
  questdb: 'postgresql',
  cloudberry: 'postgresql',
  doris: 'mysql',
  starrocks: 'mysql',
  manticore: 'mysql',
  ob_oracle: 'mysql',
};

function driverUiDirFromMetaPath(metaPath) {
  // metaPath like '../../packages/drivers/postgres/ui/meta' (from src/plugins)
  const absMetaTs = resolve(ROOT, 'src/plugins', `${metaPath}.ts`);
  return dirname(absMetaTs);
}

function resolveDriverIconImport(metaPath, dbTypeId) {
  const uiDir = driverUiDirFromMetaPath(metaPath);
  const abs = join(uiDir, 'icons', `${dbTypeId}.svg`);
  if (!existsSync(abs)) return null;
  // import path relative to src/plugins/generated.ts
  const relFromPlugins = relative(resolve(ROOT, 'src/plugins'), abs).replaceAll('\\', '/');
  const importPath = relFromPlugins.startsWith('.') ? relFromPlugins : `./${relFromPlugins}`;
  return { abs, importPath: `${importPath}?url`, fileKey: dbTypeId };
}

const BASIC_PATH_FRONTEND = {
  postgres: {
    dbTypes: [
      { id: 'postgresql', metaExport: 'postgresqlMeta' },
      { id: 'questdb', metaExport: 'questdbMeta' },
      { id: 'cloudberry', metaExport: 'cloudberryMeta' },
    ],
    metaPath: '../../packages/drivers/postgres/ui/meta',
  },
  mysql: {
    dbTypes: [
      { id: 'mysql', metaExport: 'mysqlMeta' },
      { id: 'mariadb', metaExport: 'mariadbMeta' },
      { id: 'doris', metaExport: 'dorisMeta' },
      { id: 'starrocks', metaExport: 'starrocksMeta' },
      { id: 'manticore', metaExport: 'manticoreMeta' },
      { id: 'ob_oracle', metaExport: 'obOracleMeta' },
    ],
    metaPath: '../../packages/drivers/mysql/ui/meta',
  },
  sqlite: {
    dbTypes: [{ id: 'sqlite', metaExport: 'sqliteMeta' }],
    metaPath: '../../packages/drivers/sqlite/ui/meta',
  },
  redis: {
    dbTypes: [{ id: 'redis', metaExport: 'redisMeta' }],
    metaPath: '../../packages/drivers/redis/ui/meta',
    connectionForm: {
      component: 'RedisConnectionWizard',
      path: '../../packages/drivers/redis/ui/ConnectionWizard',
      formVariant: 'redis',
      validator: { export: 'redisValidate' },
    },
    settings: {
      pluginId: 'redis',
      label: 'Redis',
      sectionExport: 'RedisSettingsSection',
      sectionPath: '../../packages/drivers/redis/ui/settings',
      schemaExport: 'redisSettingsSchema',
      schemaPath: '../../packages/drivers/redis/ui/settings',
    },
  },
  mongodb: {
    dbTypes: [{ id: 'mongodb', metaExport: 'mongodbMeta' }],
    metaPath: '../../packages/drivers/mongodb/ui/meta',
  },
  sqlserver: {
    dbTypes: [{ id: 'sqlserver', metaExport: 'sqlserverMeta' }],
    metaPath: '../../packages/drivers/sqlserver/ui/meta',
  },
  clickhouse: {
    dbTypes: [{ id: 'clickhouse', metaExport: 'clickhouseMeta' }],
    metaPath: '../../packages/drivers/clickhouse/ui/meta',
  },
  duckdb: {
    dbTypes: [{ id: 'duckdb', metaExport: 'duckdbMeta' }],
    metaPath: '../../packages/drivers/duckdb/ui/meta',
  },
  elasticsearch: {
    dbTypes: [{ id: 'elasticsearch', metaExport: 'elasticsearchMeta' }],
    metaPath: '../../packages/drivers/elasticsearch/ui/meta',
  },
  rqlite: {
    dbTypes: [{ id: 'rqlite', metaExport: 'rqliteMeta' }],
    metaPath: '../../packages/drivers/rqlite/ui/meta',
  },
  turso: {
    dbTypes: [{ id: 'turso', metaExport: 'tursoMeta' }],
    metaPath: '../../packages/drivers/turso/ui/meta',
  },
  influxdb: {
    dbTypes: [{ id: 'influxdb', metaExport: 'influxdbMeta' }],
    metaPath: '../../packages/drivers/influxdb/ui/meta',
  },
  victoriametrics: {
    dbTypes: [{ id: 'victoriametrics', metaExport: 'victoriametricsMeta' }],
    metaPath: '../../packages/drivers/victoriametrics/ui/meta',
  },
  hbase: {
    dbTypes: [{ id: 'hbase', metaExport: 'hbaseMeta' }],
    metaPath: '../../packages/drivers/hbase/ui/meta',
  },
  vector: {
    dbTypes: [{ id: 'vector', metaExport: 'vectorMeta' }],
    metaPath: '../../packages/drivers/vector/ui/meta',
  },
};

/**
 * Frontend driver configuration (path + git).
 */
const FRONTEND_DRIVER_CONFIG = {
  ...BASIC_PATH_FRONTEND,
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
  const iconImportLines = [];
  const dbEntryLines = [];
  const iconEntryLines = [];
  const iconParentEntryLines = [];
  const formEntryLines = [];
  const validatorEntryLines = [];
  const dialectEntryLines = [];
  const schemaTreeEntryLines = [];
  const settingsEntryLines = [];
  const pluginDbTypes = [];
  const iconImportByAbs = new Map();

  for (const id of plugins) {
    const cfg = FRONTEND_DRIVER_CONFIG[id];
    if (!cfg) continue;

    // Import meta exports
    const metaExports = cfg.dbTypes.map(dt => dt.metaExport).join(', ');
    importLines.push(`import { ${metaExports} } from '${cfg.metaPath}';`);

    // DB type entries
    for (const dt of cfg.dbTypes) {
      pluginDbTypes.push(dt.id);
      dbEntryLines.push(`  ${dt.id}: ${dt.metaExport},`);

      const resolved = resolveDriverIconImport(cfg.metaPath, dt.id);
      if (resolved) {
        let binding = iconImportByAbs.get(resolved.abs);
        if (!binding) {
          binding = `driverIcon_${resolved.fileKey.replace(/[^a-zA-Z0-9]/g, '_')}`;
          let n = binding;
          let i = 2;
          while ([...iconImportByAbs.values()].includes(n)) {
            n = `${binding}_${i++}`;
          }
          binding = n;
          iconImportByAbs.set(resolved.abs, binding);
          iconImportLines.push(`import ${binding} from '${resolved.importPath}';`);
        }
        iconEntryLines.push(`  'db.${dt.id}': ${binding},`);
      } else if (DRIVER_ICON_PARENT[dt.id]) {
        iconParentEntryLines.push(`  ${dt.id}: '${DRIVER_ICON_PARENT[dt.id]}',`);
      }
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

    // Plugin settings (Extensions UI)
    if (cfg.settings) {
      const s = cfg.settings;
      const settingsImports = new Set();
      if (s.sectionExport) settingsImports.add(s.sectionExport);
      if (s.schemaExport) settingsImports.add(s.schemaExport);
      const settingsPath = s.sectionPath || s.schemaPath;
      if (settingsPath && settingsImports.size > 0) {
        importLines.push(
          `import { ${[...settingsImports].join(', ')} } from '${settingsPath}';`,
        );
      }
      const entryParts = [
        `pluginId: '${s.pluginId}'`,
        `label: '${s.label.replace(/'/g, "\\'")}'`,
      ];
      if (s.sectionExport) {
        entryParts.push(`SettingsSection: ${s.sectionExport}`);
      }
      if (s.schemaExport) {
        entryParts.push(`schema: ${s.schemaExport}`);
      }
      settingsEntryLines.push(`  { ${entryParts.join(', ')} },`);
    }
  }

  const typeUnion = pluginDbTypes.length > 0
    ? pluginDbTypes.map(t => `'${t}'`).join(' | ')
    : 'never';

  // Plugin commands registry
  const pluginCommandLines = [];
  for (const id of plugins) {
    const cfg = FRONTEND_DRIVER_CONFIG[id];
    if (!cfg) continue;
    const registryEntry = JSON.parse(readFileSync(resolve(ROOT, 'drivers-registry.json'), 'utf-8'));
    const meta = registryEntry[id];
    if (meta?.tauriPlugin?.commands?.length > 0) {
      const cmds = meta.tauriPlugin.commands.map(c => `'${c}'`).join(', ');
      pluginCommandLines.push(
        `  { pluginId: '${meta.tauriPlugin.id}', commands: [${cmds}] },`
      );
    }
  }

  const content = `/**
 * AUTO-GENERATED by resolve-drivers.mjs — DO NOT EDIT MANUALLY
 *
 * This file registers frontend components and metadata for active plugins.
 * Regenerated every time the build runs with different --drivers args.
 */
${importLines.length > 0 ? importLines.join('\n') + '\n' : ''}${iconImportLines.length > 0 ? iconImportLines.join('\n') + '\n' : ''}import { invoke } from '@tauri-apps/api/core';
import type { DatabaseTypeMeta } from '@datazen/plugin-sdk';
import type { SqlDialectStrategy } from '@datazen/plugin-sdk';
import type { PluginFormValidator } from '@datazen/plugin-sdk';
import type { PluginSettingsContribution } from '@datazen/plugin-sdk';
import type { ComponentType } from 'react';

/**
 * Frontend plugin protocol version.
 * Must match the version expected by the main app.
 * Bump when making breaking changes to plugin interfaces.
 */
export const PLUGIN_PROTOCOL_VERSION = 1;

/** Database types contributed by active drivers in this build. */
export type DatabaseType = ${typeUnion};

/** Driver DB metadata entries (merged into DB_REGISTRY at runtime). */
export const DRIVER_DB_ENTRIES: Record<string, DatabaseTypeMeta> = {
${dbEntryLines.join('\n')}
};

/** Default driver badge icon URLs keyed by semantic id (\`db.<type>\`). */
export const DRIVER_ICON_ENTRIES: Record<string, string> = {
${iconEntryLines.join('\n')}
};

/** Protocol-reuse types without own badge SVG: parent dbType for composite badge. */
export const DRIVER_ICON_PARENTS: Record<string, string> = {
${iconParentEntryLines.join('\n')}
};

/** @deprecated Use DatabaseType */
export type PluginDatabaseType = DatabaseType;

/** @deprecated Use DRIVER_DB_ENTRIES */
export const PLUGIN_DB_ENTRIES = DRIVER_DB_ENTRIES;

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

// ===== Plugin Settings (Extensions UI) =====

/** Plugin-provided settings sections and/or JSON Schema forms. */
export const PLUGIN_SETTINGS_ENTRIES: PluginSettingsContribution[] = [
${settingsEntryLines.join('\n')}
];

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

  const outPath = workPath('src/plugins/generated.ts');
  mkdirSync(dirname(outPath), { recursive: true });
  writeFileSync(outPath, content);
  console.log(`[resolve-drivers] wrote ${outPath}`);
}

/**
 * Clone or update plugin repositories into .plugins/<name>/.
 * Supports git (clone from remote) and local (symlink from local path).
 * Git plugins may pin `ref` (commit SHA or tag) in drivers-registry.json.
 */
function checkoutGitRef(pluginDir, ref) {
  if (!ref) return;
  try {
    execSync(`git fetch --depth 1 origin ${ref}`, { cwd: pluginDir, stdio: 'pipe' });
  } catch {
    // Shallow fetch by SHA can fail on some hosts; fall back to full fetch.
    execSync('git fetch origin', { cwd: pluginDir, stdio: 'pipe' });
  }
  execSync(`git checkout --detach ${ref}`, { cwd: pluginDir, stdio: 'pipe' });
}

function cloneDrivers(drivers, registry) {
  mkdirSync(PLUGINS_DIR, { recursive: true });

  for (const name of drivers) {
    const meta = registry[name];
    if (meta.source === 'path' || meta.source === 'builtin') continue;

    const pluginDir = resolve(PLUGINS_DIR, name);

    if (meta.source === 'local' && meta.path) {
      const localPath = resolve(ROOT, meta.path);
      if (!existsSync(localPath)) {
        console.error(`[resolve-drivers] local plugin path not found: ${localPath}`);
        continue;
      }
      // Create symlink for local development
      if (existsSync(pluginDir)) {
        execSync(`rm -rf ${pluginDir}`, { stdio: 'pipe' });
      }
      execSync(`ln -s ${localPath} ${pluginDir}`, { stdio: 'pipe' });
      console.log(`[resolve-drivers] linked ${name} → ${localPath}`);
    } else if (meta.source === 'git' && meta.git) {
      const pinnedRef = meta.ref || meta.commit;
      if (existsSync(resolve(pluginDir, '.git'))) {
        console.log(`[resolve-drivers] updating ${name} ...`);
        try {
          execSync('git fetch origin', { cwd: pluginDir, stdio: 'pipe' });
        } catch {
          console.warn(`  [warn] git fetch failed for "${name}", using existing checkout`);
        }
        if (pinnedRef) {
          try {
            checkoutGitRef(pluginDir, pinnedRef);
            console.log(`  [resolve-drivers] ${name} checked out ${pinnedRef}`);
          } catch {
            console.warn(`  [warn] checkout ${pinnedRef} failed for "${name}", using HEAD`);
          }
        } else {
          try {
            execSync('git pull --ff-only', { cwd: pluginDir, stdio: 'pipe' });
          } catch {
            console.warn(`  [warn] git pull failed for "${name}", using existing checkout`);
          }
        }
      } else if (existsSync(resolve(pluginDir, 'Cargo.toml'))) {
        // Local development: directory exists with source but no .git — keep as-is
        console.log(`[resolve-drivers] using local ${name} (no .git, has Cargo.toml)`);
      } else {
        // Remove stale symlink or directory if source type changed
        if (existsSync(pluginDir)) {
          execSync(`rm -rf ${pluginDir}`, { stdio: 'pipe' });
        }
        console.log(`[resolve-drivers] cloning ${name} from ${meta.git} ...`);
        execSync(`git clone ${meta.git} ${pluginDir}`, { stdio: 'pipe' });
        if (pinnedRef) {
          checkoutGitRef(pluginDir, pinnedRef);
          console.log(`  [resolve-drivers] ${name} pinned to ${pinnedRef}`);
        }
      }
    }
  }
}

/**
 * Replace content between marker comments in a managed file.
 * Reads the clean baseline from stash (when present), writes to the working path.
 * Markers: `# --- BEGIN <tag> ---` / `# --- END <tag> ---`
 */
function replaceMarkerSection(relPath, tag, newContent) {
  const begin = `# --- BEGIN ${tag} (managed by resolve-drivers.mjs, do not edit) ---`;
  const end = `# --- END ${tag} ---`;
  const text = readFileSync(managedReadPath(relPath), 'utf-8');
  const re = new RegExp(
    escapeRegex(begin) + '[\\s\\S]*?' + escapeRegex(end),
  );
  if (!re.test(text)) {
    console.warn(`[resolve-drivers] marker "${tag}" not found in ${relPath}`);
    return;
  }
  const body = newContent ? `${begin}\n${newContent}\n${end}` : `${begin}\n${end}`;
  writeFileSync(workPath(relPath), text.replace(re, body));
}

function escapeRegex(s) {
  return s.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
}

/**
 * Update Cargo.toml files to include only resolved plugins.
 *
 * - src-tauri/Cargo.toml: adds optional path dependencies + feature flags
 * - Cargo.toml (root): adds [patch] entries when git-sourced plugins are
 *   cloned locally, so Cargo uses the local checkout instead of fetching
 */
function crateDepPath(name, meta) {
  if (meta.source === 'path' && meta.path) {
    // src-tauri/Cargo.toml → ../packages/drivers/X
    return `../${meta.path}`;
  }
  return `../.plugins/${name}`;
}

/** Path drivers use datazen-driver-*; git drivers keep their published crate name (usually datazen-plugin-*). */
function cratePackageName(name, meta) {
  if (meta?.source === 'path') {
    return `datazen-driver-${name}`;
  }
  return `datazen-plugin-${name}`;
}

function crateRustIdent(name, meta) {
  return cratePackageName(name, meta).replaceAll('-', '_');
}

function updateCargoFiles(plugins, registry) {
  // --- src-tauri/Cargo.toml: plugin deps ---
  const depLines = plugins.map(name => {
    const meta = registry[name];
    const crateName = cratePackageName(name, meta || {});
    const depPath = crateDepPath(name, meta || {});
    if (meta?.tauriPlugin) {
      return `${crateName} = { path = "${depPath}", optional = true, features = ["tauri-plugin"] }`;
    }
    return `${crateName} = { path = "${depPath}", optional = true }`;
  });
  replaceMarkerSection('src-tauri/Cargo.toml', 'PLUGIN DEPS', depLines.join('\n'));

  // --- src-tauri/Cargo.toml: plugin features ---
  const featureLines = plugins.map(name => {
    const meta = registry[name];
    const feature = meta?.feature;
    const crateName = cratePackageName(name, meta || {});
    return feature ? `${feature} = ["dep:${crateName}"]` : null;
  }).filter(Boolean);
  replaceMarkerSection('src-tauri/Cargo.toml', 'PLUGIN FEATURES', featureLines.join('\n'));

  // --- Root Cargo.toml: [patch] entries for git-sourced plugins ---
  const patchLines = plugins
    .filter(name => registry[name]?.source === 'git' && registry[name]?.git)
    .map(name => {
      const meta = registry[name];
      const crateName = cratePackageName(name, meta);
      return `[patch."${meta.git}"]\n${crateName} = { path = ".plugins/${name}" }`;
    });
  replaceMarkerSection('Cargo.toml', 'PLUGIN PATCHES', patchLines.join('\n\n'));

  console.log(`[resolve-drivers] updated Cargo.toml files (${plugins.length} plugin(s))`);
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
    const crateName = crateRustIdent(name, meta);

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

  const content = `// AUTO-GENERATED by resolve-drivers.mjs — DO NOT EDIT MANUALLY
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

  const outPath = workPath('src-tauri/src/plugin_init.rs');
  mkdirSync(dirname(outPath), { recursive: true });
  writeFileSync(outPath, content);
  console.log(`[resolve-drivers] wrote ${outPath}`);
}

/**
 * Replace content between start/end marker comments (idempotent).
 * Markers look like: # <<name>>  ...content...  # <</name>>
 */
function replaceMarkerBlock(content, name, lines) {
  const startTag = `# <<${name}>>`;
  const endTag = `# <</${name}>>`;
  const re = new RegExp(
    escapeRegex(startTag) + '[\\s\\S]*?' + escapeRegex(endTag),
  );
  const replacement = lines.length > 0
    ? `${startTag}\n${lines.join('\n')}\n${endTag}`
    : `${startTag}\n${endTag}`;
  return content.replace(re, replacement);
}

/**
 * Inject plugin dependencies and features into src-tauri/Cargo.toml at build time.
 * Uses start/end markers for idempotent injection.
 *
 * Plugins that declare `tauriPlugin` also enable the crate's `tauri-plugin` feature
 * so permissions/build.rs and init() are compiled in.
 */
function injectCargoToml(plugins, registry) {
  const relPath = 'src-tauri/Cargo.toml';
  let content = readFileSync(managedReadPath(relPath), 'utf-8');

  // Build dependency lines (path-based, optional)
  const depLines = [];
  for (const name of plugins) {
    const meta = registry[name];
    if (!meta.feature) continue;
    const crateName = cratePackageName(name, meta);
    const depPath = crateDepPath(name, meta);
    if (meta.tauriPlugin) {
      depLines.push(
        `${crateName} = { path = "${depPath}", optional = true, features = ["tauri-plugin"] }`,
      );
    } else {
      depLines.push(`${crateName} = { path = "${depPath}", optional = true }`);
    }
  }

  // Build feature lines
  const featureLines = [];
  for (const name of plugins) {
    const meta = registry[name];
    if (!meta.feature) continue;
    const crateName = cratePackageName(name, meta);
    featureLines.push(`${meta.feature} = ["dep:${crateName}"]`);
  }

  content = replaceMarkerBlock(content, 'plugin-dependencies', depLines);
  content = replaceMarkerBlock(content, 'plugin-features', featureLines);

  // Enable injected drivers by default so `tauri build --features webdriver` (and
  // other builds that only pass extra flags) still compile plugin crates and register
  // their Tauri ACL manifests (e.g. redis:default).
  const defaultFeatureNames = featureLines.map((line) => line.split('=')[0].trim());
  content = content.replace(
    /^default = \[[^\]]*\]/m,
    defaultFeatureNames.length > 0
      ? `default = [${defaultFeatureNames.map((f) => `"${f}"`).join(', ')}]`
      : 'default = []',
  );

  writeFileSync(workPath(relPath), content);
  console.log(`[resolve-drivers] injected ${depLines.length} deps + ${featureLines.length} features into Cargo.toml`);
}

/**
 * Sync Tauri ACL capabilities for plugin commands.
 *
 * Removes any permissions whose prefix matches a registry plugin id, then
 * appends `{tauriPlugin.id}:default` for each active plugin that exposes commands.
 * Idempotent across resolve-drivers runs.
 *
 * Preserves the committed file layout (compact `windows` array). No-ops when
 * the permissions list is already correct so --drivers=basic does not churn formatting.
 */
function syncPluginCapabilities(plugins, registry) {
  const relPath = 'src-tauri/capabilities/default.json';
  const readPath = managedReadPath(relPath);
  if (!existsSync(readPath)) {
    console.warn(`[resolve-drivers] capabilities file not found: ${relPath}`);
    return;
  }

  const before = readFileSync(readPath, 'utf-8');
  const cap = JSON.parse(before);
  if (!Array.isArray(cap.permissions)) {
    console.warn('[resolve-drivers] capabilities.permissions is not an array');
    return;
  }

  const pluginIds = new Set(
    Object.keys(registry).filter((name) => Boolean(registry[name]?.feature)),
  );

  const kept = cap.permissions.filter((entry) => {
    if (typeof entry !== 'string') return true;
    const prefix = entry.split(':')[0];
    return !pluginIds.has(prefix);
  });

  const added = [];
  for (const name of plugins) {
    const tp = registry[name]?.tauriPlugin;
    if (!tp?.id) continue;
    const perm = `${tp.id}:default`;
    if (!kept.includes(perm) && !added.includes(perm)) {
      added.push(perm);
    }
  }

  const nextPermissions = [...kept, ...added];
  // Always write working copy after stash (working path was renamed away).
  cap.permissions = nextPermissions;
  const windowsJson = `[${cap.windows.map((w) => JSON.stringify(w)).join(', ')}]`;
  const content = [
    `{`,
    `  "identifier": ${JSON.stringify(cap.identifier)},`,
    `  "description": ${JSON.stringify(cap.description)},`,
    `  "windows": ${windowsJson},`,
    `  "permissions": [`,
    cap.permissions.map((p) => `    ${JSON.stringify(p)}`).join(',\n'),
    `  ]`,
    `}`,
    ``,
  ].join('\n');
  writeFileSync(workPath(relPath), content);
  console.log(
    `[resolve-drivers] synced capabilities plugin permissions: [${added.join(', ') || 'none'}]`,
  );
}

/**
 * Inject [patch] entries into root Cargo.toml for plugins using git sources.
 * Uses start/end markers for idempotent injection.
 */
function injectRootCargoPatches(plugins, registry) {
  const relPath = 'Cargo.toml';
  let content = readFileSync(managedReadPath(relPath), 'utf-8');

  const patchLines = [];
  for (const name of plugins) {
    const meta = registry[name];
    if (meta.source !== 'git' || !meta.git) continue;
    const crateName = cratePackageName(name, meta);
    patchLines.push('');
    patchLines.push(`[patch."${meta.git}"]`);
    patchLines.push(`${crateName} = { path = ".plugins/${name}" }`);
  }

  content = replaceMarkerBlock(content, 'plugin-patches', patchLines);

  writeFileSync(workPath(relPath), content);
  if (patchLines.length > 0) {
    console.log(`[resolve-drivers] injected ${plugins.length} patch(es) into root Cargo.toml`);
  }
}

function wantsRestoreOnly() {
  return process.argv.slice(2).includes('--restore');
}

function main() {
  if (wantsRestoreOnly()) {
    restoreManagedFiles();
    return;
  }

  const registry = loadRegistry();
  const driversArg = parseArgs();

  console.log(`[resolve-drivers] drivers arg: "${driversArg}"`);

  const plugins = resolveDrivers(driversArg, registry);

  console.log(`[resolve-drivers] resolved drivers: [${plugins.join(', ')}]`);

  // Rename clean managed files aside, then write injected copies at original paths.
  // Idempotent: if an outer caller already stashed, re-inject over working copies.
  if (allStashed()) {
    console.log(
      '[resolve-drivers] stash already present; re-injecting without re-stash',
    );
  } else {
    stashManagedFiles();
  }

  try {
    // Clone/update git driver repos (path drivers use workspace members)
    cloneDrivers(plugins, registry);

    const features = generateCargoFeatures(plugins, registry);

    console.log(`[resolve-drivers] cargo features: [${features.join(', ')}]`);

    // Show plugin sources
    for (const name of plugins) {
      const meta = registry[name];
      if (meta.source === 'git') {
        console.log(`  ${name}: ${meta.git}`);
      } else if (meta.source === 'path' || meta.source === 'workspace') {
        console.log(`  ${name}: path (${meta.path})`);
      }
    }

    // Inject Cargo dependencies and features at build time
    injectCargoToml(plugins, registry);
    injectRootCargoPatches(plugins, registry);
    syncPluginCapabilities(plugins, registry);

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
    console.log(`[resolve-drivers] wrote ${outPath}`);

    generateFrontendRegistry(plugins);
    generateRustPluginInit(plugins, registry);

    // Also output to stdout for scripts that pipe this
    console.log(`\nCargo build command:`);
    if (features.length > 0) {
      console.log(`  cargo build --features "${features.join(',')}"`);
    } else {
      console.log(`  cargo build`);
    }
    console.log(`[resolve-drivers] managed files are injected; run \`node scripts/plugin-file-stash.mjs restore\` after build`);
  } catch (err) {
    console.error('[resolve-drivers] failed; attempting stash restore...');
    try {
      restoreManagedFiles();
    } catch (restoreErr) {
      console.error('[resolve-drivers] stash restore also failed:', restoreErr instanceof Error ? restoreErr.message : restoreErr);
    }
    throw err;
  }
}

if (process.argv[1] && import.meta.url === pathToFileURL(resolve(process.argv[1])).href) {
  main();
}
