#!/usr/bin/env node
/**
 * Guard: the Host test count quoted in the config, the workflow and the CI
 * matrix doc must equal the count the collector actually produces.
 *
 * Why this is a standalone script and not a vitest case:
 * the only machine-produced count available is the one vitest itself reports,
 * and collecting it is expensive. Measured on this host (driver set `all`,
 * macOS, vitest 4.1.10, node v22.20.0), at the 5714-test / 557-file baseline:
 *
 *   npx vitest list --filesOnly   ->    0.25s,   115 MB peak RSS,   557 lines
 *   npx vitest list               ->   87.31s,   299 MB peak RSS,  5714 lines
 *   this script, cold cache (CI)   ->  126.96s,   300 MB peak RSS,  5714 tests
 *
 * `--filesOnly` is ~349x cheaper but only yields the file count, which nothing
 * in the repo quotes. The test count needs the full collect, so a vitest case
 * asserting it would have to nest a ~87s vitest inside a running vitest: it
 * cannot fit the suite's own 10s `testTimeout` (documented right above it),
 * it would re-transform all ~557 files inside an already-transformed worker,
 * and it would put ~300 MB on top of the pool for over a minute on every
 * `pnpm test:unit` locally — in exchange for checking one integer.
 *
 * So the measurement is a CI step of its own, after the driver-set step, and
 * the cheap half of the contract stays in
 * `scripts/__tests__/run-unit-driver-set.test.ts` (the three quoted numbers
 * must agree with each other). This script supplies the half that cannot be
 * derived from hand-written text: the number the collector actually found.
 *
 * The codegen legs are taken from `planDriverSetCommands` rather than
 * re-listed, so "how to reach the `all` test state" keeps exactly one owner.
 * Re-running them here is what makes this script runnable on its own: the
 * count is a property of the driver set, not of whatever CI last generated.
 *
 * Usage:
 *   node scripts/check-test-count.mjs
 *   node scripts/check-test-count.mjs --drivers=basic
 *
 * Exits 0 when every quoted count matches the measured count, 1 otherwise.
 */

import { spawnSync } from 'child_process';
import { readFileSync } from 'fs';
import { join, resolve } from 'path';
import { fileURLToPath, pathToFileURL } from 'url';

import { DEFAULT_DRIVER_SET, planDriverSetCommands } from './run-unit-driver-set.mjs';

const __dirname = fileURLToPath(new URL('.', import.meta.url));
const ROOT = resolve(__dirname, '..');

/**
 * One line of `vitest list` output: `<file> > <describe> > <test name>`.
 * `.test.mjs` matters — the scripts/ guards are .mjs, and an earlier
 * [jt]sx?-only pattern silently dropped 111 of them.
 */
const LISTED_TEST_LINE = /^\S.*\.(?:test|spec)\.[cm]?[jt]sx?[ \t]+>[ \t]/;

/** ~5.7k lines x ~100 bytes; the default 1 MB cap would truncate silently. */
const MAX_BUFFER = 256 * 1024 * 1024;

/**
 * Every place that states the Host test count, with the surrounding words
 * that make each occurrence unambiguous. A bare number would be matched by
 * any unrelated digit run; anchoring on the phrase means a reworded file
 * fails loudly ("no claim found") instead of quietly guarding nothing.
 *
 * @type {{ id: string, file: string, pattern: RegExp }[]}
 */
export const TEST_COUNT_CLAIMS = [
  {
    id: 'vitest.config.ts',
    file: 'vitest.config.ts',
    pattern: /per-test wall time over all (\d+) Host tests/,
  },
  {
    id: 'ci.yml',
    file: '.github/workflows/ci.yml',
    pattern: /per-test wall time, all (\d+)\s+#\s*tests, --reporter=json\)/,
  },
  {
    id: 'ci-test-matrix.md',
    file: 'docs/development/ci-test-matrix.md',
    pattern: /对全部 (\d+) 个 Host 用例逐条统计/,
  },
];

/**
 * @param {Record<string, string>} sources keyed by the `file` of each claim
 * @returns {{ id: string, file: string, count: number }[]}
 * @throws {Error} when a claim is missing, ambiguous, or not a positive integer
 */
export function extractTestCountClaims(sources) {
  return TEST_COUNT_CLAIMS.map(({ id, file, pattern }) => {
    const text = sources[file];
    if (typeof text !== 'string') throw new Error(`no source text for ${file}`);
    const found = [
      ...text.matchAll(
        new RegExp(
          pattern.source,
          pattern.flags.includes('g') ? pattern.flags : `${pattern.flags}g`,
        ),
      ),
    ];
    if (found.length === 0) {
      throw new Error(
        `${file}: no Host test count found for ${pattern}. ` +
          `The guard is not watching anything if it cannot see the number — ` +
          `re-anchor TEST_COUNT_CLAIMS in scripts/check-test-count.mjs.`,
      );
    }
    if (found.length > 1) {
      throw new Error(
        `${file}: ${found.length} Host test counts match ${pattern}; expected exactly one. ` +
          `Quote the count once, then re-run.`,
      );
    }
    const count = Number(found[0][1]);
    if (!Number.isInteger(count) || count <= 0) {
      throw new Error(
        `${file}: Host test count parsed as ${found[0][1]}, which is not a positive integer`,
      );
    }
    return { id, file, count };
  });
}

/**
 * @param {{ id: string, file: string, count: number }[]} claims
 * @param {number} measured the count the collector reported
 * @returns {string[]} human-readable problems; empty means the guard passes
 */
