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
const vitestConfigRaw = readFileSync(resolve(root, 'vitest.config.ts'), 'utf8');
const ciMatrixRaw = readFileSync(resolve(root, 'docs/development/ci-test-matrix.md'), 'utf8');
const packageJsonRaw = readFileSync(resolve(root, 'package.json'), 'utf8');
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

  it('is an explicit soft gate, with the measured basis in an adjacent comment', () => {
    const step = frontendSteps.find((s) => s.run?.includes('test:unit:driver-set'));
    expect(step?.['continue-on-error']).toBe(true);

    // YAML parsing discards comments, so read the rationale off the raw text
    // where a maintainer editing the step will actually see it.
    const stepIndex = ciWorkflowRaw.indexOf(DRIVER_SET_STEP_NAME);
    expect(stepIndex, 'ci.yml must keep the driver-set step name').toBeGreaterThan(-1);
    const rationale = ciWorkflowRaw.slice(Math.max(0, stepIndex - 2600), stepIndex);

    // The comment must justify the soft gate by something that is still true.
    expect(rationale).toMatch(/continue-on-error/);
    expect(rationale).toMatch(/hard gate/);
    // ... and must still carry the measured evidence for the timeout budget.
    expect(rationale).toMatch(/load 48|6-7x/);
  });

  it('does not advertise a blocked list that no longer exists', () => {
    const stepIndex = ciWorkflowRaw.indexOf(DRIVER_SET_STEP_NAME);
    expect(stepIndex).toBeGreaterThan(-1);
    const rationale = ciWorkflowRaw.slice(Math.max(0, stepIndex - 2600), stepIndex);

    // Round 1 recorded two blockers measured on 7f35e1932. They are artifacts
    // of that stale base and are green on feat/platform-p0 @ 7559759b7, so the
    // comment may mention them ONLY to record that they were refuted.
    for (const stale of ['tester_tunnelValidationMatrix', 'ContentView.test.tsx']) {
      if (rationale.includes(stale)) {
        expect(rationale).toMatch(/stale|do not exist|artifacts? of/i);
      }
    }
    // The refutation must be stated, not just implied.
    expect(rationale).toMatch(/7559759b7/);
    expect(rationale).toMatch(/5677 passed/);
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
    // No multiple of a measured figure is quoted here on purpose:
    // docs/development/ci-test-matrix.md §2.1.1 forbids rebuilding the
    // "10s ≈ Nx the measured worst case" derivation, and vitest.config.ts
    // no longer carries it either. What is pinned is the value itself; the
    // numbers it was chosen against are pinned separately, below, so a
    // deleted derivation cannot be smuggled back through either file.
    expect(Number(timeoutMatch?.[1].replaceAll('_', ''))).toBe(10_000);
  });

  it('keeps the recorded measurement basis next to the value', () => {
    const valueIndex = vitestConfigRaw.indexOf('testTimeout:');
    expect(valueIndex).toBeGreaterThan(-1);
    // Anchored on the start of the rationale rather than a character count:
    // the comment grows whenever the measurements are refreshed, and a fixed
    // window silently shrank once already, hiding 1745ms/4392ms.
    const basisIndex = vitestConfigRaw.indexOf("// Vitest's 5s default");
    expect(
      basisIndex,
      'the measurement-basis anchor for testTimeout was renamed or removed',
    ).toBeGreaterThan(-1);
    const rationale = vitestConfigRaw.slice(basisIndex, valueIndex);
    expect(rationale).toContain('1745ms');
    expect(rationale).toContain('4392ms');
    expect(rationale).toContain('11629ms');
  });
});

/**
 * Sentence splitter for the two prose guards below.
 *
 * Wrapped comments are the norm in all three files, so a line-based rule is
 * useless: "…never executed on a real GitHub runner. A green run on an 8-core
 * dev host is / not evidence about a runner…" is a disclaimer whose two halves
 * are on different lines, and a line rule reads it as two claims. Joining
 * first and splitting after fixes that without inventing a parser — `:`/`;`
 * split as well so a clause is not allowed to lend its negation to the clause
 * before it. A period only ends a sentence when whitespace follows, which
 * keeps `vitest.config.ts` and decimals like `7.04` intact.
 */
function splitSentences(text: string): string[] {
  return text
    .replace(/\r\n/g, '\n')
    .split(/\n[ \t]*\n/)
    .flatMap((paragraph) =>
      paragraph.replace(/\n[ \t]*/g, ' ').split(/(?<=[。；;：:])|(?<=\.)(?=\s)/),
    )
    .map((s) => s.trim())
    .filter((s) => s.length > 0);
}

