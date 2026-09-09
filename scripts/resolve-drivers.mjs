#!/usr/bin/env node
/**
 * resolve-drivers.mjs
 *
 * Self-heal entry: downloads the last known-good resolve-drivers body from
 * GitHub (commit 0d08398 on feature/jdbc-agent) and applies the JDBC frontend
 * registration patch, writing scripts/resolve-drivers.impl.mjs.
 *
 * Prefer replacing this with the full in-tree script once uploaded; until then
 * builds need network on first materialize (or commit the generated impl).
 */
import { writeFileSync, existsSync } from 'fs';
import { fileURLToPath, pathToFileURL } from 'url';
import { dirname, join } from 'path';
import { execSync } from 'child_process';

const dir = dirname(fileURLToPath(import.meta.url));
const implPath = join(dir, 'resolve-drivers.impl.mjs');
const BASE =
  'https://raw.githubusercontent.com/flyxl/datazen/0d08398e8012b171c43c5352e0c208e1154b4d1a/scripts/resolve-drivers.mjs';

function materialize() {
  const text = execSync(`curl -fsSL '${BASE}'`, {
    encoding: 'utf8',
    maxBuffer: 12 * 1024 * 1024,
  });

  const vectorBlock = `  vector: {
    dbTypes: [{ id: 'vector', metaExport: 'vectorMeta' }],
    metaPath: '../../packages/drivers/vector/ui/meta',
  },
};`;

  const jdbcBlock = `  vector: {
    dbTypes: [{ id: 'vector', metaExport: 'vectorMeta' }],
    metaPath: '../../packages/drivers/vector/ui/meta',
  },
  jdbc: {
    dbTypes: [{ id: 'jdbc', metaExport: 'jdbcMeta' }],
    metaPath: '../../packages/drivers/jdbc/ui/meta',
    connectionForm: {
      component: 'JdbcConnectionFields',
      path: '../../packages/drivers/jdbc/ui/JdbcConnectionFields',
      formVariant: 'jdbc',
      validator: { export: 'jdbcValidate' },
    },
    settings: {
      pluginId: 'jdbc',
      label: 'JDBC',
      sectionExport: 'JdbcSettingsSection',
      sectionPath: '../../packages/drivers/jdbc/ui/settings',
      schemaExport: 'jdbcSettingsSchema',
      schemaPath: '../../packages/drivers/jdbc/ui/settings',
    },
  },
};`;

  if (!text.includes(vectorBlock)) {
    throw new Error('unexpected base resolve-drivers.mjs (vector block missing)');
  }
  let out = text.replace(vectorBlock, jdbcBlock);

  const locOld = `const DRIVER_LOCALE_CONFIG = {
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
};`;

  const locNew = `const DRIVER_LOCALE_CONFIG = {
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
  jdbc: {
    path: '../../packages/drivers/jdbc/locales',
    typeExport: 'JdbcTranslationKey',
    importPrefix: 'jdbcLocale',
  },
};`;

  if (!out.includes(locOld)) {
    throw new Error('unexpected base resolve-drivers.mjs (locale config missing)');
  }
  out = out.replace(locOld, locNew);
  writeFileSync(implPath, out);
  console.log('[resolve-drivers] materialized', implPath);
}

if (!existsSync(implPath) || process.argv.includes('--rematerialize')) {
  materialize();
}
await import(pathToFileURL(implPath).href);
