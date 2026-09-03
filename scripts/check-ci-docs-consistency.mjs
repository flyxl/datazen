#!/usr/bin/env node
/**
 * Documentation consistency guard for the prh-ci-docs track.
 *
 * 1. Driver ids mentioned in ci-test-matrix.md exist in drivers-registry.json
 * 2. Sub-window kinds documented in windows.md match windowManager / windowKind
 * 3. Toolchain versions in README / CONTRIBUTING match .github/workflows/ci.yml
 */
import { readFileSync, existsSync } from 'fs';
import { dirname, resolve } from 'path';
import { fileURLToPath } from 'url';

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), '..');

const PATHS = {
  ciMatrix: 'docs/development/ci-test-matrix.md',
  windows: 'docs/architecture/windows.md',
  readme: 'README.md',
  contributing: 'CONTRIBUTING.md',
  registry: 'drivers-registry.json',
  ciYml: '.github/workflows/ci.yml',
  windowManager: 'src/lib/windowManager.ts',
  windowKind: 'src/lib/windowKind.ts',
};

function read(root, rel) {
  const full = resolve(root, rel);
  if (!existsSync(full)) {
    throw new Error(`missing file: ${rel}`);
  }
  return readFileSync(full, 'utf-8');
}

/** Driver ids explicitly named in ci-test-matrix.md (basic, optional examples, Akulaku). */
export function extractCiMatrixDriverIds(ciMatrixText) {
  const ids = new Set();

  // basic preset parenthetical: (postgres, mysql, sqlite, redis)
  for (const m of ciMatrixText.matchAll(/\((postgres|mysql|sqlite|redis(?:,\s*\w+)*)\)/g)) {
    for (const part of m[1].split(',')) ids.add(part.trim());
  }

  // comma lists: postgres,mysql,sqlite,redis,mongodb,kiwi,superset
  for (const m of ciMatrixText.matchAll(
    /\b(postgres|mysql|sqlite|redis|mongodb|clickhouse|duckdb|sqlserver|elasticsearch|kiwi|superset)(?:,(?:postgres|mysql|sqlite|redis|mongodb|clickhouse|duckdb|sqlserver|elasticsearch|kiwi|superset))+\b/g,
  )) {
    for (const part of m[0].split(',')) ids.add(part.trim());
  }

  // optional path driver examples (mongodb、clickhouse、duckdb…)
  for (const m of ciMatrixText.matchAll(
    /(?:mongodb|clickhouse|duckdb|sqlserver|elasticsearch)/g,
  )) {
    ids.add(m[0]);
  }

  // cargo test -p datazen-driver-<id> (exclude meta crates driver-api / ai-api)
  for (const m of ciMatrixText.matchAll(/datazen-driver-([a-z][a-z0-9-]*)/g)) {
    const id = m[1];
    if (id !== 'api') ids.add(id);
  }

  return [...ids].sort();
}

/** @returns {{ ok: boolean, missing: string[], registryIds: string[] }} */
export function checkCiMatrixDrivers(opts = {}) {
  const root = opts.root ?? ROOT;
  const registry = JSON.parse(read(root, PATHS.registry));
  const registryIds = Object.keys(registry);
  const mentioned = extractCiMatrixDriverIds(read(root, PATHS.ciMatrix));
  const missing = mentioned.filter((id) => !registryIds.includes(id));
  return { ok: missing.length === 0, missing, mentioned, registryIds };
}

/** Sub-window `?window=` kinds that must exist in windowKind.ts and windowManager.ts. */
const DOCUMENTED_SUB_WINDOW_KINDS = [
  'backup',
  'data-sync',
  'schema-diff',
  'data-transfer',
];

/** Singleton labels that create_sub_window receives (from windowManager.ts). */
const EXPECTED_SINGLETON_LABELS = [
  'backup-singleton',
  'backup-restore-singleton',
  'data-sync-singleton',
  'data-transfer-singleton',
  'schema-diff-singleton',
];

