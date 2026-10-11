/**
 * @vitest-environment node
 *
 * The driver-release atomicity gate's command line.
 *
 * This file exists because of a specific false green that shipped. The gate
 * read only `--base=<ref>`. An operator re-running it by hand against a
 * suspicious ref typed `--base <ref>` — the space form — and the flag was
 * dropped **silently**. The run fell back to its default base, compared HEAD
 * against itself, printed `driver releases: 0` and `PROTOCOL_VERSION 5 -> 5`,
 * and **exited 0**.
 *
 * That output is not merely unhelpful, it is inverted: the delivery under
 * review had actually released all 15 path drivers alongside the protocol bump.
 * A reviewer reading `releases: 0` concludes the opposite of the truth, and the
 * exit code — the only thing CI acts on — says everything is fine.
 *
 * So the property pinned here is not "the flag exists". It is: **the base the
 * run measured against is the base the caller asked for, whichever spelling
 * they used, and anything the gate does not understand stops the run instead of
 * quietly changing the answer.** Both spellings are asserted to produce
 * byte-identical output, so they cannot drift apart again, and the pinned-base
 * run is asserted to differ from the default-base run, so a silent fallback
 * cannot pass as "the flag worked".
 *
 * These tests run the shipped CLI read-only against this repository, the same
 * way `check-driver-protocol-compat.test.ts` does: the argument-parsing bug
 * under test only exists in the CLI entry point, so testing the exported
 * helper alone would have missed it.
 */

import { spawnSync } from 'node:child_process';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { describe, expect, it } from 'vitest';

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), '..', '..');

/**
 * A ref old enough that the delivery since then released every path driver.
 *
 * Chosen over `HEAD` precisely because the default-base run and this run must
 * NOT look alike: if they did, "the flag was honoured" would be
 * indistinguishable from "the flag was dropped again".
 */
const BASE = 'bab7c67be';

/**
 * Budget for the tests that invoke the real gate with a real base.
 *
 * Measured on this machine: a full run against a pinned base is ~6s (it runs
 * `git diff` across every driver crate), and the argument-rejection path is
 * ~0.6s because it never reaches git. A test that compares two full runs needs
 * ~12s, past the 10s default, so these tests say so explicitly instead of
 * inheriting a budget that is too small for what they do.
 */
const FULL_RUN_TIMEOUT = 60_000;

interface CliResult {
  status: number;
  stdout: string;
  stderr: string;
}

/**
 * Cache runs by their argument vector.
 *
 * Each invocation spawns node and walks the whole driver set with git; results
 * are deterministic for a given tree and base, so tests needing the *same* run
 * share it instead of paying for it twice. This is why this file does three
 * full runs rather than seven.
 */
const cache = new Map<string, CliResult>();

/** Run the shipped gate, read-only, against this repository. */
function runCli(args: string[]): CliResult {
  const key = JSON.stringify(args);
  const hit = cache.get(key);
  if (hit !== undefined) return hit;
  const res = spawnSync(process.execPath, ['scripts/check-driver-protocol-release.mjs', ...args], {
    cwd: ROOT,
    encoding: 'utf8',
  });
  const out: CliResult = {
    status: res.status ?? -1,
    stdout: res.stdout ?? '',
    stderr: res.stderr ?? '',
  };
  cache.set(key, out);
  return out;
}

describe('the release gate accepts both spellings of --base', () => {
  it(
    'produces byte-identical output for --base <ref> and --base=<ref>',
    () => {
      const space = runCli(['--base', BASE]);
      const inline = runCli([`--base=${BASE}`]);

      expect(space.status, space.stdout + space.stderr).toBe(0);
      expect(inline.status, inline.stdout + inline.stderr).toBe(0);
      // Not "both contain the same substring" — the whole stream, so the two
      // spellings cannot agree on the base yet still disagree on the verdict.
      expect(space.stdout).toBe(inline.stdout);
    },
    FULL_RUN_TIMEOUT,
  );

  it('measures the ref that was asked for, not the default base', () => {
    const res = runCli(['--base', BASE]);
    expect(res.stdout).toContain(`base ${BASE};`);
    // The exact shape of the original false green: with the base dropped, the
    // gate compares HEAD to itself and the protocol never moves.
    expect(res.stdout).not.toContain('PROTOCOL_VERSION 5 -> 5');
    expect(res.stdout).toContain('driver releases: 15');
  });

  it(
    'a pinned base changes the answer, so the flag is demonstrably load-bearing',
    () => {
      // If the default run and the pinned run agreed, the test above could pass
      // on a gate that ignored --base entirely. They must differ.
      const pinned = runCli(['--base', BASE]);
      const fallback = runCli([]);

      expect(fallback.status, fallback.stdout + fallback.stderr).toBe(0);
      expect(fallback.stdout).toContain('driver releases: 0');
      expect(pinned.stdout).not.toBe(fallback.stdout);
    },
    FULL_RUN_TIMEOUT,
  );
});

describe('the release gate refuses arguments it does not understand', () => {
  it('exits non-zero on an unknown flag instead of ignoring it', () => {
    const res = runCli(['--nosuchflag=1']);
    expect(res.status).not.toBe(0);
    // Exit 2, distinct from the 1 that means "ran and found a violation": a
    // usage error must not reach CI logs filed as a real protocol finding.
    expect(res.status).toBe(2);
    expect(res.stderr).toContain("unknown argument '--nosuchflag=1'");
    expect(res.stderr).toContain('usage:');
  });

  it('exits non-zero on a misspelled base flag rather than falling back', () => {
    const res = runCli(['--bas=bab7c67be']);
    expect(res.status).toBe(2);
    expect(res.stderr).toContain("unknown argument '--bas=bab7c67be'");
    // The tell-tale of the original bug: a clean `ok` line alongside exit 0.
    expect(res.stdout).not.toContain('ok (base');
  });

  it('exits non-zero when --base is given no value', () => {
    const res = runCli(['--base']);
    expect(res.status).toBe(2);
    expect(res.stderr).toContain("argument '--base' requires a value");
    expect(res.stdout).not.toContain('ok (base');
  });

  it('exits non-zero when the space form is given another flag as its value', () => {
    const res = runCli(['--base', '--cwd', '.']);
    expect(res.status).toBe(2);
    expect(res.stderr).toContain("argument '--base' requires a value");
  });

  it('exits non-zero on a repeated flag, which is ambiguous about which ref was meant', () => {
    const res = runCli(['--base', BASE, '--base', 'HEAD']);
    expect(res.status).toBe(2);
    expect(res.stderr).toContain('more than once');
  });
});

describe('the release gate still accepts what it always did', () => {
  it('runs clean with no arguments at all', () => {
    const res = runCli([]);
    expect(res.status, res.stdout + res.stderr).toBe(0);
    expect(res.stdout).toContain('ok (base');
  });

  it(
    'still supports --cwd for fixture repositories',
    () => {
      const res = runCli(['--cwd', ROOT]);
      expect(res.status, res.stdout + res.stderr).toBe(0);
      expect(res.stdout).toContain('ok (base');
    },
    FULL_RUN_TIMEOUT,
  );
});
