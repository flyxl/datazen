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

import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { describe, expect, it } from 'vitest';

import ts from 'typescript';

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

function readExcludePatterns(config: string): string[] {
  const file = path.join(ROOT, config);
  const text = fs.readFileSync(file, 'utf8');
  // tsc's own parser, so the test cannot disagree with the compiler about
  // what the file says (JSON with comments is valid tsconfig input).
  const parsed = ts.parseConfigFileTextToJson(config, text);
  if (parsed.error) {
    throw new Error(`${config} is not parseable: ${ts.flattenDiagnosticMessageText(parsed.error.messageText, ' ')}`);
  }
  const exclude = (parsed.config as { exclude?: unknown }).exclude;
  return Array.isArray(exclude) ? exclude.filter((entry): entry is string => typeof entry === 'string') : [];
}

describe('probe files never enter a TypeScript program', () => {
  for (const config of CONFIGS) {
    it(`${config} excludes the reserved probe names`, () => {
      const exclude = readExcludePatterns(config);
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
      const exclude = readExcludePatterns(config);
      expect(exclude.filter((entry) => !PROBE_EXCLUDES.includes(entry))).toEqual([]);
    }
  });
});
