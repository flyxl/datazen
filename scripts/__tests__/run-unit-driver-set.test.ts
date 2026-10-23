/** @vitest-environment node */
import { readFileSync } from 'node:fs';
import { basename, join, resolve } from 'node:path';
import { describe, expect, it } from 'vitest';
import YAML from 'yaml';
import { selectCodegenSensitiveTests } from '../select-codegen-sensitive-tests.mjs';
import {
  DEFAULT_DRIVER_SET,
  parseUnitDriverSetArgs,
  planDriverSetCommands,
  resolveVitestArgs,
} from '../run-unit-driver-set.mjs';
const root = resolve(import.meta.dirname, '../..');
const ciWorkflowRaw = readFileSync(resolve(root, '.github/workflows/ci.yml'), 'utf8');
const ciWorkflow = YAML.parse(ciWorkflowRaw);
const vitestConfigRaw = readFileSync(resolve(root, 'vitest.config.ts'), 'utf8');

const flatten = (commands: { args: string[] }[]) =>
  commands.map((c) => c.args.join(' ').replaceAll('\\', '/'));

/** Entry-point script of each planned command, in plan order. */
const planScripts = (drivers: string, vitestArgs: string[] = []) =>
  planDriverSetCommands({ drivers, vitestArgs }).map((c) => basename(c.args[0]));

/** `flatten()` normalises to `/`, so the needles have to as well (Windows). */
const LOCALES_GENERATOR = join('scripts', 'generate-builtin-locales.mjs').replaceAll('\\', '/');

describe('run-unit-driver-set command plan', () => {
  it('runs exactly resolve-drivers -> generate-builtin-locales -> vitest, in that order', () => {
    // Pinned by name *and* by position on purpose: dropping or reordering any
    // leg of the plan must fail here, because on a fresh clone every leg is
    // load-bearing (see the two cases below).
    expect(planScripts('all')).toEqual([
      'resolve-drivers.mjs',
      'generate-builtin-locales.mjs',
      'vitest.mjs',
    ]);
  });

  it('regenerates codegen for the requested set before running vitest', () => {
    const commands = flatten(planDriverSetCommands({ drivers: 'all' }));
    expect(commands[0]).toContain('resolve-drivers.mjs');
    expect(commands[0]).toContain('--codegen-only');
    expect(commands[0]).toContain('--drivers=all');
    expect(commands.at(-1)).toContain('vitest.mjs run');
  });

  it('generates builtinLocales.ts, which the Host suite imports statically', () => {
    // src/locales/index.ts does `import ... from './builtinLocales'`, and
    // builtinLocales.ts is gitignored codegen that nothing on the *test* path
    // generates. On a fresh clone, omitting this leg makes vitest die at import
    // time: measured on feat/platform-p0 @ 7559759b7, 125 of 556 test files fail
    // to load and 110 tests fail.
    const commands = flatten(planDriverSetCommands({ drivers: 'all' }));
    expect(commands.some((c) => c.endsWith(LOCALES_GENERATOR))).toBe(true);

    // It must come after driver resolution and before vitest, or vitest still
    // cannot resolve the import.
    const at = (needle: string) => commands.findIndex((c) => c.includes(needle));
    expect(at('resolve-drivers.mjs')).toBeGreaterThanOrEqual(0);
    expect(at('generate-builtin-locales.mjs')).toBeGreaterThan(at('resolve-drivers.mjs'));
    expect(at('vitest.mjs')).toBeGreaterThan(at('generate-builtin-locales.mjs'));

    // The generator takes no arguments: it reads src/locales/builtin-locales.json
    // and writes src/locales/builtinLocales.ts. Forwarding the driver set here
    // would be a silent no-op masking a future contract change.
    const entry = planDriverSetCommands({ drivers: 'all' }).find((c) =>
      c.args[0].endsWith('generate-builtin-locales.mjs'),
    );
    expect(entry?.args).toHaveLength(1);
  });

  it('uses --codegen-only so Cargo.toml is never touched (no stash/restore needed)', () => {
    for (const command of flatten(planDriverSetCommands({ drivers: 'all' }))) {
      expect(command).not.toContain('driver-file-stash');
      expect(command).not.toContain('--edition');
    }
  });

  it('never shells out through a package-manager shim (identical on Windows)', () => {
    for (const command of planDriverSetCommands({ drivers: 'all' })) {
      expect(command.cmd).toBe(process.execPath);
      expect(command.args.join(' ')).not.toContain('pnpm');
      expect(command.args.join(' ')).not.toContain('npx');
    }
  });

  it('forwards extra argv to vitest without dropping the driver set', () => {
    const commands = flatten(
      planDriverSetCommands({ drivers: 'postgres,mysql', vitestArgs: ['src/foo.test.ts'] }),
    );
    expect(commands[0]).toContain('--drivers=postgres,mysql');
    // Forwarded argv must land on vitest, never on a codegen leg.
    expect(commands.at(-1)).toContain('src/foo.test.ts');
    expect(commands.slice(0, -1).join('\n')).not.toContain('src/foo.test.ts');
  });
});

