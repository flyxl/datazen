/** @vitest-environment node */
import { mkdtempSync, writeFileSync } from 'fs';
import { tmpdir } from 'os';
import { join } from 'path';
import { describe, expect, it } from 'vitest';
import {
  planTsrsCommand,
  planTypecheckCommands,
  readDriverFeatures,
  runTypecheck,
} from '../ci-union-typecheck.mjs';

function seededRoot(features: string[]): string {
  const root = mkdtempSync(join(tmpdir(), 'datazen-union-'));
  writeFileSync(join(root, '.driver-features.json'), JSON.stringify({ features }));
  return root;
}

describe('ci-union-typecheck typecheck plan', () => {
  it('runs the same codegen as pnpm build, then tsc', () => {
    const root = seededRoot(['driver-postgres']);
    const commands = planTypecheckCommands(root).map((c) => c.args.join(' ').replaceAll('\\', '/'));
    expect(commands[0]).toContain('generate-builtin-locales.mjs');
    expect(commands[1]).toContain('generate-menu-labels.mjs');
    expect(commands[2]).toContain('typescript/bin/tsc --noEmit');
    // Invoked through process.execPath so the plan is identical on Windows.
    for (const command of planTypecheckCommands(root)) {
      expect(command.cmd).toBe(process.execPath);
    }
  });

  it('never shells out through a package-manager shim', () => {
    for (const command of planTypecheckCommands(seededRoot([]))) {
      expect(command.args.join(' ')).not.toContain('pnpm');
    }
  });
});

describe('ci-union-typecheck runs the forward-looking engine too', () => {
  it('plans a tsrs step over the same union', () => {
    const root = seededRoot(['driver-postgres']);
    const { command } = planTsrsCommand(root);
    if (!command) {
      // The platform has no tsrs binary. The step is skipped, not faked.
      expect(planTsrsCommand(root).reason).toBeTruthy();
      return;
    }
    expect(command.args.join(' ')).toContain('tsrs');
    expect(command.args.join(' ')).toContain('--noEmit');
    // Same reasoning as the tsc step: invoked through process.execPath so the
    // plan is identical on Windows, macOS and Linux.
    expect(command.cmd).toBe(process.execPath);
    expect(command.args.join(' ')).not.toContain('pnpm');
  });

  it('explains itself when the engine cannot run here', () => {
    // A root with no node_modules at all: tsrs cannot resolve a binary there.
    const empty = mkdtempSync(join(tmpdir(), 'datazen-union-nobins-'));
    const { command, reason } = planTsrsCommand(empty);
    expect(command).toBeNull();
    expect(reason).toBeTruthy();
  });
});

describe('ci-union-typecheck injected driver set', () => {
  it('reads the features that with-driver-inject wrote', () => {
    expect(readDriverFeatures(seededRoot(['driver-postgres', 'driver-superset']))).toEqual([
      'driver-postgres',
      'driver-superset',
    ]);
  });

  it('fails loudly when the union was never injected', () => {
    const root = mkdtempSync(join(tmpdir(), 'datazen-union-empty-'));
    expect(() => readDriverFeatures(root)).toThrow(/\.driver-features\.json/);
    // runTypecheck must not swallow that: typechecking a non-injected tree
    // would silently cover the wrong (empty) driver set.
    expect(() => runTypecheck({ root })).toThrow(/\.driver-features\.json/);
  });

  it('treats a features-less manifest as an empty set', () => {
    const root = mkdtempSync(join(tmpdir(), 'datazen-union-nofeat-'));
    writeFileSync(join(root, '.driver-features.json'), JSON.stringify({}));
    expect(readDriverFeatures(root)).toEqual([]);
  });
});
