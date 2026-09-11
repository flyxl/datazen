#!/usr/bin/env node
/**
 * resolve-drivers.mjs
 *
 * Pre-build script that resolves which database drivers to include.
 * Controlled via:
 *   --drivers="postgres,mysql"   (explicit registry names)
 *   --drivers="basic"            (postgres, mysql, sqlite, redis) — default when omitted
 *   --drivers="all"              (all path drivers only; excludes git drivers)
 *   --drivers="basic,kiwi,superset"  (basic / :basic expands core path drivers, then adds listed ids)
 *   --drivers="all,kiwi,superset"   (all / :all expands to all path drivers, then adds listed ids)
 *   --drivers="stub"             (empty selection; empty generated.ts / driver_init)
 *   --drivers=                   (same as stub — explicit empty value)
 *   --drivers="postgres,mongodb,kiwi"  (explicit list; use this for custom SKUs)
 *   --drivers="kiwi" / --drivers="superset"  (single git driver id from the registry)
 *   --codegen-only               (write generated.ts / locales / driver_init.rs only;
 *                                 do not inject Cargo.toml or capabilities)
 *   --restore                    (restore stashed clean managed files and exit)
 *
 * Environment variable: DATAZEN_DRIVERS="basic" (overrides default when no --drivers flag)
 *
 * Hard cutover: --plugins / DATAZEN_PLUGINS / presets none|core are rejected.
 * Custom release SKUs (e.g. akulaku) must pass an explicit comma list in CI —
 * do not add more named presets beyond basic|all|stub; use expanders in lists instead.
 *
 * Tracked managed files (Cargo.toml / capabilities) are copied into
 * `.driver-file-stash/` before injection, then restored after build.
 * Gitignored codegen files are written in place and left as-is.
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
} from './driver-file-stash.mjs';
import { resolvePro } from './resolve-pro.mjs';

const __dirname = dirname(fileURLToPath(import.meta.url));
const ROOT = STASH_ROOT;
const DRIVERS_DIR = resolve(ROOT, 'packages/drivers');

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
 * Supports presets `basic` | `all` | `stub`, comma lists, and expanders
 * `basic` / `:basic` (core path drivers) and `all` / `:all` (all path drivers)
 * so e.g. `basic,kiwi,superset` or `all,kiwi,superset` include git drivers without
 * baking them into the bare presets alone.
 */
