#!/usr/bin/env node
import { isDirectInvocation } from './lib/isDirectInvocation.mjs';
/**
 * Fail if git-tracked managed files look injected, or if the static path-driver
 * declaration in crates/datazen-desktop/Cargo.toml drifted from
 * drivers-registry.json.
 *
 * Two checks:
 * 1. Injection residue (since always): the tracked managed files must look
 *    clean on a checkout. Since W3-c only **git** drivers (kiwi/superset) still
 *    inject at build time; path drivers are static and never inject.
 * 2. Static-declaration consistency (W3-c, `--check-static-drivers`): every
 *    `path` driver in drivers-registry.json must have an optional dep + a
 *    `driver-*` feature (+ a seat in the `drivers` aggregate) statically
 *    declared in crates/datazen-desktop/Cargo.toml, and every `git` driver must
 *    have none (its dep/feature lines are build-time injected inside the
 *    `<<driver-*>>` markers; a statically declared git driver would break every
 *    clean checkout because its clone dir does not exist yet).
 *    W8-a P1: the same two assertions run against crates/datazen-server/
 *    Cargo.toml (server-form dep line `workspace = true, optional = true`,
 *    same per-driver feature names, `deploy-drivers` aggregate).
 *
 * Driver codegen (generated.ts / driver_init.rs) is gitignored and not checked.
 */
import { readFileSync, existsSync } from 'fs';
import { resolve, dirname } from 'path';
import { fileURLToPath } from 'url';
import { MANAGED_FILES } from './driver-file-stash.mjs';
import { fileHasInjection } from './driver-stash-precommit.mjs';

const __dirname = dirname(fileURLToPath(import.meta.url));
const DEFAULT_ROOT = resolve(__dirname, '..');

const DESKTOP_CARGO_TOML = 'crates/datazen-desktop/Cargo.toml';
const SERVER_CARGO_TOML = 'crates/datazen-server/Cargo.toml';
const DRIVERS_REGISTRY = 'drivers-registry.json';

/** Marker blocks whose content is build-time injected, not static. */
const MARKER_BLOCKS = ['driver-dependencies', 'driver-features'];

/**
 * Strip the content of `# <<name>> … # <</name>>` marker blocks so only the
 * static (hand-maintained) part of the manifest is judged.
 * @param {string} content
 * @returns {string}
 */
export function stripMarkerBlocks(content) {
  let out = content;
  for (const name of MARKER_BLOCKS) {
    const re = new RegExp(`# <<${name}>>[\\s\\S]*?# <</${name}>>`, 'g');
    out = out.replace(re, `# <<${name}>>\n# <</${name}>>`);
  }
  return out;
}

/**
 * The dep line the static declaration must carry for a path driver.
 * Kept in one place so the generator (resolve-drivers.mjs's format), the
 * manifest and this check agree on the exact shape.
 * @param {string} id
 * @returns {RegExp}
 */
function staticDepRe(id) {
  return new RegExp(
    `^datazen-driver-${id} = \\{ path = "\\.\\./drivers/${id}", optional = true \\}$`,
    'm',
  );
}

/**
 * Server-form dep line (W8-a P1): path comes from the root workspace table,
 * the member line only marks it optional.
 * @param {string} id
 * @returns {RegExp}
 */
function serverDepRe(id) {
  return new RegExp(
    `^datazen-driver-${id} = \\{ workspace = true, optional = true \\}$`,
    'm',
  );
}

/**
 * @param {string} id registry id
 * @param {string} feature cargo feature from the registry
 * @returns {RegExp} the static `driver-* = ["dep:…"]` feature line
 */
function staticFeatureRe(id, feature) {
  return new RegExp(`^${feature} = \\["dep:datazen-driver-${id}"\\]$`, 'm');
}

/**
 * Static-declaration consistency between drivers-registry.json and the desktop
 * Cargo.toml. Pure given the two file contents; separated from fs so tests can
 * feed fixtures.
 *
 * @param {{
 *   registry: Record<string, { source?: string, feature?: string }>,
 *   cargoToml: string,
 *   log?: (...args: unknown[]) => void,
 *   error?: (...args: unknown[]) => void,
 * }} opts
 * @returns {string[]} human-readable problems (empty = consistent)
 */
