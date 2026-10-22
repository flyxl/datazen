/**
 * @vitest-environment node
 */

/**
 * The reserved probe files must never enter a TypeScript program.
 *
 * A guard test plants a throwaway probe in the working tree to prove a layer
 * rule is armed, and deletes it again. From tsc's point of view that file is
 * ordinary source, and it breaks the build in two ways:
 *
 * - the probe's body is not necessarily valid TypeScript for the package it
 *   lands in (it names modules the package does not depend on), so `tsc`
 *   reports errors in a file nobody wrote;
 * - the probe can be deleted between tsc listing the file and reading it,
 *   which is TS6053.
 *
 * Both failures belong to the probe, not to the tree, and both cost whoever
 * runs the build a red they cannot explain. The fix is an `exclude` in every
 * tsconfig whose program can see a probe.
 *
 * This gate asserts the *configuration*, not the behaviour: it must not run
 * `tsc --listFiles` (far too slow for a unit test), and the behavioural
 * check belongs to the build itself.
 */

import { spawnSync } from 'node:child_process';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { describe, expect, it } from 'vitest';

import { engineBinPath } from '../typecheck.mjs';

const ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../..');

/** The reserved probe name prefixes, as the probe helper defines them. */
const PROBE_EXCLUDES = ['**/__boundaryProbe__*', '**/__uiBoundaryProbe__*'];

/**
 * The tsconfigs whose program can contain a probe.
 *
 * The root config covers `src` + `packages/*`; the two package configs are
 * separate programs that a developer (or an editor) can run on their own —
 * `tsc -p packages/ui` — and fixing only the root config would leave those
 * doors open.
 */
const CONFIGS = [
  'tsconfig.json',
  'packages/ui/tsconfig.json',
  'packages/backend-client/tsconfig.json',
];

/**
 * The compiler's own view of a config file.
 *
 * TypeScript 7 no longer ships the compiler API: the `typescript` package
 * resolves to `lib/version`, so `ts.parseConfigFileTextToJson` and
 * `ts.flattenDiagnosticMessageText` do not exist, and importing them is a
 * type error rather than a runtime surprise. The remaining way to let a
 * compiler parse its own config is to ask it — `tsc --showConfig` prints the
 * resolved configuration as JSON, which is the same parse the build performs
 * (comments, `extends` and all).
 *
 * Reading the *resolved* config is also the more faithful thing to assert:
 * what matters is which files the compiler keeps out of the program, not
 * which array a file happens to spell.
 */
function resolvedConfig(config: string): { exclude?: unknown } {
  const bin = engineBinPath(ROOT, 'tsc');
  if (!bin) {
    throw new Error('the tsc binary is not installed');
  }
  const result = spawnSync(process.execPath, [bin, '-p', config, '--showConfig'], {
    cwd: ROOT,
    encoding: 'utf-8',
    shell: false,
  });
  if (result.status !== 0) {
    const detail = (result.stderr || '').trim().split('\n')[0] || `exit ${result.status}`;
    throw new Error(`${config} is not parseable: ${detail}`);
  }
  return JSON.parse(result.stdout) as { exclude?: unknown };
}

function excludePatterns(config: string): string[] {
  const exclude = resolvedConfig(config).exclude;
  return Array.isArray(exclude) ? exclude.filter((entry): entry is string => typeof entry === 'string') : [];
}

describe('probe files never enter a TypeScript program', () => {
  for (const config of CONFIGS) {
    it(`${config} excludes the reserved probe names`, () => {
      const exclude = excludePatterns(config);
      for (const pattern of PROBE_EXCLUDES) {
        expect(exclude).toContain(pattern);
      }
    });
  }

  it('keeps the exclusion list to exactly the reserved probe names', () => {
    // An exclude that catches more than the probes would silently drop real
    // sources from the program, which is a worse failure than the one this
    // gate exists to prevent.
    for (const config of CONFIGS) {
      const exclude = excludePatterns(config);
      expect(exclude.filter((entry) => !PROBE_EXCLUDES.includes(entry))).toEqual([]);
    }
  });
});