describe('run-unit-driver-set driver set resolution', () => {
  it('defaults to `all`, the widest set CI can run without credentials', () => {
    expect(DEFAULT_DRIVER_SET).toBe('all');
    expect(parseUnitDriverSetArgs([], {}).drivers).toBe('all');
  });

  it('honours DATAZEN_UNIT_DRIVERS', () => {
    expect(parseUnitDriverSetArgs([], { DATAZEN_UNIT_DRIVERS: 'basic,kiwi' }).drivers).toBe(
      'basic,kiwi',
    );
  });

  it('lets an explicit --drivers flag win over the env var', () => {
    const parsed = parseUnitDriverSetArgs(['--drivers=postgres'], {
      DATAZEN_UNIT_DRIVERS: 'all',
    });
    expect(parsed.drivers).toBe('postgres');
    expect(parsed.vitestArgs).toEqual([]);
  });

  it('rejects an empty set instead of silently regenerating nothing', () => {
    expect(() => parseUnitDriverSetArgs(['--drivers='], {})).toThrow(/Empty driver set/);
    expect(() => parseUnitDriverSetArgs([], { DATAZEN_UNIT_DRIVERS: '  ' })).toThrow(
      /Empty driver set/,
    );
  });

  it('keeps unknown argv for vitest', () => {
    expect(parseUnitDriverSetArgs(['--reporter=json'], {}).vitestArgs).toEqual(['--reporter=json']);
  });
});

describe('run-unit-driver-set narrows the vitest leg to codegen-sensitive files', () => {
  // The narrowing criterion lives in select-codegen-sensitive-tests.mjs and
  // has its own suite; these assertions pin the *decision* in this runner:
  // when to narrow, what survives, and that explicit file arguments win.
  const real = selectCodegenSensitiveTests({ root, log: () => {} });

  it('appends the selection when no test-file argument is given', () => {
    const resolved = resolveVitestArgs({ root, log: () => {} });
    expect(resolved.narrowed).toBe(true);
    expect(resolved.vitestArgs).toEqual(real.files);
    // The canonical driver-set-sensitive case must be in: its expectations
    // read DB_REGISTRY quote characters, which `basic` does not register.
    expect(resolved.vitestArgs).toContain('src/lib/__tests__/cellEditorCandidates.test.ts');
    // ...and the run stays a strict subset of the collected suite.
    expect(real.files.length).toBeLessThan(real.testFileCount);
  });

  it('passes explicit test-file arguments through, taking the caller at their word', () => {
    const resolved = resolveVitestArgs({ root, vitestArgs: ['src/foo.test.ts'], log: () => {} });
    expect(resolved.narrowed).toBe(false);
    expect(resolved.vitestArgs).toEqual(['src/foo.test.ts']);
  });

  it('still narrows when only flags are given — flags are not a file list', () => {
    const resolved = resolveVitestArgs({ root, vitestArgs: ['--reporter=json'], log: () => {} });
    expect(resolved.narrowed).toBe(true);
    expect(resolved.vitestArgs[0]).toBe('--reporter=json');
    expect(resolved.vitestArgs.length).toBe(real.files.length + 1);
  });
});

describe('ci.yml runs the unit suite outside `basic`', () => {
  const frontendSteps = ciWorkflow.jobs.frontend.steps as {
    name?: string;
    run?: string;
    'continue-on-error'?: boolean;
  }[];

  it('has a frontend step invoking the driver-set runner', () => {
    const step = frontendSteps.find((s) => s.run?.includes('test:unit:driver-set'));
    expect(step, 'ci.yml must run the unit suite under a non-basic driver set').toBeDefined();
    // Must not be an inline `--drivers=` in the workflow: the set lives in
    // exactly one place (run-unit-driver-set.mjs) so it cannot drift per-job.
    expect(step?.run).toBe('pnpm test:unit:driver-set');
    expect(step?.run).not.toContain('--drivers');
  });

  it('is a hard gate — continue-on-error must stay absent', () => {
    const step = frontendSteps.find((s) => s.run?.includes('test:unit:driver-set'));
    // Absence, not `false`: the field is deleted, so a re-added `false` would
    // also be a regression — it reads as "planned to be soft" to a maintainer.
    expect(step).toBeDefined();
    expect(step).not.toHaveProperty('continue-on-error');
  });

  it('keeps the `basic` gate too — `all` does not replace it', () => {
    expect(frontendSteps.some((s) => s.run === 'pnpm test:unit')).toBe(true);
  });
});

describe('vitest timeout budget', () => {
  const timeoutMatch = vitestConfigRaw.match(/^\s*testTimeout:\s*([\d_]+)\s*,?\s*$/m);

  it('pins testTimeout explicitly instead of inheriting the 5s Vitest default', () => {
    expect(
      timeoutMatch,
      'vitest.config.ts must set testTimeout explicitly; the Vitest default 5000 ' +
        'left the measured worst case at 88% of budget',
    ).not.toBeNull();
  });

  it('uses the 10s value that covers the measured duration distribution', () => {
    // What is pinned is the value itself; docs/development/ci-test-matrix.md
    // §2.1.1 records why it was chosen, and the measurement history lives in
    // git — not in prose that tests have to re-assert.
    expect(Number(timeoutMatch?.[1].replaceAll('_', ''))).toBe(10_000);
  });
});

describe('retry budget in vitest.config.ts', () => {
  const retryKeyIndex = vitestConfigRaw.indexOf('\n    retry:');

  it('pins the retry value in the root test block', () => {
    // Without this, dropping the key (falling back to the Vitest default 0)
    // or setting it to 0 is a one-character change nobody notices.
    expect(retryKeyIndex).toBeGreaterThan(-1);
    expect(vitestConfigRaw).toMatch(/^\s{4}retry:\s*2\s*,?\s*$/m);
  });
});