export function staticDriverDeclarationProblems({
  registry,
  cargoToml,
  log = () => {},
  error = () => {},
}) {
  const problems = [];
  const staticToml = stripMarkerBlocks(cargoToml);

  const pathDrivers = Object.entries(registry).filter(([, m]) => m?.source === 'path');
  const gitDrivers = Object.entries(registry).filter(([, m]) => m?.source === 'git');

  for (const [id, meta] of pathDrivers) {
    if (!meta.feature) {
      problems.push(`path driver "${id}" has no cargo feature in ${DRIVERS_REGISTRY}`);
      continue;
    }
    if (!staticDepRe(id).test(staticToml)) {
      problems.push(`missing static optional dep for path driver "${id}" in ${DESKTOP_CARGO_TOML}`);
    }
    if (!staticFeatureRe(id, meta.feature).test(staticToml)) {
      problems.push(
        `missing static feature "${meta.feature}" for path driver "${id}" in ${DESKTOP_CARGO_TOML}`,
      );
    }
  }

  const aggregate = staticToml.match(/^drivers = \[([^\]]*)\]/m);
  if (!aggregate) {
    problems.push(`missing "drivers" aggregate feature in ${DESKTOP_CARGO_TOML}`);
  } else {
    for (const [, meta] of pathDrivers) {
      if (meta.feature && !aggregate[1].includes(`"${meta.feature}"`)) {
        problems.push(`"drivers" aggregate is missing path-driver feature "${meta.feature}"`);
      }
    }
    for (const [, meta] of gitDrivers) {
      if (meta.feature && aggregate[1].includes(`"${meta.feature}"`)) {
        problems.push(
          `"drivers" aggregate must not enable git-driver feature "${meta.feature}" (build-time injected)`,
        );
      }
    }
  }

  for (const [id] of gitDrivers) {
    if (staticDepRe(id).test(staticToml)) {
      problems.push(
        `git driver "${id}" must NOT be statically declared in ${DESKTOP_CARGO_TOML} ` +
          `(its clone dir does not exist on a clean checkout; injection owns its lines)`,
      );
    }
  }

  if (problems.length === 0) {
    log(
      `[check-managed-stubs] ok static driver declaration (${pathDrivers.length} path / ${gitDrivers.length} git)`,
    );
  } else {
    for (const p of problems) error(`[check-managed-stubs] ${p}`);
  }
  return problems;
}

/**
 * Static-declaration consistency between drivers-registry.json and the server
 * Cargo.toml (W8-a P1). Same shape as the desktop leg; the dep line is the
 * server form (workspace = true, optional = true) and the aggregate feature is
 * `deploy-drivers`. Pure given the two file contents.
 *
 * @param {{
 *   registry: Record<string, { source?: string, feature?: string }>,
 *   cargoToml: string,
 *   log?: (...args: unknown[]) => void,
 *   error?: (...args: unknown[]) => void,
 * }} opts
 * @returns {string[]} human-readable problems (empty = consistent)
 */
