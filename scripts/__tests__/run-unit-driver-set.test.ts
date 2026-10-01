/** @vitest-environment node */
import { readFileSync } from 'node:fs';
import { basename, join, resolve } from 'node:path';
import { describe, expect, it } from 'vitest';
import YAML from 'yaml';
import {
  DEFAULT_DRIVER_SET,
  parseUnitDriverSetArgs,
  planDriverSetCommands,
} from '../run-unit-driver-set.mjs';

const root = resolve(import.meta.dirname, '../..');
const ciWorkflowRaw = readFileSync(resolve(root, '.github/workflows/ci.yml'), 'utf8');
const ciWorkflow = YAML.parse(ciWorkflowRaw);
const DRIVER_SET_STEP_NAME = '- name: Frontend unit tests (all path drivers)';

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

  it('is an explicit soft gate, with the blockers named in an adjacent comment', () => {
    const step = frontendSteps.find((s) => s.run?.includes('test:unit:driver-set'));
    expect(step?.['continue-on-error']).toBe(true);

    // YAML parsing discards comments, so read the rationale off the raw text
    // where a maintainer editing the step will actually see it.
    const stepIndex = ciWorkflowRaw.indexOf(DRIVER_SET_STEP_NAME);
    expect(stepIndex, 'ci.yml must keep the driver-set step name').toBeGreaterThan(-1);
    const rationale = ciWorkflowRaw.slice(Math.max(0, stepIndex - 1600), stepIndex);
    expect(rationale).toContain('tester_tunnelValidationMatrix');
    expect(rationale).toContain('ContentView.test.tsx');
    expect(rationale).toMatch(/continue-on-error|hard gate/);
  });

  it('keeps the `basic` gate too — `all` does not replace it', () => {
    expect(frontendSteps.some((s) => s.run === 'pnpm test:unit')).toBe(true);
  });
});