export function resolveDrivers(driversArg, registry) {
  if (driversArg === 'none' || driversArg === 'core') {
    console.error(
      `[resolve-drivers] preset "${driversArg}" is no longer supported. Use --drivers=basic for the four core drivers, or --drivers=stub for an empty generated registry.`,
    );
    process.exit(1);
  }

  // Empty / stub: generated.ts with DatabaseType = never. Do not use as a runtime SKU.
  if (driversArg === 'stub' || driversArg === '') {
    return [];
  }

  if (driversArg === 'basic' || driversArg === ':basic') {
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
    } else if (token === 'basic' || token === ':basic') {
      names = [...BASIC_DRIVERS];
    } else if (token.startsWith(':')) {
      console.error(
        `[resolve-drivers] Unknown expander "${token}". Supported expanders: basic, :basic, all, :all`,
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
 * Frontend driver configuration.
 *
 * Each driver declares:
 * - dbTypes: array of { id, metaExport } — database types provided by this driver
 * - metaPath: path to the file exporting meta objects
 * - connectionForm: { component, path, formVariant, advanced? } — custom connection form (optional)
 * - connectionView: { component, path, viewMode } — custom connection window view (optional)
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
  // metaPath like '../../packages/drivers/postgres/ui/meta' (from src/extensions)
  const absMetaTs = resolve(ROOT, 'src/extensions', `${metaPath}.ts`);
  return dirname(absMetaTs);
}

function resolveDriverIconImport(metaPath, dbTypeId) {
  const uiDir = driverUiDirFromMetaPath(metaPath);
  const abs = join(uiDir, 'icons', `${dbTypeId}.svg`);
  if (!existsSync(abs)) return null;
  // import path relative to src/extensions/generated.ts
  const relFromExtensions = relative(resolve(ROOT, 'src/extensions'), abs).replaceAll('\\', '/');
  const importPath = relFromExtensions.startsWith('.') ? relFromExtensions : `./${relFromExtensions}`;
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
      advanced: 'RedisTlsFields',
      validator: { export: 'redisValidate' },
    },
    clipboardParsers: [
      {
        dbType: 'redis',
        export: 'parseRedisConnectionClipboard',
        path: '../../packages/drivers/redis/ui/parseRedisClipboard',
      },
    ],
    connectionView: {
      component: 'RedisConnectionView',
      path: '../../packages/drivers/redis/ui/RedisConnectionView',
      viewMode: 'keyvalue',
    },
    settings: {
      driverId: 'redis',
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
    metaPath: '../../packages/drivers/kiwi/ui/plugin-meta',
    connectionForm: {
      component: 'KiwiConnectionFields',
      path: '../../packages/drivers/kiwi/ui/KiwiConnectionFields',
      formVariant: 'kiwi',
    },
    sqlDialects: [],
  },
  olap: {
    dbTypes: [
      { id: 'presto', metaExport: 'prestoMeta' },
      { id: 'trino', metaExport: 'trinoMeta' },
    ],
    metaPath: '../../packages/drivers/olap/ui/plugin-meta',
    connectionForm: {
      component: 'CatalogConnectionFields',
      path: '../../packages/drivers/olap/ui/CatalogConnectionFields',
      formVariant: 'catalog',
    },
    sqlDialects: [
      { family: 'trino', export: 'trinoDialect', path: '../../packages/drivers/olap/ui/trinoDialect' },
    ],
  },
  superset: {
    dbTypes: [{ id: 'superset', metaExport: 'supersetMeta' }],
    metaPath: '../../packages/drivers/superset/ui/plugin-meta',
    connectionForm: {
      component: 'SupersetConnectionFields',
      path: '../../packages/drivers/superset/ui/SupersetConnectionFields',
      formVariant: 'superset',
      validator: { export: 'supersetValidate' },
    },
    schemaTree: {
      component: 'SupersetSchemaTree',
      path: '../../packages/drivers/superset/ui/SupersetSchemaTree',
      dbType: 'superset',
    },
    sqlDialects: [],
  },
};

/** Drivers that ship frontend locale modules (merged into generated-locales.ts). */
const DRIVER_LOCALE_CONFIG = {
  redis: {
    path: '../../packages/drivers/redis/locales',
    typeExport: 'RedisTranslationKey',
    importPrefix: 'redisLocale',
  },
  mongodb: {
    path: '../../packages/drivers/mongodb/locales',
    typeExport: 'MongoTranslationKey',
    importPrefix: 'mongoLocale',
  },
};

const HOST_LOCALES = ['en', 'zh-CN'];

function generateExtensionLocales(drivers) {
  const importLines = [];
  const typeParts = [];
  const localeIds = HOST_LOCALES.map((id) =>
    id.includes('-') ? `'${id}'` : id,
  );

  const enabledDrivers = drivers.filter((id) => DRIVER_LOCALE_CONFIG[id]);
  const importBindingByPath = new Map();

  for (const driverId of enabledDrivers) {
    const cfg = DRIVER_LOCALE_CONFIG[driverId];
    typeParts.push(cfg.typeExport);
    importLines.push(
      `import type { ${cfg.typeExport} } from '${cfg.path}/en';`,
    );
    for (const localeId of HOST_LOCALES) {
      const binding = `${cfg.importPrefix}_${localeId.replace(/[^a-zA-Z0-9]/g, '_')}`;
      const importPath = `${cfg.path}/${localeId}`;
      importLines.push(`import ${binding} from '${importPath}';`);
      importBindingByPath.set(`${driverId}:${localeId}`, binding);
    }
  }

  const typeUnion = typeParts.length > 0 ? typeParts.join(' | ') : 'never';
  const useEmpty = enabledDrivers.length === 0;

  const localeEntryLines = HOST_LOCALES.map((localeId) => {
    const spreads = enabledDrivers.map((driverId) => {
      const binding = importBindingByPath.get(`${driverId}:${localeId}`);
      return `{ ...${binding} }`;
    });
    const localeKey = localeId.includes('-') ? `'${localeId}'` : localeId;
    return `  ${localeKey}: ${spreads.length > 0 ? `Object.assign({}, ${spreads.join(', ')})` : 'EMPTY'},`;
  });

  const emptyDecl = useEmpty ? `const EMPTY: Record<string, string> = {};\n\n` : '';

  const content = `/**
 * AUTO-GENERATED by resolve-drivers.mjs — DO NOT EDIT MANUALLY
 *
 * Merges locale dictionaries from enabled driver packages.
 * Regenerated every time the build runs with different --drivers args.
 */
${importLines.length > 0 ? importLines.join('\n') + '\n' : ''}
export type DriverTranslationKey = ${typeUnion};

${emptyDecl}/** Driver locale strings keyed by translation key, merged per host locale. */
export const DRIVER_LOCALES: Record<string, Record<string, string>> = {
${localeEntryLines.join('\n')}
};
`;

  const outPath = workPath('src/extensions/generated-locales.ts');
  mkdirSync(dirname(outPath), { recursive: true });
  writeFileSync(outPath, content);
  console.log(`[resolve-drivers] wrote ${outPath}`);
}

function generateFrontendRegistry(drivers) {
  const importLines = [];
  const iconImportLines = [];
  const dbEntryLines = [];
  const iconEntryLines = [];
  const iconParentEntryLines = [];
  const formEntryLines = [];
  const validatorEntryLines = [];
  const clipboardParserEntryLines = [];
  const dialectEntryLines = [];
  const schemaTreeEntryLines = [];
  const connectionViewEntryLines = [];
  const settingsEntryLines = [];
  const driverDbTypes = [];
  const iconImportByAbs = new Map();

  for (const id of drivers) {
    const cfg = FRONTEND_DRIVER_CONFIG[id];
    if (!cfg) continue;

    // Import meta exports
    const metaExports = cfg.dbTypes.map(dt => dt.metaExport).join(', ');
    importLines.push(`import { ${metaExports} } from '${cfg.metaPath}';`);

    // DB type entries
    for (const dt of cfg.dbTypes) {
      driverDbTypes.push(dt.id);
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
      if (cfg.connectionForm.advanced) {
        formImports.push(cfg.connectionForm.advanced);
      }
      if (cfg.connectionForm.validator) {
        formImports.push(cfg.connectionForm.validator.export);
        validatorEntryLines.push(
          `  ${cfg.connectionForm.formVariant}: ${cfg.connectionForm.validator.export},`
        );
      }
      importLines.push(
        `import { ${formImports.join(', ')} } from '${cfg.connectionForm.path}';`
      );
      const advancedPart = cfg.connectionForm.advanced
        ? `, advanced: ${cfg.connectionForm.advanced}`
        : '';
      formEntryLines.push(
        `  { formVariant: '${cfg.connectionForm.formVariant}', component: ${cfg.connectionForm.component}${advancedPart} },`
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

    // Connection view (e.g. Redis keyvalue shell)
    if (cfg.connectionView) {
      importLines.push(
        `import { ${cfg.connectionView.component} } from '${cfg.connectionView.path}';`
      );
      connectionViewEntryLines.push(
        `  { viewMode: '${cfg.connectionView.viewMode}', component: ${cfg.connectionView.component} },`
      );
    }

    // Clipboard parsers (new-connection auto-detect)
    for (const parser of cfg.clipboardParsers || []) {
      importLines.push(`import { ${parser.export} } from '${parser.path}';`);
      clipboardParserEntryLines.push(
        `  { dbType: '${parser.dbType}', parse: ${parser.export} },`,
      );
    }

    // SQL dialects
    for (const dial of cfg.sqlDialects || []) {
      importLines.push(`import { ${dial.export} } from '${dial.path}';`);
      dialectEntryLines.push(`  ${dial.family}: ${dial.export},`);
    }

    // Driver settings (Extensions UI)
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
        `driverId: '${s.driverId}'`,
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

  const typeUnion = driverDbTypes.length > 0
    ? driverDbTypes.map(t => `'${t}'`).join(' | ')
    : 'never';

  // Driver commands registry
  const driverCommandLines = [];
  for (const id of drivers) {
    const cfg = FRONTEND_DRIVER_CONFIG[id];
    if (!cfg) continue;
    const registryEntry = JSON.parse(readFileSync(resolve(ROOT, 'drivers-registry.json'), 'utf-8'));
    const meta = registryEntry[id];
    if (meta?.tauriPlugin?.commands?.length > 0) {
      const cmds = meta.tauriPlugin.commands.map(c => `'${c}'`).join(', ');
      driverCommandLines.push(
        `  { driverId: '${meta.tauriPlugin.id}', commands: [${cmds}] },`
      );
    }
  }

  const content = `/**
 * AUTO-GENERATED by resolve-drivers.mjs — DO NOT EDIT MANUALLY
 *
 * This file registers frontend components and metadata for active drivers.
 * Regenerated every time the build runs with different --drivers args.
 */
${importLines.length > 0 ? importLines.join('\n') + '\n' : ''}${iconImportLines.length > 0 ? iconImportLines.join('\n') + '\n' : ''}import { invoke } from '@tauri-apps/api/core';
import type { DatabaseTypeMeta } from '@datazen/driver-sdk';
import type { SqlDialectStrategy } from '@datazen/driver-sdk';
import type { DriverFormValidator } from '@datazen/driver-sdk';
import type { ConnectionClipboardParser } from '@datazen/driver-sdk';
import type { DriverSettingsContribution } from '../lib/driverSettings';
import type { ComponentType } from 'react';

/**
 * Frontend driver protocol version.
 * Must match the version expected by the main app.
 * Bump when making breaking changes to driver interfaces.
 */
export const DRIVER_PROTOCOL_VERSION = 1;

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

/** Driver-provided SQL dialect strategies (merged into DIALECTS). */
export const DRIVER_SQL_DIALECTS: Record<string, SqlDialectStrategy> = {
${dialectEntryLines.join('\n')}
};

/** Driver-provided connection form components. */
interface DriverFormEntry {
  formVariant: string;
  component: ComponentType<any>;
  advanced?: ComponentType<any>;
}

const DRIVER_FORMS: DriverFormEntry[] = [
${formEntryLines.join('\n')}
];

/** Lookup driver-provided connection form by form variant (e.g. 'kiwi', 'catalog'). */
export function getDriverConnectionForm(formVariant: string): ComponentType<any> | undefined {
  for (const entry of DRIVER_FORMS) {
    if (entry.formVariant === formVariant) {
      return entry.component;
    }
  }
  return undefined;
}

/** Lookup driver-provided Advanced-settings fields by form variant. */
export function getDriverConnectionAdvanced(formVariant: string): ComponentType<any> | undefined {
  for (const entry of DRIVER_FORMS) {
    if (entry.formVariant === formVariant) {
      return entry.advanced;
    }
  }
  return undefined;
}

/** Driver-provided form validators, keyed by form variant. */
const DRIVER_VALIDATORS: Record<string, DriverFormValidator> = {
${validatorEntryLines.join('\n')}
};

/** Lookup driver-provided form validator by form variant. */
export function getDriverValidator(formVariant: string): DriverFormValidator | undefined {
  return DRIVER_VALIDATORS[formVariant];
}

/** Driver-provided clipboard parsers for new-connection auto-detect. */
const DRIVER_CLIPBOARD_PARSERS: { dbType: string; parse: ConnectionClipboardParser }[] = [
${clipboardParserEntryLines.join('\n')}
];

export function getDriverClipboardParsers(): { dbType: string; parse: ConnectionClipboardParser }[] {
  return DRIVER_CLIPBOARD_PARSERS;
}

// ===== Driver Schema Trees =====

interface DriverSchemaTreeEntry {
  dbType: string;
  component: ComponentType<any>;
}

const DRIVER_SCHEMA_TREES: DriverSchemaTreeEntry[] = [
${schemaTreeEntryLines.join('\n')}
];

/** Lookup driver-provided schema tree by database type. */
export function getDriverSchemaTree(dbType: string): ComponentType<any> | undefined {
  for (const entry of DRIVER_SCHEMA_TREES) {
    if (entry.dbType === dbType) {
      return entry.component;
    }
  }
  return undefined;
}

// ===== Driver Connection Views =====

interface DriverConnectionViewEntry {
  viewMode: string;
  component: ComponentType<any>;
}

const DRIVER_CONNECTION_VIEWS: DriverConnectionViewEntry[] = [
${connectionViewEntryLines.join('\n')}
];

/** Lookup driver-provided connection view by connectionView mode (e.g. 'keyvalue'). */
export function getDriverConnectionView(viewMode: string): ComponentType<any> | undefined {
  for (const entry of DRIVER_CONNECTION_VIEWS) {
    if (entry.viewMode === viewMode) {
      return entry.component;
    }
  }
  return undefined;
}

// ===== Driver Settings (Extensions UI) =====

/** Driver-provided settings sections and/or JSON Schema forms. */
export const DRIVER_SETTINGS_ENTRIES: DriverSettingsContribution[] = [
${settingsEntryLines.join('\n')}
];

// ===== Driver Commands =====

export interface DriverCommandMeta {
  driverId: string;
  commands: string[];
}

/** Commands registered by active drivers via Tauri Plugin system. */
export const DRIVER_COMMANDS: DriverCommandMeta[] = [
${driverCommandLines.join('\n')}
];

/** Invoke a driver-provided Tauri command using plugin:id|command format. */
export async function driverInvoke<T = unknown>(
  driverId: string,
  command: string,
  args?: Record<string, unknown>,
): Promise<T> {
  return invoke<T>(\`plugin:\${driverId}|\${command}\`, args ?? {});
}

/** Check whether a specific plugin command is available in this build. */
export function hasDriverCommand(driverId: string, command: string): boolean {
  return DRIVER_COMMANDS.some(
    (p) => p.driverId === driverId && p.commands.includes(command),
  );
}
`;

  const outPath = workPath('src/extensions/generated.ts');
  mkdirSync(dirname(outPath), { recursive: true });
  writeFileSync(outPath, content);
  console.log(`[resolve-drivers] wrote ${outPath}`);
}

/**
 * Clone or update git/local drivers into packages/drivers/<name>/.
 * Supports git (clone from remote) and local (symlink from local path).
 * Git drivers may pin `ref` (commit SHA or tag) in drivers-registry.json.
 * These checkouts are gitignored and excluded from the Cargo workspace.
 */
function checkoutGitRef(driverDir, ref) {
  if (!ref) return;
  try {
    execSync(`git fetch --depth 1 origin ${ref}`, { cwd: driverDir, stdio: 'pipe' });
  } catch {
    // Shallow fetch by SHA can fail on some hosts; fall back to full fetch.
    execSync('git fetch origin', { cwd: driverDir, stdio: 'pipe' });
  }
  execSync(`git checkout --detach ${ref}`, { cwd: driverDir, stdio: 'pipe' });
}

function cloneDrivers(drivers, registry) {
  mkdirSync(DRIVERS_DIR, { recursive: true });

  for (const name of drivers) {
    const meta = registry[name];
    if (meta.source === 'path' || meta.source === 'builtin') continue;

    const driverDir = resolve(DRIVERS_DIR, name);

    if (meta.source === 'local' && meta.path) {
      const localPath = resolve(ROOT, meta.path);
      if (!existsSync(localPath)) {
        console.error(`[resolve-drivers] local plugin path not found: ${localPath}`);
        continue;
      }
      // Create symlink for local development
      if (existsSync(driverDir)) {
        execSync(`rm -rf ${driverDir}`, { stdio: 'pipe' });
      }
      execSync(`ln -s ${localPath} ${driverDir}`, { stdio: 'pipe' });
      console.log(`[resolve-drivers] linked ${name} → ${localPath}`);
    } else if (meta.source === 'git' && meta.git) {
      const pinnedRef = meta.ref || meta.commit;
      if (existsSync(resolve(driverDir, '.git'))) {
        console.log(`[resolve-drivers] updating ${name} ...`);
        try {
          execSync('git fetch origin', { cwd: driverDir, stdio: 'pipe' });
        } catch {
          console.warn(`  [warn] git fetch failed for "${name}", using existing checkout`);
        }
        if (pinnedRef) {
          try {
            checkoutGitRef(driverDir, pinnedRef);
            console.log(`  [resolve-drivers] ${name} checked out ${pinnedRef}`);
          } catch {
            console.warn(`  [warn] checkout ${pinnedRef} failed for "${name}", using HEAD`);
          }
        } else {
          try {
            execSync('git pull --ff-only', { cwd: driverDir, stdio: 'pipe' });
          } catch {
            console.warn(`  [warn] git pull failed for "${name}", using existing checkout`);
          }
        }
      } else if (existsSync(resolve(driverDir, 'Cargo.toml'))) {
        // Local development: directory exists with source but no .git — keep as-is
        console.log(`[resolve-drivers] using local ${name} (no .git, has Cargo.toml)`);
      } else {
        // Remove stale symlink or directory if source type changed
        if (existsSync(driverDir)) {
          execSync(`rm -rf ${driverDir}`, { stdio: 'pipe' });
        }
        console.log(`[resolve-drivers] cloning ${name} from ${meta.git} ...`);
        execSync(`git clone ${meta.git} ${driverDir}`, { stdio: 'pipe' });
        if (pinnedRef) {
          checkoutGitRef(driverDir, pinnedRef);
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
 * Update Cargo.toml files to include only resolved drivers.
 *
 * - src-tauri/Cargo.toml: adds optional path dependencies + feature flags
 * - Cargo.toml (root): adds [patch] entries when git-sourced drivers are
 *   cloned locally, so Cargo uses the local checkout instead of fetching
 */
function crateDepPath(name, meta) {
  if (meta.source === 'path' && meta.path) {
    // src-tauri/Cargo.toml → ../packages/drivers/X
    return `../${meta.path}`;
  }
  return `../packages/drivers/${name}`;
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

function updateCargoFiles(drivers, registry) {
  // --- src-tauri/Cargo.toml: plugin deps ---
  const depLines = drivers.map(name => {
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
  const featureLines = drivers.map(name => {
    const meta = registry[name];
    const feature = meta?.feature;
    const crateName = cratePackageName(name, meta || {});
    return feature ? `${feature} = ["dep:${crateName}"]` : null;
  }).filter(Boolean);
  replaceMarkerSection('src-tauri/Cargo.toml', 'PLUGIN FEATURES', featureLines.join('\n'));

  // --- Root Cargo.toml: [patch] entries for git-sourced drivers ---
  // Prefer the same builder as injectRootCargoPatches (includes driver-api unify).
  replaceMarkerSection(
    'Cargo.toml',
    'DRIVER PATCHES',
    buildRootCargoPatchLines(drivers, registry).join('\n').replace(/^\n/, ''),
  );

  console.log(`[resolve-drivers] updated Cargo.toml files (${drivers.length} driver(s))`);
}

/**
 * Generate src-tauri/src/driver_init.rs — Rust plugin Tauri-plugin registration.
 *
 * For each plugin that declares a `tauriPlugin` block in the registry,
 * generates a #[cfg(feature = "...")] guarded .plugin() call.
 */
function generateRustDriverInit(drivers, registry) {
  const externCrateLines = [];
  const driverInitLines = [];

  for (const name of drivers) {
    const meta = registry[name];
    if (!meta.feature) continue;

    const feature = meta.feature;
    const crateName = crateRustIdent(name, meta);

    externCrateLines.push(`#[cfg(feature = "${feature}")]`);
    externCrateLines.push(`extern crate ${crateName};`);

    if (meta.tauriPlugin) {
      const initFn = meta.tauriPlugin.initFn;
      driverInitLines.push(`    #[cfg(feature = "${feature}")]`);
      driverInitLines.push(`    let builder = builder.plugin(${initFn}());`);
      driverInitLines.push('');
    }
  }

  const externBlock = externCrateLines.length > 0
    ? externCrateLines.join('\n') + '\n'
    : '';

  const body = driverInitLines.length > 0
    ? driverInitLines.join('\n')
    : '    // No drivers with Tauri commands enabled';

  const content = `// AUTO-GENERATED by resolve-drivers.mjs — DO NOT EDIT MANUALLY
//
// Ensures driver crates are linked into the binary (extern crate)
// so that inventory-based driver registration takes effect.
// Also registers Tauri plugins for crates that declare a tauriPlugin block.

${externBlock}use tauri::Runtime;

pub fn register_drivers<R: Runtime>(builder: tauri::Builder<R>) -> tauri::Builder<R> {
    let builder = builder;

${body}

    builder
}
`;

  const outPath = workPath('src-tauri/src/driver_init.rs');
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
 * Drivers that declare `tauriPlugin` also enable the crate's `tauri-plugin` feature
 * so permissions/build.rs and init() are compiled in.
 */
function injectCargoToml(drivers, registry) {
  const relPath = 'src-tauri/Cargo.toml';
  let content = readFileSync(managedReadPath(relPath), 'utf-8');

  // Build dependency lines (path-based, optional)
  const depLines = [];
  for (const name of drivers) {
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
  for (const name of drivers) {
    const meta = registry[name];
    if (!meta.feature) continue;
    const crateName = cratePackageName(name, meta);
    featureLines.push(`${meta.feature} = ["dep:${crateName}"]`);
  }

  content = replaceMarkerBlock(content, 'driver-dependencies', depLines);
  content = replaceMarkerBlock(content, 'driver-features', featureLines);

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
 * Generate Tauri ACL capabilities by merging host + plugin permissions.
 *
 * Reads `default_host.json` (git-tracked, host-only permissions) and appends
 * `{tauriPlugin.id}:default` for each active plugin that exposes commands.
 * Writes the merged result to `default.json` (gitignored).
 */
function syncDriverCapabilities(drivers, registry) {
  const hostPath = workPath('src-tauri/capabilities/default.json.host');
  if (!existsSync(hostPath)) {
    console.warn(`[resolve-drivers] host capabilities file not found: ${hostPath}`);
    return;
  }

  const tomlPath = workPath('src-tauri/Cargo.toml');
  const toml = existsSync(tomlPath) ? readFileSync(tomlPath, 'utf-8') : '';
  const effective = drivers.filter((name) => {
    const feat = registry[name]?.feature;
    return Boolean(feat) && toml.includes(feat);
  });
  const skipped = drivers.filter((name) => !effective.includes(name));
  if (skipped.length > 0) {
    console.log(
      `[resolve-drivers] skipping capabilities for not-injected drivers: [${skipped.join(', ')}]`,
    );
  }

  const cap = JSON.parse(readFileSync(hostPath, 'utf-8'));
  if (!Array.isArray(cap.permissions)) {
    console.warn('[resolve-drivers] capabilities.permissions is not an array');
    return;
  }

  const added = [];
  for (const name of effective) {
    const tp = registry[name]?.tauriPlugin;
    if (!tp?.id) continue;
    // Skip drivers with no commands — they don't generate an ACL permissions
    // manifest, so referencing `<id>:default` would fail at build time.
    if (!Array.isArray(tp.commands) || tp.commands.length === 0) continue;
    const perm = `${tp.id}:default`;
    if (!cap.permissions.includes(perm) && !added.includes(perm)) {
      added.push(perm);
    }
  }

  const nextPermissions = [...cap.permissions, ...added];
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
  writeFileSync(workPath('src-tauri/capabilities/default.json'), content);
  console.log(
    `[resolve-drivers] generated capabilities (host + drivers): [${added.join(', ') || 'no drivers'}]`,
  );
}

/**
 * Git drivers consume `datazen-driver-api` from crates.io. When building inside
 * the Host workspace, patch crates.io to the local path so inventory registration
 * reaches the same API crate as the Host (avoids "Driver not found for type: …").
 */
/** Build root Cargo.toml [patch] lines for git drivers (+ crates.io driver-api unify). */
export function buildRootCargoPatchLines(drivers, registry) {
  const patchLines = [];
  let hasGitDriver = false;
  for (const name of drivers) {
    const meta = registry[name];
    if (meta.source !== 'git' || !meta.git) continue;
    hasGitDriver = true;
    const crateName = cratePackageName(name, meta);
    patchLines.push('');
    patchLines.push(`[patch."${meta.git}"]`);
    patchLines.push(`${crateName} = { path = "packages/drivers/${name}" }`);
  }
  if (hasGitDriver) {
    patchLines.push('');
    patchLines.push('[patch.crates-io]');
    patchLines.push('datazen-driver-api = { path = "packages/driver-api" }');
  }
  return patchLines;
}

/**
 * Inject [patch] entries into root Cargo.toml for drivers using git sources.
 * Uses start/end markers for idempotent injection.
 */
function injectRootCargoPatches(drivers, registry) {
  const relPath = 'Cargo.toml';
  let content = readFileSync(managedReadPath(relPath), 'utf-8');

  const patchLines = buildRootCargoPatchLines(drivers, registry);

  content = replaceMarkerBlock(content, 'driver-patches', patchLines);

  writeFileSync(workPath(relPath), content);
  if (patchLines.length > 0) {
    console.log(`[resolve-drivers] injected.*drivers.length} patch(es) into root Cargo.toml`);
  }
}

function wantsRestoreOnly(argv = process.argv.slice(2)) {
  return argv.includes('--restore');
}

export function wantsCodegenOnly(argv = process.argv.slice(2)) {
  return argv.includes('--codegen-only');
}

function main() {
  if (wantsRestoreOnly()) {
    restoreManagedFiles();
    return;
  }

  const registry = loadRegistry();
  const driversArg = parseArgs();
  const codegenOnly = wantsCodegenOnly();

  console.log(`[resolve-drivers] drivers arg: "${driversArg}"`);
  if (codegenOnly) {
    console.log('[resolve-drivers] --codegen-only (skip Cargo.toml / capabilities inject)');
  }

  const resolvedDrivers = resolveDrivers(driversArg, registry);

  console.log(`[resolve-drivers] resolved drivers: [${resolvedDrivers.join(', ')}]`);

  // Stash tracked managed files, then write injected copies at original paths.
  // Idempotent: if an outer caller already stashed, re-inject over working copies.
  // Codegen-only never touches Cargo.toml / capabilities.
  if (!codegenOnly) {
    if (allStashed()) {
      console.log(
        '[resolve-drivers] stash already present; re-injecting without re-stash',
      );
    } else {
      stashManagedFiles();
    }
  }

  try {
    // Clone/update git driver repos (path drivers use workspace members)
    cloneDrivers(resolvedDrivers, registry);

    const features = generateCargoFeatures(resolvedDrivers, registry);

    console.log(`[resolve-drivers] cargo features: [${features.join(', ')}]`);

    // Show plugin sources
    for (const name of resolvedDrivers) {
      const meta = registry[name];
      if (meta.source === 'git') {
        console.log(`  ${name}: ${meta.git}`);
      } else if (meta.source === 'path' || meta.source === 'workspace') {
        console.log(`  ${name}: path (${meta.path})`);
      }
    }

    if (!codegenOnly) {
      injectCargoToml(resolvedDrivers, registry);
      injectRootCargoPatches(resolvedDrivers, registry);
    }
    // Always generate capabilities (default.json is gitignored, built from default_host.json + drivers).
    // Capabilities ALWAYS mirror src-tauri/Cargo.toml's actual injection state: only drivers whose
    // cargo feature is present may contribute permissions. In full mode injectCargoToml runs first,
    // so every selected plugin qualifies; in codegen-only / post-restore states the file stays
    // consistent with what cargo will actually compile (otherwise tauri-build fails with
    // "Permission <plugin>:default not found"). This also self-heals stale capabilities left by a
    // stash restore after a full build.
    syncDriverCapabilities(resolvedDrivers, registry);

    // Write the features file for the build system to consume
    const output = {
      drivers: resolvedDrivers,
      features,
      cargoArgs: features.length > 0
        ? `--features "${features.join(',')}"`
        : '',
    };

    const outPath = resolve(ROOT, '.driver-features.json');
    writeFileSync(outPath, JSON.stringify(output, null, 2) + '\n');
    console.log(`[resolve-drivers] wrote ${outPath}`);

    generateFrontendRegistry(resolvedDrivers);
    generateExtensionLocales(resolvedDrivers);
    generateRustDriverInit(resolvedDrivers, registry);
    if (!existsSync(resolve(ROOT, 'src/extensions/generated-pro.ts'))) {
      resolvePro({ codegenOnly: true });
    }

    // Also output to stdout for scripts that pipe this
    console.log(`\nCargo build command:`);
    if (features.length > 0) {
      console.log(`  cargo build --features "${features.join(',')}"`);
    } else {
      console.log(`  cargo build`);
    }
    if (codegenOnly) {
      console.log(
        '[resolve-drivers] codegen-only: wrote generated.ts / generated-locales.ts / driver_init.rs',
      );
    } else {
      console.log(
        `[resolve-drivers] managed files are injected; run \`node scripts/driver-file-stash.mjs restore\` after build`,
      );
    }
  } catch (err) {
    if (codegenOnly) {
      throw err;
    }
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
