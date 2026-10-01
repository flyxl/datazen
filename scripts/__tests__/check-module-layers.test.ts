/** @vitest-environment node */
import { spawnSync } from 'child_process';
import { existsSync, readFileSync, readdirSync } from 'fs';
import { dirname, join, resolve } from 'path';
import { fileURLToPath } from 'url';
import { describe, expect, it } from 'vitest';
import { checkModuleLayers, collectModuleLayerFindings, LAYER_RULES } from '../check-module-layers.mjs';
import {
  checkDriverImportBoundaries,
  createGitIgnorePredicate,
} from '../check-driver-import-boundaries.mjs';
import { SKIP_DIR_NAMES } from '../lib/scanTargets.mjs';
import {
  PROBE_PATHS,
  withProbeLock,
  withTempSourceFile,
  withTempSourceFiles,
} from './boundaryMutation';

/** Collect the messages a run logged, and its exit code. */
function run() {
  const logs: string[] = [];
  const code = checkModuleLayers({ log: (msg: unknown) => logs.push(String(msg)) });
  return { code, logs, output: logs.join('\n') };
}

/**
 * A tracking predicate that un-hides exactly one probe path and defers to the
 * real git state for everything else.
 *
 * `checkDriverImportBoundaries` downgrades a finding to advisory when the file
 * it sits in is gitignored (BUG-008: untracked external trees — git-driver
 * clones, a staged Pro EP — must never block the host gate). The probe files
 * these tests write are themselves gitignored, so without an override the
 * guard would report the probe it just planted as "external", and the
 * comparison being tested would be vacuous.
 *
 * The override used to be `() => false`, i.e. "nothing is untracked". That is
 * broader than the intent: it also re-armed the BUG-008 downgrade for the whole
 * tree, so the moment a Pro checkout was present beside the worktree its
 * `setLocale()` advisories became blocking and this test failed — a failure
 * caused entirely by a tree this test is not about, asserting a global
 * property it had never meant to assert. Forcing exactly one path visible
 * keeps the comparison local to the probe and leaves real tracking state in
 * charge of everything else.
 */
function onlyProbeVisible(probeRel: string): (rel: string) => boolean {
  const realIsIgnored = createGitIgnorePredicate(
    resolve(dirname(fileURLToPath(import.meta.url)), '../..'),
  );
  return (rel: string) => rel !== probeRel && realIsIgnored(rel);
}

/** The design-system rule, asserted to exist so a rename cannot silently drop it. */
function designSystemRule() {
  const rule = LAYER_RULES.find((r) => r.from === 'packages/ui');
  expect(rule).toBeDefined();
  return rule!;
}

describe('checkModuleLayers', () => {
  it('passes on the current tree', () => {
    // Under the probe lock: another suite's mutation probe must not be able to
    // show up in this walk, in either direction.
    const { code, output } = withProbeLock(() => run());
    expect(output).not.toMatch(/violation/);
    expect(code).toBe(0);
  });

  it('guards the shared relation-metadata layer against importing its consumers', () => {
    const rule = LAYER_RULES.find((r) => r.from === 'src/lib/relationMetadata');
    expect(rule).toBeDefined();
    expect(rule!.forbidden).toEqual(
      expect.arrayContaining(['src/components', 'src/stores', 'src/windows', 'src/hooks']),
    );
  });

  it('reports the offending file and target when a rule is violated', () => {
    // A synthetic rule proves the detector actually inspects imports rather than
    // trusting the rule table. Query editor modules import Host library helpers.
    const probe = {
      name: 'probe',
      from: 'src/windows/connection/query',
      forbidden: ['src/lib'],
    };
    LAYER_RULES.push(probe);
    try {
      const { code, output } = run();
      expect(code).toBe(1);
      expect(output).toContain('src/lib');
      expect(output).toContain('probe');
    } finally {
      LAYER_RULES.pop();
    }
  });
});