export function serverDriverDeclarationProblems({
  registry,
  cargoToml,
  log = () => {},
  error = () => {},
}) {
  const problems = [];
  const staticToml = stripMarkerBlocks(cargoToml);

  const pathDrivers = Object.entries(registry).filter(([, m]) => m?.source === 'path');
  const gitDrivers = Object.entries(registry).filter(([, m]) => m?.source === 'git');

  for (const [id, meta] of pathDrivers) {
    if (!meta.feature) {
      problems.push(`path driver "${id}" has no cargo feature in ${DRIVERS_REGISTRY}`);
      continue;
    }
    if (!serverDepRe(id).test(staticToml)) {
      problems.push(`missing static optional dep for path driver "${id}" in ${SERVER_CARGO_TOML}`);
    }
    if (!staticFeatureRe(id, meta.feature).test(staticToml)) {
      problems.push(
        `missing static feature "${meta.feature}" for path driver "${id}" in ${SERVER_CARGO_TOML}`,
      );
    }
  }

  const aggregate = staticToml.match(/^deploy-drivers = \[([^\]]*)\]/m);
  if (!aggregate) {
    problems.push(`missing "deploy-drivers" aggregate feature in ${SERVER_CARGO_TOML}`);
  } else {
    for (const [, meta] of pathDrivers) {
      if (meta.feature && !aggregate[1].includes(`"${meta.feature}"`)) {
        problems.push(`"deploy-drivers" aggregate is missing path-driver feature "${meta.feature}"`);
      }
    }
    for (const [, meta] of gitDrivers) {
      if (meta.feature && aggregate[1].includes(`"${meta.feature}"`)) {
        problems.push(
          `"deploy-drivers" aggregate must not enable git-driver feature "${meta.feature}" (build-time injected)`,
        );
      }
    }
  }

  for (const [id] of gitDrivers) {
    if (serverDepRe(id).test(staticToml)) {
      problems.push(
        `git driver "${id}" must NOT be statically declared in ${SERVER_CARGO_TOML} ` +
          `(its clone dir does not exist on a clean checkout; injection owns its lines)`,
      );
    }
  }

  if (problems.length === 0) {
    log(
      `[check-managed-stubs] ok server static driver declaration (${pathDrivers.length} path / ${gitDrivers.length} git)`,
    );
  } else {
    for (const p of problems) error(`[check-managed-stubs] ${p}`);
  }
  return problems;
}

/**
 * @param {{
 *   root?: string,
 *   files?: string[],
 *   checkStaticDrivers?: boolean,
 *   registry?: Record<string, unknown>,
 *   desktopCargoToml?: string,
 *   serverCargoToml?: string,
 *   log?: (...args: unknown[]) => void,
 *   error?: (...args: unknown[]) => void,
 * }} [opts]
 * @returns {number} 0 if clean, 1 if any file is missing or injected
 */
export function checkManagedStubs(opts = {}) {
  const root = opts.root ?? DEFAULT_ROOT;
  const files = opts.files ?? MANAGED_FILES;
  const log = opts.log ?? console.log.bind(console);
  const error = opts.error ?? console.error.bind(console);

  let failed = false;
  for (const rel of files) {
    const path = resolve(root, rel);
    if (!existsSync(path)) {
      error(`[check-managed-stubs] missing ${rel}`);
      failed = true;
      continue;
    }
    const content = readFileSync(path, 'utf-8');
    if (fileHasInjection(rel, content)) {
      error(`[check-managed-stubs] ${rel} looks injected (driver deps / ACL present).`);
      error('  Restore with: node scripts/driver-file-stash.mjs restore');
      failed = true;
      continue;
    }
    log(`[check-managed-stubs] ok ${rel}`);
  }

  if (opts.checkStaticDrivers) {
    let registry = opts.registry;
    let cargoToml = opts.desktopCargoToml;
    let serverCargoToml = opts.serverCargoToml;
    try {
      if (registry == null) {
        registry = JSON.parse(readFileSync(resolve(root, DRIVERS_REGISTRY), 'utf-8'));
      }
      if (cargoToml == null) {
        cargoToml = readFileSync(resolve(root, DESKTOP_CARGO_TOML), 'utf-8');
      }
      if (serverCargoToml == null) {
        serverCargoToml = readFileSync(resolve(root, SERVER_CARGO_TOML), 'utf-8');
      }
    } catch (e) {
      error(
        `[check-managed-stubs] cannot read static-declaration inputs: ${e instanceof Error ? e.message : e}`,
      );
      return 1;
    }
    const problems = staticDriverDeclarationProblems({ registry, cargoToml, log, error });
    if (problems.length > 0) failed = true;
    const serverProblems = serverDriverDeclarationProblems({
      registry,
      cargoToml: serverCargoToml,
      log,
      error,
    });
    if (serverProblems.length > 0) failed = true;
  }

  return failed ? 1 : 0;
}

function main() {
  const checkStaticDrivers = process.argv.includes('--check-static-drivers');
  process.exit(checkManagedStubs({ checkStaticDrivers }));
}

if (isDirectInvocation(import.meta.url)) {
  main();
}
