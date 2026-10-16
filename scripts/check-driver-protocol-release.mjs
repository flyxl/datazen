#!/usr/bin/env node
/**
 * @file Protocol-bump / driver-release atomicity gate.
 *
 * `check-driver-protocol-compat.mjs` already makes the *numbers* reviewable: it
 * proves that a change to a governed contract reconciled `PROTOCOL_VERSION`,
 * `MIN_PROTOCOL_VERSION` and the crate SemVer. What it cannot see is the second
 * half of the sentence in `platform-development-plan.md` §6 P2 —
 *
 *   「协议升级与驱动发布是原子兼容门槛」
 *   (a protocol upgrade and the driver release are an atomic threshold)
 *
 * — because "were the drivers released" is not a number in `lib.rs`. It is a
 * fact about what the delivery did to other Cargo manifests, which is why the
 * same section recorded the gap honestly:
 *
 *   「协议号上移与驱动发布是否在同一次交付中发生属于发布流程约束，脚本无从判定，
 *     因此没有任何门禁覆盖它」
 *
 * This file is that gate. It answers exactly two questions, both decidable from
 * `git diff <base>` plus the working tree, with no network and no human input:
 *
 *   1. Did the protocol move in this delivery, and did any driver ship with it?
 *   2. If a driver did ship, is it bound to the protocol this delivery
 *      publishes — same artifact, same commit, declared number inside the
 *      delivered window?
 *
 * ## What counts as "a driver release"
 *
 * Two kinds, and both are read from files git already tracks, so the set cannot
 * drift from what is actually being shipped:
 *
 *   - a **path** driver released when `[package] version` advances in
 *     `packages/drivers/<id>/Cargo.toml`;
 *   - a **git** driver released when its pinned `ref` moves in
 *     `drivers-registry.json`.
 *
 * Driver identity comes from `drivers-registry.json`, never from a directory
 * listing, so `http-support` (a shared helper crate under `packages/drivers/`)
 * cannot be mistaken for a driver.
 *
 * ## Why there is no new negotiation mechanism
 *
 * Nothing here invents a handshake. The gate reuses the existing
 * `PROTOCOL_VERSION` / `MIN_PROTOCOL_VERSION` window and the existing host-side
 * check (`packages/runtime/src/driver_registry/registry.rs`, §5.1 of
 * `docs/architecture/platform/driver-capability-migration.md`): a driver whose
 * declared `protocol_version()` falls outside `[MIN, PROTOCOL]` is refused at
 * load. Rule `released-driver-outside-delivered-protocol-window` asserts the
 * same predicate at review time, for the releases this delivery performed, so a
 * driver that would be refused at runtime cannot be published in the first
 * place.
 *
 * ## Usage
 *
 *     node scripts/check-driver-protocol-release.mjs [--base <ref> | --base=<ref>] [--cwd <dir>]
 *
 * Exit 0 with an `ok (...)` line, exit 1 with one line per violation on stderr,
 * exit 2 with the usage text if the arguments could not be parsed.
 * `--base` defaults to `merge-base HEAD main` (the same default the sibling
 * compatibility gate uses); `--cwd` exists so the gate can be exercised against
 * a fixture repository instead of only against the real tree.
 *
 * Argument parsing is shared with `check-driver-protocol-compat.mjs`
 * (`parseGateArgs`), and it **refuses what it does not understand**. This gate
 * used to read only `--base=<ref>`: the space form `--base <ref>` was dropped
 * silently, the run fell back to the default base, compared HEAD with itself,
 * printed `driver releases: 0`, and still exited 0 — a false green for a
 * delivery that had actually released 15 drivers. A gate's exit code is its
 * verdict, so a misspelled flag now fails the run (exit 2) instead of quietly
 * changing which range is being measured.
 */