/** The files whose prose makes timing, counting and runner claims. */
const PROSE_CLAIM_FILES = {
  'vitest.config.ts': vitestConfigRaw,
  '.github/workflows/ci.yml': ciWorkflowRaw,
  'docs/development/ci-test-matrix.md': ciMatrixRaw,
} as const;

describe('measured-duration prose in vitest.config.ts', () => {
  /** `2.3x`, `~4.4x`, `2.3 倍`, `4.3倍` — a multiple, not a contention ratio. */
  const MULTIPLE = /(?<![\w.])~?\d+(?:\.\d+)?\s*(?:倍|[x×])(?![\w])/;
  /**
   * Bridge between the two halves of a derivation. Bounded, and forbidden
   * from crossing a digit or a line, so "6-7x oversubscription" next to
   * "p95 329ms" in the same table row is not read as a derivation of it.
   */
  const BRIDGE = '[^\\d\\n]{0,30}';
  const MEASURED_FIGURE = /\d[\d_]*\s*ms|\bmax\b|\bworst\b|最坏|worst-case/i;
  const REFUTATION =
    /不再|已删除|站不住|自相矛盾|不要再|不要按|无法|不在此|no longer|do(?:es)?\s+not|don'?t|forbids?|deleted|stale|self-contradict|invalid/i;
  /**
   * A multiple immediately applied to a bare duration — `3 × 10s = 30s`, the
   * attempts times the configured budget. Two settings multiplied, not a
   * multiple of anything measured, so it is exempt. Anchored on what follows
   * the multiple, so "10s is 2.3x the measured worst case, and a retry costs
   * 3 × 10s" still goes red on its first clause.
   */
  const BUDGET_PRODUCT = /^\d[\d_]*\s*s\b/;
  /** A measured figure on either side of the multiple, within the bridge. */
  const DERIVES = ({ before, after }: { before: string; after: string }) => {
    const figureBefore = new RegExp(`(?:${MEASURED_FIGURE.source})${BRIDGE}$`);
    const figureAfter = new RegExp(`^${BRIDGE}(?:${MEASURED_FIGURE.source})`);
    return figureBefore.test(before) || figureAfter.test(after);
  };
  /**
   * The same derivation, anchored on its other side: a configured budget
   * written in whole seconds — `10s`, `30s` — within reach of the multiple.
   * The figure-side rule alone misses the exact wording F3 deleted, because a
   * percentile label sits between them ("~4.4x its p99.9 of 2299ms") and the
   * bridge is digit-free by design, so it cannot reach across `p99` to the
   * `2299ms`. Seconds carry no such interference: a bare `\d+s` never appears
   * inside a duration reading like `329ms`, so this side stays clear of the
   * contention ratios ("6-7x oversubscription") that the bridge exists to
   * leave alone. `BUDGET_PRODUCT` is checked first — `3 x 10s = 30s` is two
   * settings multiplied, not a ratio of the budget to anything measured.
   */
  const BUDGET_VALUE = /\b\d[\d_]*(?:\.\d+)?\s*s\b(?!\w)/;
  const DERIVES_BUDGET = (before: string, after: string) =>
    new RegExp(`${BUDGET_VALUE.source}${BRIDGE}$`).test(before) ||
    new RegExp(`^${BRIDGE}${BUDGET_VALUE.source}`).test(after);

  /**
   * A multiple quoted in order to refute it is the doc's own ruling, not a
   * violation. It only counts as a refutation when the marker sits as close to
   * the multiple as the figure it derives from does: "…2.3 倍"站不住" quotes
   * the form to knock it down, while a `forbids` three clauses later is
   * banning a different claim and must not launder this one. A sentence-wide
   * test was tried first and let a real "~4.4x its p99.9 of 2299ms" through
   * the very clause that forbids the form.
   */
  const REFUTES = (after: string) => new RegExp(`^${BRIDGE}(?:${REFUTATION.source})`).test(after);

  it('quotes no multiple of a measured duration anywhere it makes timing claims', () => {
    // docs/development/ci-test-matrix.md §2.1.1 deleted the "10_000 ms ≈ 2.3x
    // the measured worst case" derivation and forbade rebuilding it; the
    // config kept a verbatim copy for two commits. This is the guard that
    // makes that deletion stick.
    const offenders: string[] = [];
    for (const [file, text] of Object.entries(PROSE_CLAIM_FILES)) {
      for (const sentence of splitSentences(text)) {
        for (const m of sentence.matchAll(new RegExp(MULTIPLE.source, 'g'))) {
          const before = sentence.slice(0, m.index);
          const after = sentence.slice(m.index + m[0].length).trimStart();
          if (BUDGET_PRODUCT.test(after)) continue;
          if (!DERIVES({ before, after }) && !DERIVES_BUDGET(before, after)) continue;
          if (REFUTES(after)) continue;
          offenders.push(`${file}: ${sentence}`);
          break;
        }
      }
    }
    expect(offenders, offenders.join('\n')).toEqual([]);
  });

  it('keeps the refutation that tells a reader the multiple form is banned', () => {
    // The ban above is satisfiable by deleting the refutation too, leaving a
    // reader no idea why no multiple is quoted. Pin the refutation itself.
    expect(ciMatrixRaw).toMatch(/本文不再给出任何倍数推导/);
    expect(ciMatrixRaw).toMatch(/该推导已删除/);
  });

  it('keeps the one ratio that is real, and scoped to the row that produced it', () => {
    // 4392/5000 = 87.8%. It survives on purpose: it is the alarm that
    // motivated the raise, not a margin claim, and the guard below pins it.
    expect(vitestConfigRaw).toMatch(/88% of a 5s budget/);
    // The sentence is wrapped across two comment lines, so match across the
    // line break rather than assuming the source is one line per sentence.
    expect(vitestConfigRaw).toMatch(/11629ms \(worst[^)]*overall\) exceeds 10s outright/);
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

  it('keeps the stated cost of retry next to the value it costs', () => {
    // `retry` makes a flaky pass indistinguishable from a clean pass, so the
    // cost has to be written where someone raising or reading it will see it.
    const costIndex = vitestConfigRaw.indexOf('COST, stated plainly');
    expect(costIndex, 'the COST declaration next to `retry` was deleted').toBeGreaterThan(-1);
    expect(vitestConfigRaw).toMatch(/reported as PASSED, so an all-green run is NOT evidence/);
    expect(
      retryKeyIndex - costIndex,
      'the COST declaration drifted away from the `retry` key',
    ).toBeLessThan(1200);
  });
});

describe('runner core-count claims', () => {
  /** `8 cores`, `8-core`, `8 核`. A bare "how many cores" is not a claim. */
  const CORE_COUNT = /(?:\b\d+\s*[-–—]?\s*cores?\b|\d+\s*核)/i;
  const RUNNER = /runner/i;
  const DISCLAIMER =
    /\b(?:not|never|no|none|cannot|without|unknown|unspecified|do(?:es)?\s+not|don'?t)\b|不能|不可|不会|不得|不应|不推断|不控制|假定|不依赖|无法|未/;

  it('states no core count as a property of the runner, in any of the three files', () => {
    // KEYWORD HEURISTIC — 防形不防语义. It catches "GitHub runners give 4
    // cores, so 8 workers is fine" and cannot catch a core count written in
    // words ("four cores") or in a sentence this regex cannot see. A human
    // still owns the claim; this only stops the common form coming back.
    const offenders: string[] = [];
    for (const [file, text] of Object.entries(PROSE_CLAIM_FILES)) {
      for (const sentence of splitSentences(text)) {
        if (!CORE_COUNT.test(sentence) || !RUNNER.test(sentence)) continue;
        if (!DISCLAIMER.test(sentence)) offenders.push(`${file}: ${sentence}`);
      }
    }
    expect(offenders, offenders.join('\n')).toEqual([]);
  });

  it('freezes how many core-count claims exist, so a new one has to be argued for', () => {
    // The rule above only constrains sentences that also mention a runner, so
    // a bare "runners are 4-core" split across a line boundary could slip
    // past it. Pinning the surface size closes that gap for the cost of an
    // explicit update whenever the local-host figures are re-measured.
    const surface = Object.fromEntries(
      Object.entries(PROSE_CLAIM_FILES).map(([file, text]) => [
        file,
        splitSentences(text).filter((s) => CORE_COUNT.test(s)).length,
      ]),
    );
    expect(surface).toEqual({
      'vitest.config.ts': 1,
      '.github/workflows/ci.yml': 1,
      'docs/development/ci-test-matrix.md': 2,
    });
  });
});
