/**
 * The self-proof harness's own load-bearing predicates.
 *
 * `scripts/platform-arch-selfcheck.mjs` proves the platform boundary guard can
 * still go red: it mutates the real tree, re-runs the guard, and asserts the
 * guard names the expected rule *and* the expected crate each time. Two of its
 * helpers decide whether that proof counts:
 *
 *   * `violationMentions` — "did the guard report THIS rule for THIS crate?"
 *     A matcher that only looked at the rule id would accept a run that went
 *     red for an unrelated reason, and the suite would read as a pass while
 *     proving nothing.
 *   * `declaredInTable` — "did the probe land in the TOML table the rule is
 *     about?" M2/M3/M5 shipped broken because the answer was "the file mentions
 *     the dependency", which an end-of-file append satisfies just as well as a
 *     `[dependencies]` entry.
 *
 * Both are pure functions over strings, so they are pinned here — no cargo, no
 * tree mutation. The mutations themselves are exercised by
 * `pnpm test:platform-arch:mutations`, which needs a Rust toolchain and
 * therefore cannot live in the cargo-free script suite.
 */

import { readFileSync } from 'fs';
import { dirname, join, resolve } from 'path';
import { fileURLToPath } from 'url';
import { describe, expect, it } from 'vitest';

import { RULES, SPEC_PATH } from '../check-platform-crate-boundaries.mjs';
import { MUTATIONS, declaredInTable, violationMentions } from '../platform-arch-selfcheck.mjs';

const REPO_ROOT = resolve(dirname(fileURLToPath(import.meta.url)), '../..');

describe('violationMentions', () => {
  const stdout = [
    '[check-platform-arch] VIOLATION F-01  datazen-driver-redis (packages/drivers/redis/Cargo.toml) → tauri in its normal+build closure [crate family]: drivers stay host-free and platform-free.',
    '[check-platform-arch] ok F-01: datazen-driver-redis',
  ].join('\n');

  it('accepts a violation line that names the rule and the crate', () => {
    expect(violationMentions(stdout, 'F-01', 'datazen-driver-redis')).toBe(true);
  });

  it('rejects a violation that names a different crate', () => {
    // The negative that keeps the matcher honest: "the guard went red" is not
    // the question, "the guard went red *about this crate*" is.
    expect(violationMentions(stdout, 'F-01', 'datazen-driver-postgres')).toBe(false);
  });

  it('rejects a different rule', () => {
    expect(violationMentions(stdout, 'F-02', 'datazen-driver-redis')).toBe(false);
  });

  it('does not accept an advisory or a summary line as a violation', () => {
    // The load-bearing negative. If an `ADVISORY` line or an `ok F-01:` line
    // satisfied the matcher, a guard that stayed green would still "prove"
    // every mutation, and the suite would be a machine for manufacturing
    // green.
    const notAViolation = [
      '[check-platform-arch] ADVISORY F-01?  datazen (src-tauri/Cargo.toml) declares tauri-plugin-webdriver in [dependencies].',
      '[check-platform-arch] ok F-01: datazen-driver-redis',
    ].join('\n');
    expect(violationMentions(notAViolation, 'F-01', 'datazen')).toBe(false);
    expect(violationMentions(notAViolation, 'F-01', 'datazen-driver-redis')).toBe(false);
  });
});

describe('declaredInTable', () => {
  const manifest = [
    '[package]',
    'name = "datazen-driver-redis"',
    '',
    '[dependencies]',
    'tauri = { version = "2", optional = true }',
    '',
    '[dev-dependencies]',
    'tauri-build = { version = "2" }',
    '',
  ].join('\n');

  it('finds a declaration inside the named table', () => {
    expect(declaredInTable(manifest, 'tauri', 'dependencies')).toBe(true);
  });

  it('does not confuse a neighbouring table for the one asked about', () => {
    // A probe that landed in `[dev-dependencies]` is invisible to the guard's
    // closure walk — M2/M3/M5 shipped that way and reported themselves
    // applied while the guard never saw the edge.
    expect(declaredInTable(manifest, 'tauri-build', 'dependencies')).toBe(false);
    expect(declaredInTable(manifest, 'tauri', 'dev-dependencies')).toBe(false);
  });

  it('returns false for a table the manifest does not have', () => {
    expect(declaredInTable(manifest, 'tauri', 'build-dependencies')).toBe(false);
  });
});

describe('the mutation list', () => {
  it('gives every mutation an id, a title, a crate and an apply step', () => {
    for (const mutation of MUTATIONS) {
      expect(mutation.id).toMatch(/^M\d+$/);
      expect(mutation.title.length).toBeGreaterThan(0);
      expect(mutation.crate.length).toBeGreaterThan(0);
      expect(typeof mutation.apply).toBe('function');
    }
  });

  it('has unique ids', () => {
    const ids = MUTATIONS.map((m) => m.id);
    expect(new Set(ids).size).toBe(ids.length);
  });

  it('only names rules the guard actually implements', () => {
    // A mutation naming a rule that does not exist could never be satisfied,
    // so it would sit in the suite as a permanent red — or, if the matcher were
    // loosened, as a permanent fake.
    const implemented = new Set(RULES.map((r) => r.id));
    for (const mutation of MUTATIONS) {
      if (mutation.rule === 'unclassified') continue;
      expect(implemented.has(mutation.rule)).toBe(true);
    }
  });

  it('names a rule that §2.4 actually states', () => {
    const spec = readFileSync(join(REPO_ROOT, SPEC_PATH), 'utf8');
    const rows = new Set(
      spec
        .split('\n')
        .map((line) => line.match(/^\|\s*(F-\d+)\s*\|/)?.[1])
        .filter((id): id is string => id !== undefined),
    );
    expect(rows.size).toBeGreaterThan(0);
    for (const mutation of MUTATIONS) {
      if (mutation.rule === 'unclassified') continue;
      expect(rows.has(mutation.rule)).toBe(true);
    }
  });

  it('carries a mutation for the armed F-01 tauri ban, on both halves of it', () => {
    // F-01's `tauri*` ban has two halves — the resolved closure and the
    // declared-but-dormant edge — and each needs its own mutation: a rule with
    // two halves and one proof proves half of itself. The mutations must also
    // probe *different* crates, or the second is the first one again.
    const tauriMutations = MUTATIONS.filter(
      (m) => m.rule === 'F-01' && /tauri/.test(`${m.title} ${m.crate}`),
    );
    expect(tauriMutations.length).toBeGreaterThanOrEqual(2);
    expect(new Set(tauriMutations.map((m) => m.crate)).size).toBe(tauriMutations.length);
  });
});