/** @returns {{ ok: boolean, errors: string[] }} */
export function checkWindowBoundaries(opts = {}) {
  const root = opts.root ?? ROOT;
  const errors = [];
  const windowsMd = read(root, PATHS.windows);
  const windowManager = read(root, PATHS.windowManager);
  const windowKind = read(root, PATHS.windowKind);

  for (const kind of DOCUMENTED_SUB_WINDOW_KINDS) {
    if (!windowsMd.includes(kind)) {
      errors.push(`windows.md missing documented sub-window kind: ${kind}`);
    }
    if (!windowKind.includes(`'${kind}'`)) {
      errors.push(`windowKind.ts missing WindowKind: '${kind}'`);
    }
    if (!windowManager.includes(`window: '${kind}'`)) {
      errors.push(`windowManager.ts missing openSingletonWindow param window: '${kind}'`);
    }
  }

  for (const label of EXPECTED_SINGLETON_LABELS) {
    if (!windowManager.includes(`'${label}'`)) {
      errors.push(`windowManager.ts missing singleton label: '${label}'`);
    }
  }

  const capabilityExport = windowManager.match(
    /export const WINDOW_CAPABILITY_LABEL_SAMPLES = \[([\s\S]*?)\] as const;/,
  );
  if (!capabilityExport) {
    errors.push('windowManager.ts missing WINDOW_CAPABILITY_LABEL_SAMPLES export');
  } else {
    for (const label of EXPECTED_SINGLETON_LABELS) {
      if (!capabilityExport[1].includes(`'${label}'`)) {
        errors.push(`WINDOW_CAPABILITY_LABEL_SAMPLES missing '${label}'`);
      }
    }
  }

  // windows.md §4 snippet should list every non-main WindowKind (doc drift guard).
  const kindTypeMatch = windowsMd.match(
    /export type WindowKind = ([^;]+);/,
  );
  if (kindTypeMatch) {
    for (const kind of DOCUMENTED_SUB_WINDOW_KINDS) {
      if (!kindTypeMatch[1].includes(`'${kind}'`)) {
        errors.push(
          `windows.md §4 WindowKind snippet missing '${kind}' (code has it in windowKind.ts)`,
        );
      }
    }
  }

  return { ok: errors.length === 0, errors };
}

/** @returns {{ ok: boolean, errors: string[], ci: { node: string, pnpm: string } }} */
export function checkToolchainVersions(opts = {}) {
  const root = opts.root ?? ROOT;
  const errors = [];
  const ciYml = read(root, PATHS.ciYml);
  const readme = read(root, PATHS.readme);
  const contributing = read(root, PATHS.contributing);

  const nodeMatch = ciYml.match(/node-version:\s*(\d+)/);
  const pnpmMatch = ciYml.match(/version:\s*(\d+)/);
  if (!nodeMatch || !pnpmMatch) {
    errors.push('ci.yml missing node-version or pnpm version');
    return { ok: false, errors, ci: { node: '', pnpm: '' } };
  }

  const ci = { node: nodeMatch[1], pnpm: pnpmMatch[1] };

  for (const [name, text] of [
    ['README.md', readme],
    ['CONTRIBUTING.md', contributing],
    ['ci-test-matrix.md', read(root, PATHS.ciMatrix)],
  ]) {
    if (!text.includes(`Node **${ci.node}**`) && !text.includes(`Node.js | **${ci.node}**`)) {
      errors.push(`${name} does not document Node ${ci.node} (CI node-version)`);
    }
    if (!text.includes(`pnpm **${ci.pnpm}**`) && !text.includes(`pnpm | **${ci.pnpm}**`)) {
      errors.push(`${name} does not document pnpm ${ci.pnpm} (CI pnpm version)`);
    }
    if (!/Rust.*stable/i.test(text)) {
      errors.push(`${name} does not mention Rust stable`);
    }
  }

  if (!ciYml.includes('dtolnay/rust-toolchain@stable')) {
    errors.push('ci.yml does not use Rust stable toolchain action');
  }

  return { ok: errors.length === 0, errors, ci };
}

/**
 * @param {{
 *   root?: string,
 *   log?: (...args: unknown[]) => void,
 *   error?: (...args: unknown[]) => void,
 * }} [opts]
 * @returns {number} 0 if all checks pass
 */
export function checkCiDocsConsistency(opts = {}) {
  const root = opts.root ?? ROOT;
  const log = opts.log ?? console.log.bind(console);
  const error = opts.error ?? console.error.bind(console);
  let failed = false;

  const drivers = checkCiMatrixDrivers({ root });
  if (drivers.ok) {
    log(`[check-ci-docs] drivers ok (${drivers.mentioned.length} ids in ci-test-matrix.md)`);
  } else {
    failed = true;
    error(`[check-ci-docs] unknown driver ids in ci-test-matrix.md: ${drivers.missing.join(', ')}`);
  }

  const windows = checkWindowBoundaries({ root });
  if (windows.ok) {
    log('[check-ci-docs] window boundaries ok');
  } else {
    failed = true;
    for (const msg of windows.errors) error(`[check-ci-docs] ${msg}`);
  }

  const toolchain = checkToolchainVersions({ root });
  if (toolchain.ok) {
    log(`[check-ci-docs] toolchain ok (Node ${toolchain.ci.node}, pnpm ${toolchain.ci.pnpm}, Rust stable)`);
  } else {
    failed = true;
    for (const msg of toolchain.errors) error(`[check-ci-docs] ${msg}`);
  }

  return failed ? 1 : 0;
}

/* istanbul ignore next */
if (process.argv[1] && process.argv[1].endsWith('check-ci-docs-consistency.mjs')) {
  process.exitCode = checkCiDocsConsistency();
}