describe('checkModuleLayers · @datazen/ui must stay a host-free leaf', () => {
  it('declares both halves of the rule: forbidden subtrees and forbidden bare packages', () => {
    const rule = designSystemRule();
    expect(rule.forbidden).toEqual(
      expect.arrayContaining([
        'src',
        'packages/drivers',
        'packages/driver-sdk',
        'packages/wapp-sdk',
        'packages/extension-points',
      ]),
    );
    expect(rule.forbiddenPackages).toEqual(
      expect.arrayContaining(['@tauri-apps/', 'zustand', '@datazen/driver-sdk']),
    );
  });

  // Mutation test against the REAL tree — this is the guard's teeth. Every
  // shape below is a real way the boundary can be broken; a guard that only
  // understood `import … from` would let the dynamic / require / mock ones
  // through, which is precisely how the original `PathInput` regression would
  // have been reintroduced one keystroke after being fixed.
  const SHAPES: Array<{ what: string; body: string; expectedTarget: string }> = [
    {
      what: 'a static import of a Tauri plugin',
      body: "import { open } from '@tauri-apps/plugin-dialog';\nexport const pick = open;\n",
      expectedTarget: '@tauri-apps/…',
    },
    {
      what: 'a dynamic import of a Tauri plugin',
      body: "export const pick = () => import('@tauri-apps/plugin-fs');\n",
      expectedTarget: '@tauri-apps/…',
    },
    {
      what: 'a CommonJS require of a Tauri plugin',
      body: "const dlg = require('@tauri-apps/api/dialog');\nexport default dlg;\n",
      expectedTarget: '@tauri-apps/…',
    },
    {
      what: 'a test-time mock of a host store package',
      body: "vi.mock('zustand', () => ({}));\nexport const noop = true;\n",
      expectedTarget: 'zustand…',
    },
    {
      what: 'a relative climb back into the host src tree',
      body: "import { useSettingsStore } from '../../../src/stores/settingsStore';\nexport const s = useSettingsStore;\n",
      expectedTarget: 'src/stores/settingsStore',
    },
    {
      what: 'a relative climb into a driver package',
      body: "import { redisMeta } from '../../drivers/redis/ui/shared/meta';\nexport default redisMeta;\n",
      expectedTarget: 'packages/drivers/redis/ui/shared/meta',
    },
  ];

  for (const { what, body, expectedTarget } of SHAPES) {
    it(`fails on ${what}`, () => {
      const { code, output } = withTempSourceFile(
        'packages/ui/src/__boundaryProbe__.ts',
        body,
        () => run(),
      );
      expect(code).toBe(1);
      expect(output).toContain('packages/ui/src/__boundaryProbe__.ts');
      expect(output).toContain(expectedTarget);
      expect(output).toContain('shared design system (@datazen/ui) must stay a host-free leaf');
    });
  }

  it('does not fire on the design system’s own imports (no false positives)', () => {
    // Scoped to the probe: the shipped tree is asserted clean by the
    // "passes on the current tree" case above, not by this one.
    const { output } = withTempSourceFile(
      'packages/ui/src/__boundaryProbe__.tsx',
      [
        "import { useState } from 'react';",
        "import { FolderOpen } from 'lucide-react';",
        "import { twMerge } from 'tailwind-merge';",
        "import { Button } from './Button';",
        "import { cn } from '../cn';",
        "import { PathInput } from './PathInput';",
        '// A comment naming @tauri-apps/plugin-dialog must not count.',
        '/* Nor may a block comment: import "zustand" */',
        "const label = 'pick a @tauri-apps/plugin-dialog path';",
        'export const Probe = () => useState(cn(FolderOpen, twMerge(label)));',
        'export { Button, PathInput };',
        '',
      ].join('\n'),
      () => run(),
    );
    expect(output).not.toContain('__boundaryProbe__');
  });
});