export function findTestCountProblems(claims, measured) {
  const problems = [];
  for (const claim of claims) {
    if (claim.count !== measured) {
      problems.push(
        `${claim.file} quotes ${claim.count} Host tests, collector measured ${measured}`,
      );
    }
  }
  const distinct = new Set(claims.map((c) => c.count));
  if (distinct.size > 1) {
    problems.push(
      `the quoted counts disagree with each other: ${claims.map((c) => `${c.file}=${c.count}`).join(', ')}`,
    );
  }
  if (claims.length !== TEST_COUNT_CLAIMS.length) {
    problems.push(`expected ${TEST_COUNT_CLAIMS.length} quoted counts, got ${claims.length}`);
  }
  return problems;
}

/**
 * Count what vitest actually collected.
 *
 * Anything on stdout that is not a listed test is a hard failure rather than
 * a skipped line: a stray warning would otherwise be deducted from the count
 * and produce a baffling "off by one" against a correct file.
 *
 * @param {string} root
 * @returns {{ tests: number, files: number }}
 */
export function collectTestCount(root) {
  const plan = planDriverSetCommands({ drivers: DEFAULT_DRIVER_SET, vitestArgs: ['list'], root });
  if (plan.length !== 3) {
    throw new Error(
      `planDriverSetCommands returned ${plan.length} legs; expected 2 codegen legs + 1 vitest leg`,
    );
  }
  const vitestLeg = plan[plan.length - 1];
  if (vitestLeg.args[1] !== 'run') {
    throw new Error(
      `planDriverSetCommands no longer ends in a "vitest run" leg (got ${JSON.stringify(vitestLeg.args.slice(1, 2))}); ` +
        `check-test-count.mjs rewrites that leg to "vitest list" and must be updated with it.`,
    );
  }
  // The plan's tail owns the vitest entry point; only the subcommand changes.
  const listLeg = { cmd: vitestLeg.cmd, args: [vitestLeg.args[0], 'list'] };

  for (const { cmd, args } of plan.slice(0, -1)) {
    const codegen = spawnSync(cmd, args, {
      cwd: root,
      encoding: 'utf8',
      maxBuffer: MAX_BUFFER,
      shell: false,
    });
    if (codegen.error) throw codegen.error;
    if (codegen.status !== 0) {
      const tail = `${codegen.stdout ?? ''}${codegen.stderr ?? ''}`
        .trim()
        .split('\n')
        .slice(-15)
        .join('\n');
      process.stderr.write(`[test-count] codegen leg failed (${args.join(' ')}):\n${tail}\n`);
      process.exit(1);
    }
  }

  const result = spawnSync(listLeg.cmd, listLeg.args, {
    cwd: root,
    encoding: 'utf8',
    maxBuffer: MAX_BUFFER,
    shell: false,
  });
  if (result.error) throw result.error;
  if (result.status !== 0) {
    const tail = `${result.stdout ?? ''}${result.stderr ?? ''}`
      .trim()
      .split('\n')
      .slice(-15)
      .join('\n');
    process.stderr.write(`[test-count] vitest list failed:\n${tail}\n`);
    process.exit(1);
  }

  const lines = (result.stdout ?? '').split('\n').filter((line) => line.trim().length > 0);
  const tests = lines.filter((line) => LISTED_TEST_LINE.test(line));
  if (tests.length !== lines.length) {
    const unexpected = lines.filter((line) => !LISTED_TEST_LINE.test(line)).slice(0, 10);
    process.stderr.write(
      `[test-count] ${lines.length - tests.length} line(s) of vitest list output are not test entries; ` +
        `refusing to count them as absent tests:\n${unexpected.join('\n')}\n`,
    );
    process.exit(1);
  }
  return { tests: tests.length, files: new Set(tests.map((line) => line.split(' > ')[0])).size };
}

function main() {
  const driversArg = process.argv.slice(2).find((a) => a.startsWith('--drivers='));
  if (driversArg && driversArg.slice('--drivers='.length) !== DEFAULT_DRIVER_SET) {
    // The quotes in the three files are stated for the `all` driver set.
    // Measuring another set would report a mismatch that is not a defect.
    process.stderr.write(
      `[test-count] the documented count is for driver set "${DEFAULT_DRIVER_SET}"; ` +
        `refusing to measure "${driversArg.slice('--drivers='.length)}" against it.\n`,
    );
    process.exit(1);
  }

  const sources = Object.fromEntries(
    TEST_COUNT_CLAIMS.map(({ file }) => [file, readFileSync(join(ROOT, file), 'utf8')]),
  );
  const claims = extractTestCountClaims(sources);
  const { tests, files } = collectTestCount(ROOT);
  console.log(`[test-count] driver set: ${DEFAULT_DRIVER_SET}`);
  console.log(`[test-count] measured by \`vitest list\`: ${tests} tests across ${files} files`);
  for (const claim of claims) console.log(`[test-count] quoted in ${claim.file}: ${claim.count}`);

  const problems = findTestCountProblems(claims, tests);
  if (problems.length > 0) {
    process.stderr.write(`[test-count] FAILED:\n${problems.map((p) => `  - ${p}`).join('\n')}\n`);
    process.stderr.write(
      '[test-count] If the suite really changed, re-measure, then update every quoted count in the same change. ' +
        'If it did not, the collector and the quotes have diverged — do not edit the quotes to match a number you have not re-measured.\n',
    );
    process.exit(1);
  }
  console.log(`[test-count] OK: all ${claims.length} quoted counts equal the measured ${tests}`);
}

if (import.meta.url === pathToFileURL(resolve(process.argv[1] ?? '')).href) {
  main();
}