import { spawnSync } from 'node:child_process';
import { existsSync, readFileSync, readdirSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

import {
  GOVERNED_CRATE_MANIFEST,
  MIN_PROTOCOL_VERSION_SYMBOL,
  PROTOCOL_VERSION_FILE,
  PROTOCOL_VERSION_SYMBOL,
  compareCrateVersions,
  gateUsageError,
  parseGateArgs,
  readBlobAtRef,
  readConstValue,
  readCrateVersion,
  resolveBaseRef,
} from './check-driver-protocol-compat.mjs';

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const LOG_PREFIX = '[check-driver-protocol-release]';

/** Driver identity and pin table. See AGENTS.md「驱动选型与双版本构建模式」. */
export const DRIVER_REGISTRY_FILE = 'drivers-registry.json';

/**
 * @param {string[]} args
 * @param {string} cwd
 * @returns {{ status: number, stdout: string, stderr: string }}
 */
function git(args, cwd) {
  const res = spawnSync('git', args, { cwd, encoding: 'utf8' });
  return { status: res.status ?? 1, stdout: res.stdout ?? '', stderr: res.stderr ?? '' };
}

/**
 * @typedef {object} DriverEntry
 * @property {string} id
 * @property {'path' | 'git'} source
 * @property {string | null} path      repo-relative crate directory (path drivers only)
 * @property {string | null} ref       pinned commit (git drivers only)
 */

/**
 * Read the driver registry.
 *
 * A registry that cannot be read is a verdict, not silence: every rule below is
 * derived from the driver set, so an unreadable registry would otherwise make
 * the gate report `ok` while covering nothing.
 *
 * @param {string} cwd
 * @returns {{ ok: boolean, entries: DriverEntry[], reason: string | null }}
 */
export function readDriverRegistry(cwd) {
  const path = join(cwd, DRIVER_REGISTRY_FILE);
  if (!existsSync(path)) {
    return { ok: false, entries: [], reason: `${DRIVER_REGISTRY_FILE} does not exist` };
  }
  /** @type {unknown} */
  let parsed;
  try {
    parsed = JSON.parse(readFileSync(path, 'utf8'));
  } catch (error) {
    return {
      ok: false,
      entries: [],
      reason: `${DRIVER_REGISTRY_FILE} is not valid JSON: ${String(error)}`,
    };
  }
  if (parsed === null || typeof parsed !== 'object' || Array.isArray(parsed)) {
    return { ok: false, entries: [], reason: `${DRIVER_REGISTRY_FILE} is not an object` };
  }
  /** @type {DriverEntry[]} */
  const entries = [];
  for (const [id, value] of Object.entries(parsed)) {
    if (value === null || typeof value !== 'object') continue;
    const source = /** @type {{ source?: unknown }} */ (value).source;
    if (source !== 'path' && source !== 'git') continue;
    const path = /** @type {{ path?: unknown }} */ (value).path;
    const ref = /** @type {{ ref?: unknown }} */ (value).ref;
    entries.push({
      id,
      source,
      path: typeof path === 'string' ? path : null,
      ref: typeof ref === 'string' ? ref : null,
    });
  }
  return { ok: true, entries, reason: null };
}

/**
 * Every `.rs` file under a directory, depth-first.
 *
 * `target/` is skipped: it is build output, not source, and a stale copy there
 * must never decide what protocol number a driver declares.
 *
 * @param {string} dir
 * @returns {Array<{ file: string, text: string }>}
 */
function collectRustSources(dir) {
  /** @type {Array<{ file: string, text: string }>} */
  const out = [];
  /** @type {string[]} */
  const stack = [dir];
  while (stack.length > 0) {
    const current = /** @type {string} */ (stack.pop());
    /** @type {import('node:fs').Dirent[]} */
    let entries;
    try {
      entries = readdirSync(current, { withFileTypes: true });
    } catch {
      continue;
    }
    for (const entry of entries) {
      const full = join(current, entry.name);
      if (entry.isDirectory()) {
        if (entry.name === 'target' || entry.name.startsWith('.')) continue;
        stack.push(full);
        continue;
      }
      if (!entry.isFile() || !entry.name.endsWith('.rs')) continue;
      const text = readFileSync(full, 'utf8');
      // Narrowing here rather than after the walk keeps a crate that does not
      // override anything out of the rest of the pipeline entirely.
      if (!text.includes('protocol_version')) continue;
      out.push({ file: full, text });
    }
  }
  return out;
}

/**
 * Resolve the protocol number a driver crate declares.
 *
 * `DatabaseDriverFactory::protocol_version()` has a default body returning
 * `crate::PROTOCOL_VERSION` (`packages/driver-api/src/factory.rs`), and no path
 * driver overrides it — which is exactly why a protocol bump silently changes
 * what every driver says it speaks. An override is therefore the only place a
 * driver can opt out, so the override is the only thing worth reading.
 *
 * `value` is `null` with `overridden: true` when the override is not a literal
 * (it computes the number some other way). Such a driver is left to the window
 * rule as inherited rather than guessed at: reporting a number this gate did
 * not read would be worse than reporting none.
 *
 * Test files are searched only after production files. A `#[cfg(test)]` module
 * inside a driver crate may well declare a fake factory with its own
 * `protocol_version`, and picking that up would invent an override the crate
 * does not have. The two passes encode the repo's own layout instead — a real
 * `impl DatabaseDriverFactory for X` lives in production code, and `lib.rs` /
 * `*_tests.rs` are where test-only declarations are collected — so the choice
 * is deterministic rather than dependent on directory walk order.
 *
 * @param {Array<{ file: string, text: string }>} sources
 * @returns {{ overridden: boolean, value: number | null, file: string | null }}
 */
export function readDriverDeclaredProtocol(sources) {
  const isTestFile = (file) => {
    const name = file.split(/[\\/]/).pop() ?? file;
    return name === 'lib.rs' || name.endsWith('_tests.rs') || name.startsWith('test_');
  };
  const ordered = [
    ...sources.filter((s) => !isTestFile(s.file)),
    ...sources.filter((s) => isTestFile(s.file)),
  ];
  for (const { file, text } of ordered) {
    const match = /fn\s+protocol_version\s*\(\s*&self\s*\)\s*->\s*u32/.exec(text);
    if (!match) continue;
    const tail = text.slice(match.index, match.index + 400);
    const literal = /\{\s*(\d+)\s*;?\s*\}/.exec(tail) ?? /=>\s*(\d+)/.exec(tail);
    return { overridden: true, value: literal ? Number(literal[1]) : null, file };
  }
  return { overridden: false, value: null, file: null };
}

/**
 * @typedef {object} DriverRelease
 * @property {string} id
 * @property {'path' | 'git'} source
 * @property {string} locator     repo-relative file whose version / pin moved
 * @property {string} before
 * @property {string} after
 * @property {string} reason      what moved, for the report
 */

/**
 * Evaluate the atomicity question over the range `base .. working tree`.
 *
 * @param {{ cwd?: string, base?: string }} [options]
 * @returns {{
 *   base: string,
 *   violations: Array<{ code: string, message: string }>,
 *   releases: DriverRelease[],
 *   protocol: { before: number | null, after: number | null },
 *   minProtocol: { before: number | null, after: number | null },
 *   driverApi: { before: string | null, after: string | null },
 *   recompiled: Array<{ id: string, declaredBefore: number, declaredAfter: number, overridden: boolean }>,
 *   floating: string[],
 * }}
 */
export function evaluate(options = {}) {
  const cwd = options.cwd ?? ROOT;
  const base = resolveBaseRef(cwd, options.base);
  const violations = [];
  /** @type {DriverRelease[]} */
  const releases = [];
  /** @type {Array<{ id: string, declaredBefore: number, declaredAfter: number, overridden: boolean }>} */
  const recompiled = [];
  /** @type {string[]} */
  const floating = [];

  const libBefore = readBlobAtRef(cwd, PROTOCOL_VERSION_FILE, base);
  const libAfter = readFileSync(join(cwd, PROTOCOL_VERSION_FILE), 'utf8');
  const manifestBefore = readBlobAtRef(cwd, GOVERNED_CRATE_MANIFEST, base);
  const manifestAfter = readFileSync(join(cwd, GOVERNED_CRATE_MANIFEST), 'utf8');

  const protocol = {
    before: libBefore === null ? null : readConstValue(libBefore, PROTOCOL_VERSION_SYMBOL),
    after: readConstValue(libAfter, PROTOCOL_VERSION_SYMBOL),
  };
  const minProtocol = {
    before: libBefore === null ? null : readConstValue(libBefore, MIN_PROTOCOL_VERSION_SYMBOL),
    after: readConstValue(libAfter, MIN_PROTOCOL_VERSION_SYMBOL),
  };
  const driverApi = {
    before: manifestBefore === null ? null : readCrateVersion(manifestBefore),
    after: readCrateVersion(manifestAfter),
  };

  if (protocol.after === null || minProtocol.after === null) {
    violations.push({
      code: 'unreadable-version-constants',
      message: `${LOG_PREFIX} could not read ${PROTOCOL_VERSION_SYMBOL} / ${MIN_PROTOCOL_VERSION_SYMBOL} from ${PROTOCOL_VERSION_FILE}`,
    });
    return { base, violations, releases, protocol, minProtocol, driverApi, recompiled, floating };
  }

  const registry = readDriverRegistry(cwd);
  if (!registry.ok) {
    violations.push({
      code: 'driver-registry-unreadable',
      message: `${LOG_PREFIX} ${registry.reason}. Every rule below is derived from the driver set it declares, so the gate would report ok while covering no driver at all.`,
    });
    return { base, violations, releases, protocol, minProtocol, driverApi, recompiled, floating };
  }

  /** @type {Array<{ id: string, declared: number | null, overridden: boolean }>} */
  const declared = [];

  for (const entry of registry.entries) {
    if (entry.source === 'git') {
      const registryAfter = readFileSync(join(cwd, DRIVER_REGISTRY_FILE), 'utf8');
      const beforeRef = readRegistryRefAtRef(cwd, base, entry.id);
      const afterRef = readRegistryRef(entry.id, registryAfter);
      if (beforeRef === null && afterRef === null) {
        // An unpinned git driver tracks its default branch, so there is no
        // reviewable pin to move. It is reported, never counted as a release:
        // counting it would let a floating branch satisfy the atomicity rule
        // for a protocol move it had nothing to do with.
        floating.push(entry.id);
        continue;
      }
      if (beforeRef === null || afterRef === null) {
        violations.push({
          code: 'driver-release-state-unreadable',
          message: `${LOG_PREFIX} git driver '${entry.id}' is pinned at ${beforeRef ?? '<none>'} in ${base} and ${afterRef ?? '<none>'} in the working tree. A pin appearing or disappearing mid-delivery changes what the build fetches without changing any version, so it has to be judged rather than assumed.`,
        });
        continue;
      }
      if (beforeRef !== afterRef) {
        releases.push({
          id: entry.id,
          source: 'git',
          locator: `${DRIVER_REGISTRY_FILE}#${entry.id}.ref`,
          before: beforeRef,
          after: afterRef,
          reason: 'pinned git driver ref moved',
        });
      }
      continue;
    }

    if (entry.path === null) {
      violations.push({
        code: 'driver-release-state-unreadable',
        message: `${LOG_PREFIX} path driver '${entry.id}' declares no 'path' in ${DRIVER_REGISTRY_FILE}, so this gate cannot tell which Cargo manifest to read for its release.`,
      });
      continue;
    }

    const manifestPath = `${entry.path}/Cargo.toml`;
    const beforeManifest = readBlobAtRef(cwd, manifestPath, base);
    const afterManifestPath = join(cwd, entry.path, 'Cargo.toml');
    if (beforeManifest === null || !existsSync(afterManifestPath)) {
      violations.push({
        code: 'driver-release-state-unreadable',
        message: `${LOG_PREFIX} ${manifestPath} ${beforeManifest === null ? `did not exist at ${base}` : 'does not exist in the working tree'}; the release state of driver '${entry.id}' cannot be judged, so an unreleased driver would pass unnoticed.`,
      });
      continue;
    }
    const before = readCrateVersion(beforeManifest);
    const after = readCrateVersion(readFileSync(afterManifestPath, 'utf8'));
    if (before === null || after === null) {
      violations.push({
        code: 'driver-release-state-unreadable',
        message: `${LOG_PREFIX} ${manifestPath} has no readable [package] version at ${before === null ? base : 'head'} for driver '${entry.id}'.`,
      });
      continue;
    }
    if (compareCrateVersions(after, before) > 0) {
      releases.push({
        id: entry.id,
        source: 'path',
        locator: manifestPath,
        before,
        after,
        reason: 'crate version advanced',
      });
    }

    const sources = collectRustSources(join(cwd, entry.path, 'src'));
    const headProtocol = readDriverDeclaredProtocol(sources);
    const headValue = headProtocol.overridden ? headProtocol.value : protocol.after;
    const baseSources = readDriverSourcesAtRef(cwd, entry, base);
    const baseProtocol =
      baseSources === null ? { overridden: false, value: null, file: null } : readDriverDeclaredProtocol(baseSources);
    const baseValue = baseProtocol.overridden ? baseProtocol.value : protocol.before;
    declared.push({ id: entry.id, declared: headValue, overridden: headProtocol.overridden });
    if (headValue !== null && baseValue !== null && headValue !== baseValue) {
      recompiled.push({
        id: entry.id,
        declaredBefore: baseValue,
        declaredAfter: headValue,
        overridden: headProtocol.overridden,
      });
    }
  }

  const protocolMoved =
    protocol.before !== null &&
    (protocol.after !== protocol.before || minProtocol.after !== minProtocol.before);
  const driverApiMoved = compareCrateVersions(driverApi.after, driverApi.before) > 0;

  // Both rules below speak about "the protocol moved", so they must name the same
  // constants. When only MIN_PROTOCOL_VERSION moves, printing
  // "PROTOCOL_VERSION 4 -> 4" reads like the gate lost track of its own state.
  const protocolMovePhrase =
    `${PROTOCOL_VERSION_SYMBOL} ${protocol.before} -> ${protocol.after}` +
    (minProtocol.after !== minProtocol.before
      ? ` and ${MIN_PROTOCOL_VERSION_SYMBOL} ${minProtocol.before} -> ${minProtocol.after}`
      : '');
  // What a driver would have to be "bound to". A MIN-only move changes the
  // window, not the ceiling, so naming PROTOCOL alone would point at the number
  // that did not move.
  const boundToPhrase =
    minProtocol.after !== minProtocol.before
      ? `the ${minProtocol.before} ..= ${protocol.after} window this delivery declares`
      : `protocol ${protocol.after}`;

  // Rule 1 — the headline question: a protocol number nobody shipped with.
  if (protocolMoved && releases.length === 0) {
    const detail = registry.entries
      .map((entry) =>
        floating.includes(entry.id)
          ? `  ${entry.id} (git, unpinned — follows its default branch, so its release is decided at clone time and cannot be judged here)`
          : entry.source === 'git'
            ? `  ${entry.id} (git, pinned ref unchanged)`
            : `  ${entry.id} (${entry.path}, version ${readHeadVersion(cwd, entry)})`,
      )
      .join('\n');
    violations.push({
      code: 'protocol-moved-without-driver-release',
      message: `${LOG_PREFIX} ${protocolMovePhrase} in this delivery, but no driver crate was released with it. The number is declared in ${PROTOCOL_VERSION_FILE} and inherited by every driver that does not override protocol_version(), so raising it alone republishes a contract that no driver was built against. Release the affected drivers in the same commit, or state in the PR which subset is deliberately frozen. Drivers that were NOT released by this delivery:\n${detail}`,
    });
  }

  // Rule 2 — the protocol moved but the crate carrying it was not versioned.
  if (protocolMoved && !driverApiMoved) {
    violations.push({
      code: 'protocol-moved-without-driver-api-release',
      message: `${LOG_PREFIX} ${protocolMovePhrase} ${
        minProtocol.after !== minProtocol.before ? 'were' : 'was'
      } declared in ${PROTOCOL_VERSION_FILE}, but ${GOVERNED_CRATE_MANIFEST} stays at version ${driverApi.after}. The published crate keeps reporting ${PROTOCOL_VERSION_SYMBOL} ${protocol.before} and ${MIN_PROTOCOL_VERSION_SYMBOL} ${minProtocol.before}, so ${
        releases.length === 0
          ? `no driver published in this delivery can be bound to ${boundToPhrase} at all`
          : `the ${releases.length} driver crate(s) released in this delivery cannot be bound to ${boundToPhrase}`
      }: the two numbers live in different artifacts and only one of them was versioned.`,
    });
  }

  // Rule 3 — a driver was published against a crate nobody re-versioned.
  if (releases.length > 0 && !driverApiMoved) {
    const detail = releases.map((r) => `  ${r.id} ${r.before} -> ${r.after} (${r.locator})`).join('\n');
    violations.push({
      code: 'driver-released-without-driver-api-release',
      message: `${LOG_PREFIX} ${releases.length} driver release(s) shipped in this delivery while ${GOVERNED_CRATE_MANIFEST} stays at version ${driverApi.after}:\n${detail}\n  ${PROTOCOL_VERSION_SYMBOL} lives in that crate, so a driver published without it is not bound to any protocol delivery.`,
    });
  }

  // Rule 4 — the same predicate the host applies at load time, at review time.
  //
  // An empty window is a different defect with a different owner (the sibling
  // gate's `protocol-window-non-empty` invariant); when MIN > PROTOCOL every
  // number is trivially "outside" it, and reporting that here would name the
  // drivers instead of the constants that actually broke.
  const windowLo = minProtocol.after;
  const windowHi = protocol.after;
  if (windowLo <= windowHi) {
    for (const release of releases) {
      const driver = declared.find((d) => d.id === release.id);
      if (!driver) continue;
      if (driver.declared === null) continue;
      if (driver.declared >= windowLo && driver.declared <= windowHi) continue;
      violations.push({
        code: 'released-driver-outside-delivered-protocol-window',
        message: `${LOG_PREFIX} driver '${release.id}' was released in this delivery (${release.locator} ${release.before} -> ${release.after}) but declares protocol_version() = ${driver.declared}${driver.overridden ? ' (hard-coded override)' : ''}, outside the window ${MIN_PROTOCOL_VERSION_SYMBOL} ${windowLo} ..= ${PROTOCOL_VERSION_SYMBOL} ${windowHi} this delivery publishes. The host refuses such a driver at load (packages/runtime/src/driver_registry/registry.rs), so publishing it now ships a driver that cannot connect.`,
      });
    }
  }

  // Rule 5 — the strict form of the atomicity threshold.
  //
  // A driver inherits PROTOCOL_VERSION from datazen-driver-api rather than
  // declaring it, so raising that constant changes what every driver says it
  // speaks even when no driver file is touched. Once a driver's declared
  // number has moved, its published crate is stale by definition: the version
  // a consumer pins still reports the old number while linking code that
  // answers the new one. That is exactly the split 「协议升级与驱动发布是原子」
  // forbids, and it is a per-driver fact, so it is reported per driver.
  //
  // Keying on the declared number rather than on "a .rs file changed" is what
  // keeps this rule honest: a delivery can touch hundreds of driver files with
  // no protocol move at all (a refactor, a fix), and rejecting those would be
  // a gate nobody would run.
  for (const entry of recompiled) {
    const release = releases.find((r) => r.id === entry.id);
    if (release) continue;
    violations.push({
      code: 'recompiled-driver-not-released',
      message: `${LOG_PREFIX} driver '${entry.id}' ${
        entry.overridden
          ? `hard-codes protocol_version() and changed it ${entry.declaredBefore} -> ${entry.declaredAfter}`
          : `inherits ${PROTOCOL_VERSION_SYMBOL} from ${GOVERNED_CRATE_MANIFEST}, so its declared protocol moved ${entry.declaredBefore} -> ${entry.declaredAfter}`
      } in this delivery, but ${entry.id}'s [package] version did not advance. A published driver whose protocol number moved without a release is bound to no delivery at all: a consumer pinning the old crate gets a driver that reports the old number while running the new contract. Bump ${entry.id}'s version in the same commit, or keep the old number deliberately.`,
    });
  }

  return { base, violations, releases, protocol, minProtocol, driverApi, recompiled, floating };
}

/**
 * Head `[package] version` for a driver entry, for the reject message that has
 * to enumerate what was *not* released.
 *
 * @param {string} cwd
 * @param {DriverEntry} entry
 * @returns {string}
 */
function readHeadVersion(cwd, entry) {
  if (entry.source !== 'path' || entry.path === null) return '?';
  const manifestPath = join(cwd, entry.path, 'Cargo.toml');
  if (!existsSync(manifestPath)) return '?';
  return readCrateVersion(readFileSync(manifestPath, 'utf8')) ?? '?';
}

/**
 * @param {string} id
 * @param {string} source
 * @returns {string | null}
 */
function readRegistryRef(id, source) {
  /** @type {unknown} */
  let parsed;
  try {
    parsed = JSON.parse(source);
  } catch {
    return null;
  }
  if (parsed === null || typeof parsed !== 'object') return null;
  const entry = /** @type {Record<string, unknown>} */ (parsed)[id];
  if (entry === null || typeof entry !== 'object') return null;
  const ref = /** @type {{ ref?: unknown }} */ (entry).ref;
  return typeof ref === 'string' ? ref : null;
}

/**
 * @param {string} cwd
 * @param {string} ref
 * @param {string} id
 * @returns {string | null}
 */
function readRegistryRefAtRef(cwd, ref, id) {
  const blob = readBlobAtRef(cwd, DRIVER_REGISTRY_FILE, ref);
  return blob === null ? null : readRegistryRef(id, blob);
}

/**
 * The driver's Rust sources as they were at `ref`, or `null` when the crate did
 * not exist there (a brand-new driver, which has no prior declared protocol).
 *
 * @param {string} cwd
 * @param {DriverEntry} entry
 * @param {string} ref
 * @returns {Array<{ file: string, text: string }> | null}
 */
function readDriverSourcesAtRef(cwd, entry, ref) {
  if (entry.source !== 'path' || entry.path === null) return null;
  // `git grep -l` rather than `git ls-tree` + a `git show` per file: a driver
  // crate is hundreds of files, and this gate runs in CI on every push. Only the
  // files that could possibly declare an override are ever read back.
  const listing = git(['grep', '-l', '-e', 'protocol_version', ref, '--', `${entry.path}/src`], cwd);
  if (listing.status !== 0) return [];
  /** @type {Array<{ file: string, text: string }>} */
  const out = [];
  for (const path of listing.stdout
    .split('\n')
    .map((line) => line.trim())
    .filter((line) => line.endsWith('.rs'))) {
    const blob = readBlobAtRef(cwd, path, ref);
    if (blob !== null) out.push({ file: path, text: blob });
  }
  return out;
}

/**
 * @param {string[]} argv
 * @returns {number} process exit code
 */
export function main(argv = process.argv.slice(2)) {
  const parsed = parseGateArgs(argv, { flags: ['--base', '--cwd'] });
  if (!parsed.ok) {
    return gateUsageError(
      LOG_PREFIX,
      'node scripts/check-driver-protocol-release.mjs [--base <ref> | --base=<ref>] [--cwd <dir>]',
      parsed,
    );
  }
  const cwd = parsed.values['--cwd'] === undefined ? ROOT : resolve(parsed.values['--cwd']);
  const result = evaluate({ cwd, base: parsed.values['--base'] });

  const releaseLines = result.releases.map((r) => `  ${r.id} ${r.before} -> ${r.after} (${r.reason})`);
  process.stdout.write(
    `${LOG_PREFIX} driver releases vs ${result.base}:\n${
      releaseLines.length === 0 ? '  (none)' : releaseLines.join('\n')
    }\n`,
  );
  if (result.floating.length > 0) {
    process.stdout.write(
      `${LOG_PREFIX} unpinned git drivers (release decided at clone time, never counted): ${result.floating.join(', ')}\n`,
    );
  }

  if (result.violations.length > 0) {
    process.stderr.write(
      `\nprotocol / driver-release atomicity failed (base ${result.base}):\n${result.violations
        .map((v) => `  ${v.code}: ${v.message.replace(`${LOG_PREFIX} `, '')}`)
        .join('\n')}\n`,
    );
    return 1;
  }

  const window = `${MIN_PROTOCOL_VERSION_SYMBOL} ${result.minProtocol.after}..=${PROTOCOL_VERSION_SYMBOL} ${result.protocol.after}`;
  process.stdout.write(
    `${LOG_PREFIX} ok (base ${result.base}; ${PROTOCOL_VERSION_SYMBOL} ${result.protocol.before} -> ${result.protocol.after}, ${MIN_PROTOCOL_VERSION_SYMBOL} ${result.minProtocol.before} -> ${result.minProtocol.after}, datazen-driver-api ${result.driverApi.before} -> ${result.driverApi.after}; window ${window}; driver releases: ${result.releases.length})\n`,
  );
  return 0;
}

if (process.argv[1] && import.meta.url === `file://${resolve(process.argv[1])}`) {
  process.exit(main());
}