describe('checkModuleLayers watches the same file set as the driver boundary guard', () => {
  const TAIURI_IMPORT =
    "import { open } from '@tauri-apps/plugin-dialog';\nexport const pick = open;\n";

  it('ignores vendored and generated directories under the design system', () => {
    // `packages/ui/` ships no `dist/` or `node_modules/`, so a guard that walks
    // them is only wrong the day somebody installs or builds inside the design
    // system — at which point it starts failing this repository's gate on
    // third-party code. `SKIP_DIR_NAMES` is shared with the driver boundary
    // guard for the same reason.
    const { code, output } = withTempSourceFiles(
      [
        ['packages/ui/dist/__boundaryProbe__.js', TAIURI_IMPORT],
        ['packages/ui/node_modules/vendored-lib/__boundaryProbe__.js', TAIURI_IMPORT],
        ['packages/ui/coverage/__boundaryProbe__.js', TAIURI_IMPORT],
      ],
      () => run(),
    );
    expect(code).toBe(0);
    expect(output).not.toMatch(/violation/);
  });

  it('still scans those directories when the rule points straight at them', () => {
    // The control: the skip list is scoped to directory *names* below a rule's
    // `from`, never a blanket "don't look here".
    LAYER_RULES.push({
      name: 'probe',
      from: 'packages/ui/dist',
      forbiddenPackages: ['@tauri-apps/'],
    });
    try {
      const { code } = withTempSourceFiles(
        [['packages/ui/dist/__boundaryProbe__.ts', TAIURI_IMPORT]],
        () => run(),
      );
      expect(code).toBe(1);
    } finally {
      LAYER_RULES.pop();
    }
  });

  // The driver boundary guard scans six extensions; a guard that watches only
  // `.ts`/`.tsx` is a guard with a hole shaped exactly like the thing it is
  // meant to forbid.
  for (const ext of ['.ts', '.tsx', '.js', '.jsx', '.mjs', '.cjs']) {
    it(`scans a ${ext} file under packages/ui`, () => {
      const rel = `packages/ui/src/__boundaryProbe__${ext}`;
      const { code, output } = withTempSourceFile(rel, TAIURI_IMPORT, () => run());
      expect(code).toBe(1);
      expect(output).toContain(rel);
    });
  }

  // The three probes above prove the walk *honours* the skip list; they cannot
  // prove which names are in it, and `build/` is deliberately not among the
  // probed ones (see the gitignore test below). Pin the list itself instead.
  it('skips exactly the vendored and generated directory names', () => {
    expect([...SKIP_DIR_NAMES].sort()).toEqual([
      '.git',
      '.turbo',
      '__snapshots__',
      'build',
      'coverage',
      'dist',
      'node_modules',
      'target',
    ]);
  });

  // A probe outside a gitignored path shows up in `git status -uall` for as
  // long as it lives, which is a `git add -A` away from a committed test
  // artifact. Cheap to assert, expensive to notice by eye.
  it('keeps every probe path invisible to git status', () => {
    for (const rel of PROBE_PATHS) {
      const ignored = spawnSync('git', ['check-ignore', '-q', '--', rel], {
        cwd: resolve(dirname(fileURLToPath(import.meta.url)), '../..'),
      });
      expect({ rel, status: ignored.status }).toEqual({ rel, status: 0 });
    }
  });

  it('reaches the same verdict as check-driver-import-boundaries on one file', () => {
    // The regression this pins: the two guards used to declare their scan
    // targets independently, so one reported a file the other ignored. They
    // are redundant, not a fallback for each other — this asserts they really
    // are looking at the same thing, on the real tree, today.
    const rel = 'packages/ui/src/__boundaryProbe__.ts';
    withTempSourceFile(rel, TAIURI_IMPORT, () => {
      const mine = run();
      const theirs: string[] = [];
      const theirCode = checkDriverImportBoundaries({
        log: (msg: unknown) => theirs.push(String(msg)),
        error: (msg: unknown) => theirs.push(String(msg)),
        // Probe names are gitignored on purpose, so the gitignore downgrade
        // would hide a real finding from the guard being compared with.
        isIgnored: onlyProbeVisible(rel),
      });
      const theirOutput = theirs.join('\n');

      expect(mine.code).toBe(1);
      expect(theirCode).toBe(1);
      expect(mine.output).toContain(rel);
      expect(theirOutput).toContain(rel);
    });
  });

  it('agrees with check-driver-import-boundaries on a skipped file too', () => {
    const rel = 'packages/ui/dist/__boundaryProbe__.ts';
    withTempSourceFiles([[rel, TAIURI_IMPORT]], () => {
      const mine = run();
      const theirs: string[] = [];
      const theirCode = checkDriverImportBoundaries({
        log: (msg: unknown) => theirs.push(String(msg)),
        error: (msg: unknown) => theirs.push(String(msg)),
        // Probe names are gitignored on purpose, so the gitignore downgrade
        // would hide a real finding from the guard being compared with.
        isIgnored: onlyProbeVisible(rel),
      });
      expect(mine.code).toBe(0);
      expect(theirCode).toBe(0);
      expect(theirs.join('\n')).not.toContain(rel);
    });
  });

  /**
   * The two port rules (§8.1), and the reason they are graded differently.
   *
   * `driver-sdk-no-direct-tauri` is **advisory** on this baseline: three
   * `packages/driver-sdk/src/ipc/*.ts` files import `@tauri-apps/api/core` today,
   * and moving them onto `BackendClient` is the frontend track's migration, not
   * the guard's to perform. It is therefore tested for what it must *not* do —
   * block the build on known debt, or lose the findings — rather than for a green
   * tree. A guard that is red forever is a guard people disable; a guard that is
   * red and says so is a debt ledger.
   *
   * `backend-client-transport-agnostic` is the opposite: `packages/backend-client`
   * does not exist here, so the rule is vacuous, and this file proves it is
   * *armed* by planting the package and making it go red.
   */
  describe('port rules', () => {
    const REPO_ROOT = resolve(dirname(fileURLToPath(import.meta.url)), '../..');
    const CLIENT_PROBE = 'packages/backend-client/src/__boundaryProbe__.ts';
    const CLIENT = 'backend-client-transport-agnostic';

    function ruleNamed(name: string) {
      const rule = LAYER_RULES.find((r) => r.name === name);
      expect(rule).toBeDefined();
      return rule!;
    }

    /**
     * The files under `rel` whose raw text imports a Tauri specifier.
     *
     * Deliberately *not* the guard's tokenizer: the guard's job is to decide
     * which channel a needle belongs on, so an expectation derived from that same
     * tokenizer agrees with the guard by construction and can never catch it
     * looking in the wrong channel. A regex over the file as written is an
     * independent statement of "these three files import Tauri today".
     */
    function filesImportingTauri(rel: string): string[] {
      const out: string[] = [];
      const walk = (dir: string) => {
        for (const entry of readdirSync(dir, { withFileTypes: true })) {
          const full = join(dir, entry.name);
          if (entry.isDirectory()) walk(full);
          else if (/\.(ts|tsx|js|jsx|mjs|cjs)$/.test(entry.name)) {
            if (/\bfrom\s+['"]@tauri-apps\//.test(readFileSync(full, 'utf8')))
              out.push(full.slice(REPO_ROOT.length + 1));
          }
        }
      };
      walk(resolve(REPO_ROOT, rel));
      return out.sort();
    }

    /**
     * Plant the file, prove it landed, then run the guard.
     *
     * The lock is taken by `withTempSourceFile` itself. `withProbeLock` is a
     * plain `mkdir` lock and is deliberately not re-entrant — nesting a second
     * acquisition around the helper makes the inner one spin the full 60s
     * timeout against its own pid before failing, so wrapping it here would
     * turn every case in this block into a 60s hang.
     */
    function clientVerdict(body: string) {
      return withTempSourceFile(CLIENT_PROBE, body, () => {
        expect(readFileSync(resolve(REPO_ROOT, CLIENT_PROBE), 'utf8')).toBe(body);
        return { ...run(), findings: collectModuleLayerFindings() };
      });
    }

    it('matches packages as prefixes, never as globs', () => {
      // `forbiddenPackage` uses `startsWith`, so a glob would match nothing at
      // all and the rule would pass for the rest of its life.
      for (const rule of LAYER_RULES) {
        for (const pkg of rule.forbiddenPackages ?? []) {
          expect({ rule: rule.name, pkg, glob: pkg.includes('*') }).toEqual({
            rule: rule.name,
            pkg,
            glob: false,
          });
        }
      }
    });

    it('scans exactly the subtrees §8.1 names', () => {
      expect(ruleNamed(CLIENT).from).toBe('packages/backend-client/src');
      expect(ruleNamed('driver-sdk-no-direct-tauri').from).toBe('packages/driver-sdk/src');
    });

    it('grades the backend client rule blocking and the driver rule advisory', () => {
      expect(ruleNamed(CLIENT).blocking).toBe(true);
      expect(ruleNamed('driver-sdk-no-direct-tauri').blocking).toBe(false);
    });

    describe('driver-sdk-no-direct-tauri (advisory, known debt)', () => {
      it('reports every file that imports Tauri, and none that does not', () => {
        const expected = filesImportingTauri('packages/driver-sdk/src');
        const { advisories, violations } = collectModuleLayerFindings();
        expect(advisories.map((a) => a.file).sort()).toEqual(expected);
        // The point of the grader: these findings exist and the tree is still
        // green. Asserting the debt is reported *and* not enforced is what keeps
        // the day the migration lands from looking like a regression.
        expect(violations.filter((v) => v.rule === 'driver-sdk-no-direct-tauri')).toEqual([]);
        expect(run().code).toBe(0);
      });

      it('says out loud that its findings are non-blocking', () => {
        const { code, output } = run();
        expect(code).toBe(0);
        expect(output).toMatch(/advisory finding\(s\) \(non-blocking\)/);
      });

      it('keeps the findings out of every blocking bucket', () => {
        const { errors, vacuous } = collectModuleLayerFindings();
        expect(errors).toEqual([]);
        expect(vacuous.map((v) => v.rule)).not.toContain('driver-sdk-no-direct-tauri');
      });
    });

    describe(`backend-client-transport-agnostic (${CLIENT})`, () => {
      it('reports the missing package as vacuous instead of claiming a pass', () => {
        if (existsSync(resolve(REPO_ROOT, 'packages/backend-client')))
          throw new Error(
            'packages/backend-client exists now — this case is stale, flip it to the armed case',
          );
        expect(collectModuleLayerFindings().vacuous.map((v) => v.rule)).toContain(CLIENT);
        expect(run().code).toBe(0);
      });

      it('turns that vacuous report into a failure when the rule is required', () => {
        // `pnpm test:layers` does not pass `--require-layers` today, so this is
        // the knob the frontend track turns on the day `packages/backend-client`
        // is created. Pinned so it exists, and is known to work, before it is needed.
        const logs: string[] = [];
        const code = checkModuleLayers({
          log: (msg: unknown) => logs.push(String(msg)),
          requireLayers: [CLIENT],
        });
        expect(code).toBe(2);
      });

      it('stays quiet on a client that only reaches the transport it was handed', () => {
        const { code, findings } = clientVerdict(
          [
            'export interface Transport {',
            '  request(path: string, body: unknown): Promise<unknown>;',
            '}',
            'export class BackendClient {',
            '  constructor(private readonly transport: Transport) {}',
            '}',
            '',
          ].join('\n'),
        );
        expect(findings.vacuous.map((v) => v.rule)).not.toContain(CLIENT);
        expect(findings.violations.filter((v) => v.rule === CLIENT)).toEqual([]);
        expect(code).toBe(0);
      });

      it('stays quiet on a file that documents why it may not use fetch or XMLHttpRequest', () => {
        const { code, findings } = clientVerdict(
          [
            '// This package owns the transport. Do not call fetch( or use',
            '// XMLHttpRequest here — the transport is injected by the host.',
            '/** @see BackendClient */',
            'export const noop = (): void => undefined;',
            '',
          ].join('\n'),
        );
        expect(findings.violations.filter((v) => v.rule === CLIENT)).toEqual([]);
        expect(code).toBe(0);
      });

      it('blocks a direct Tauri import once the package exists', () => {
        const { code, findings } = clientVerdict(
          "import { invoke } from '@tauri-apps/api/core';\nexport const call = invoke;\n",
        );
        const hits = findings.violations.filter((v) => v.rule === CLIENT);
        expect(hits).toHaveLength(1);
        expect(hits[0]).toMatchObject({
          file: CLIENT_PROBE,
          line: 1,
          specifier: '@tauri-apps/api/core',
        });
        expect(code).toBe(1);
      });

      it('blocks a raw network call once the package exists', () => {
        const { code, findings } = clientVerdict(
          'export const load = (url: string): Promise<unknown> => fetch(url);\n',
        );
        const hits = findings.violations.filter((v) => v.rule === CLIENT);
        expect(hits).toHaveLength(1);
        expect(hits[0]).toMatchObject({
          file: CLIENT_PROBE,
          line: 1,
          target: 'fetch(',
          specifier: null,
        });
        expect(code).toBe(1);
      });

      it('blocks XMLHttpRequest, and does not fire on q.refetch()', () => {
        const xhr = clientVerdict(
          'export const open = (): XMLHttpRequest => new XMLHttpRequest();\n',
        );
        expect(xhr.findings.violations.filter((v) => v.rule === CLIENT)).toHaveLength(1);
        expect(xhr.code).toBe(1);

        const innocent = clientVerdict(
          'export interface Q { refetch(): Promise<void> }\nexport const go = (q: Q) => q.refetch();\n',
        );
        expect(innocent.findings.violations.filter((v) => v.rule === CLIENT)).toEqual([]);
        expect(innocent.code).toBe(0);
      });
    });
  });
});
