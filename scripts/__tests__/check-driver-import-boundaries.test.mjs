/** @vitest-environment node */
/**
 * Wave 4 `import-guard`: unit tests for
 * `scripts/check-driver-import-boundaries.mjs`.
 *
 * Fixtures are **inline virtual file trees** (the `files` option) so every
 * branch of the guard is pinned without touching the real repo; the last block
 * runs the guard against the actual working tree to prove it is effective where
 * it matters and that the shipped allow-list is exactly the coordinator ruling.
 */
import { describe, expect, it } from 'vitest';
import { dirname, resolve } from 'path';
import { fileURLToPath } from 'url';
import {
  ALLOWLIST,
  EXTERNAL_ADVISORY_NOTE,
  RULES,
  checkDriverImportBoundaries,
  createGitIgnorePredicate,
  resolveSpecifier,
  runCli,
  scanCode,
} from '../check-driver-import-boundaries.mjs';

/** Run the guard on a virtual tree, capturing both output channels. */
function run(files, opts = {}) {
  const out = [];
  const err = [];
  const code = checkDriverImportBoundaries({
    files,
    allowlist: [],
    checkExpiredAllowlist: false,
    log: (msg) => out.push(String(msg)),
    error: (msg) => err.push(String(msg)),
    ...opts,
  });
  return { code, out: out.join('\n'), err: err.join('\n') };
}

// Depth bookkeeping (all fixtures are host climbs from these directories):
//   packages/drivers/redis/ui              → ../../../../src/…
//   packages/drivers/redis/ui/__tests__    → ../../../../../src/…
const DRIVER_UI = 'packages/drivers/redis/ui/ValuePanel.tsx';
const DRIVER_UI_HOST_CLIMB = '../../../../src/hooks/useI18n';
const DRIVER_TEST = 'packages/drivers/redis/ui/__tests__/valuePanel.test.tsx';
const DRIVER_TEST_HOST_CLIMB = '../../../../../src/stores/settingsStore';

describe('scanCode (specifier extraction)', () => {
  it('collects single-quoted, double-quoted and static template literals with lines', () => {
    const { literals } = scanCode(
      ["import a from './a';", 'import b from "./b";', 'const c = () => import(`./c`);'].join('\n'),
    );
    expect(literals).toEqual([
      { value: './a', line: 1 },
      { value: './b', line: 2 },
      { value: './c', line: 3 },
    ]);
  });

  it('skips `${}` templates (computed specifiers cannot be judged statically)', () => {
    const { literals } = scanCode('const m = await import(`./dyn/${name}`);\nimport x from "./keep";');
    expect(literals).toEqual([{ value: './keep', line: 2 }]);
  });

  it('does not let a quote inside a regex literal open a string', () => {
    const { code, literals } = scanCode("const re = /[\"']/g; const s = 'real';");
    expect(literals).toEqual([{ value: 'real', line: 1 }]);
    expect(code).toContain('const re = /["\']/g;');
  });

  it('blanks comments and string bodies while preserving line breaks', () => {
    const source = "// setLocale('en')\n/* block\n   setLocale('fr') */\nsetLocale('de');\n";
    const { code } = scanCode(source);
    const lines = code.split('\n');
    expect(lines).toHaveLength(source.split('\n').length);
    expect(lines[0].trim()).toBe('');
    expect(lines[1].trim()).toBe('');
    expect(lines[2].trim()).toBe('');
    expect(lines[3]).toContain('setLocale(');
  });

  it('tolerates an unterminated block comment and an unterminated string', () => {
    // Matches JavaScript: an unclosed `/* … * /` runs to end of file, so the
    // code below it is comment territory — the guard must not crash on it.
    const { literals, code } = scanCode("const a = /* never closed\nsetLocale();\nconst b = \"dangling\n");
    expect(literals).toEqual([]);
    expect(code.split('\n')).toHaveLength(4);
  });
});

describe('resolveSpecifier', () => {
  it('resolves relative specifiers to repo-relative POSIX paths', () => {
    expect(resolveSpecifier('packages/drivers/redis/ui/a.ts', '../shared/meta')).toBe(
      'packages/drivers/redis/shared/meta',
    );
    expect(resolveSpecifier(DRIVER_UI, DRIVER_UI_HOST_CLIMB)).toBe('src/hooks/useI18n');
    expect(resolveSpecifier('src/test/driverUiSetup.ts', '../../packages/drivers/redis/ui/shared/meta')).toBe(
      'packages/drivers/redis/ui/shared/meta',
    );
  });

  it('returns null for bare package specifiers', () => {
    expect(resolveSpecifier(DRIVER_UI, '@datazen/ui')).toBeNull();
    expect(resolveSpecifier(DRIVER_UI, 'react')).toBeNull();
  });
});

describe('R1 · drivers must not reference host src/', () => {
  it('passes a clean tree and says so', () => {
    const result = run({
      [DRIVER_UI]: "import { useI18n } from '@datazen/ui';\nimport { helpers } from '../shared/helpers';\nexport const x = 1;\n",
    });
    expect(result.code).toBe(0);
    expect(result.out).toContain('ok (1 file(s) scanned');
    expect(result.err).toBe('');
  });

  it('flags a plain `import … from` climb into the host, naming file:line', () => {
    const result = run({
      [DRIVER_UI]: `const a = 1;\nimport { useI18n } from '${DRIVER_UI_HOST_CLIMB}';\n`,
    });
    expect(result.code).toBe(1);
    expect(result.err).toContain(`R1 ${DRIVER_UI}:2`);
    expect(result.err).toContain('resolves to host src/hooks/useI18n');
    expect(result.err).toContain(`import { useI18n } from '${DRIVER_UI_HOST_CLIMB}'`);
    expect(result.err).toContain('§2.1.2');
  });

  it('flags the `vi.mock` / `vi.doMock` string-argument form (the shape a `from`-only grep misses)', () => {
    const result = run({
      [DRIVER_TEST]:
        `import { describe, it, vi } from 'vitest';\n` +
        `vi.mock('${DRIVER_TEST_HOST_CLIMB}');\n` +
        `vi.doMock('../../../../../src/hooks/useI18n', () => ({}));\n`,
    });
    expect(result.code).toBe(1);
    expect(result.err).toContain(`${DRIVER_TEST}:2`);
    expect(result.err).toContain(`${DRIVER_TEST}:3`);
    expect(result.err).toContain('settingsStore');
    expect(result.err).toContain('2 violation(s)');
  });

  it('flags dynamic `import()`, `export … from` and `require()` specifiers', () => {
    const file = 'packages/drivers/mongodb/ui/a.ts';
    const result = run({
      [file]: [
        "export { cn } from '../../../../src/lib/cn';",
        "const lazy = () => import('../../../../src/types');",
        "const legacy = require('../../../../src/stores/panelStore');",
      ].join('\n'),
    });
    expect(result.code).toBe(1);
    for (const line of [1, 2, 3]) expect(result.err).toContain(`${file}:${line}`);
  });

  it('ignores host-looking paths that live in comments; reports them inside real strings', () => {
    const result = run({
      [DRIVER_UI]: [
        `// TODO: replace ${DRIVER_UI_HOST_CLIMB} with @datazen/ui`,
        `/* import x from '${DRIVER_UI_HOST_CLIMB}' */`,
        `export const legacyPath = '${DRIVER_UI_HOST_CLIMB}';`,
      ].join('\n'),
    });
    // A bare string literal cannot be told apart from a specifier, so line 3 is
    // reported — the point of this case is that the two comment shapes (lines
    // 1–2) never count, otherwise the guard would drown in prose.
    expect(result.err).toContain(`${DRIVER_UI}:3`);
    expect(result.err).not.toContain(`${DRIVER_UI}:1`);
    expect(result.err).not.toContain(`${DRIVER_UI}:2`);
  });

  it('leaves driver-internal ../src/ trees alone (they are not the host)', () => {
    const result = run({
      'packages/drivers/mysql/ui/a.ts': "import { helper } from '../src/helper';\n",
      'packages/drivers/mysql/ui/b/c.ts': "import { meta } from '../../src/meta';\n",
    });
    expect(result.code).toBe(0);
  });
});

describe('R2 · only the host may call setLocale()', () => {
  it('flags a call in driver production code', () => {
    const result = run({
      'packages/drivers/redis/ui/console.ts': "const boot = () => {\n  setLocale('zh-CN');\n};\n",
    });
    expect(result.code).toBe(1);
    expect(result.err).toContain('R2 packages/drivers/redis/ui/console.ts:2');
    expect(result.err).toContain('only be called from host src/**');
  });

  it('flags calls in other non-host packages (driver-sdk / extension-points / wapp-sdk)', () => {
    const result = run({
      'packages/driver-sdk/src/x.ts': "setLocale('fr');\n",
      'packages/extension-points/src/y.ts': 'await setLocale(locale);\n',
      'packages/wapp-sdk/src/z.ts': "if (flag) setLocale('de');\n",
    });
    expect(result.code).toBe(1);
    expect(result.err.match(/R2 /g)).toHaveLength(3);
  });

  it('does not flag comments, contract member declarations or bare imports', () => {
    const result = run({
      'packages/drivers/redis/ui/locale.ts': [
        "import { setLocale } from '@datazen/ui';",
        '// the host wires setLocale(settings.language) once',
        '/*',
        " * drivers never call setLocale('en')",
        ' */',
        'export interface LocaleBridge {',
        '  setLocale(locale: string): void;',
        '}',
        'declare function setLocale(next: string): void;',
      ].join('\n'),
    });
    expect(result.code).toBe(0);
    expect(result.err).toBe('');
  });

  it('never flags the i18n runtime owner package (it defines setLocale and tests it)', () => {
    const result = run({
      'packages/ui/src/i18n.ts': 'export function setLocale(locale: string): void {\n  currentLocale = locale;\n}\n',
      'packages/ui/src/__tests__/i18n.test.tsx': "import { setLocale } from '../i18n';\nsetLocale('zh-CN');\n",
    });
    expect(result.code).toBe(0);
  });

  it('keeps the host out of scope (src/** is the legitimate caller)', () => {
    const result = run({
      'src/lib/localeSync.ts': "import { setLocale } from '@datazen/ui';\nsetLocale(next);\n",
    });
    expect(result.code).toBe(0);
  });
});

describe('R3 · host must not import driver internals', () => {
  it('reports host → driver references as advisory findings without failing', () => {
    const result = run({
      'src/test/driverUiSetup.ts': "import '../locales';\nimport '../../packages/drivers/redis/ui/shared/meta';\n",
    });
    expect(result.code).toBe(0);
    expect(result.out).toContain('R3 (advisory) src/test/driverUiSetup.ts:2');
    expect(result.out).toContain('1 advisory finding(s)');
    expect(result.err).toBe('');
  });

  it('exempts the gitignored codegen registries (the sanctioned host → driver edge)', () => {
    const result = run({
      'src/extensions/generated.ts': "import { redisMeta } from '../../packages/drivers/redis/ui/shared/meta';\n",
    });
    expect(result.code).toBe(0);
    expect(result.out).not.toContain('advisory');
  });
});

// BUG-008 blocking scope: rules only fail the gate for source this repository
// tracks. Violations inside gitignored external trees (git-driver clones under
// `packages/drivers/<id>/`, staged Pro EPs under `packages/pro-extensions/`)
// are downgraded to advisory and never absorbed into the allow-list.
describe('tracking-scope classification (BUG-008)', () => {
  const SUPERSET_FILE = 'packages/drivers/superset/ui/SupersetConnectionFields.tsx';
  const EP_TEST_FILE = 'packages/pro-extensions/sql-editor-pro/src/locales/__tests__/locales.test.ts';
  /** One R1 (external clone), one R2 (staged Pro EP) and one R3 (tracked host). */
  const MIXED_TREE = {
    [SUPERSET_FILE]: "import { useI18n } from '../../../../src/hooks/useI18n';\n",
    [EP_TEST_FILE]: "import { setLocale } from '@datazen/ui';\nsetLocale('en');\n",
    'src/test/driverUiSetup.ts': "import '../../packages/drivers/redis/ui/shared/meta';\n",
  };
  const ignoredExternal = (rel) =>
    rel.startsWith('packages/drivers/superset/') || rel.startsWith('packages/pro-extensions/');

  it('downgrades R1/R2 violations in ignored external trees to advisory, exit 0', () => {
    const result = run(MIXED_TREE, { isIgnored: ignoredExternal });
    expect(result.code).toBe(0);
    expect(result.err).toBe('');
    expect(result.out).toContain(`R1 (advisory) ${SUPERSET_FILE}:1`);
    expect(result.out).toContain(`R2 (advisory) ${EP_TEST_FILE}:2`);
    // each downgraded finding is tagged so the developer sees who owns the fix …
    expect(result.out.split(EXTERNAL_ADVISORY_NOTE)).toHaveLength(3); // 2 findings + trailing piece
    // … while the plain R3 host advisory keeps its usual shape
    expect(result.out).toContain('R3 (advisory) src/test/driverUiSetup.ts:1');
    expect(result.out).toContain('3 advisory finding(s)');
  });

  it('keeps the same violations blocking (exit 1) when the files are tracked', () => {
    const result = run(MIXED_TREE, { isIgnored: () => false });
    expect(result.code).toBe(1);
    expect(result.err).toContain(`R1 ${SUPERSET_FILE}:1`);
    expect(result.err).toContain(`R2 ${EP_TEST_FILE}:2`);
    expect(result.err).toContain('FAILED: 2 violation(s)');
    expect(result.err).not.toContain(EXTERNAL_ADVISORY_NOTE);
  });

  it('virtual trees default to all-tracked (no git consulted, gate stays strict)', () => {
    const result = run({ [SUPERSET_FILE]: MIXED_TREE[SUPERSET_FILE] });
    expect(result.code).toBe(1);
    expect(result.err).toContain(`R1 ${SUPERSET_FILE}:1`);
  });

  it('createGitIgnorePredicate answers from the real repository and caches per file', () => {
    const root = resolve(dirname(fileURLToPath(import.meta.url)), '..', '..');
    const isIgnored = createGitIgnorePredicate(root);
    // tracked source ⇒ never ignored, even where a glob would match
    expect(isIgnored('packages/drivers/redis/ui/__tests__/redisKeyWebContextMenu.test.tsx')).toBe(
      false,
    );
    // not tracked + covered by `.gitignore` `/packages/drivers/*` ⇒ external
    expect(isIgnored('packages/drivers/not-a-builtin-driver/ui/probe.tsx')).toBe(true);
    // second call hits the cache and returns the same verdict
    expect(isIgnored('packages/drivers/not-a-builtin-driver/ui/probe.tsx')).toBe(true);
  });

  it('createGitIgnorePredicate fails closed (tracked) when git cannot answer', () => {
    const isIgnored = createGitIgnorePredicate('/tmp/definitely-not-a-datazen-root');
    expect(isIgnored('packages/drivers/superset/ui/probe.tsx')).toBe(false);
  });
});

describe('allow-list', () => {
  const entry = {
    rule: 'R1',
    file: DRIVER_TEST,
    specifier: DRIVER_TEST_HOST_CLIMB,
    reason: 'integration fixture that mounts the host web-menu host',
    milestone: 'Wave 4 import-guard',
  };

  it('suppresses exactly the allow-listed (rule, file, specifier) triple', () => {
    const files = {
      [DRIVER_TEST]: `import { WebContextMenuHost } from '${DRIVER_TEST_HOST_CLIMB}';\n`,
    };
    expect(run(files, { allowlist: [entry] }).code).toBe(0);
    expect(run(files, { allowlist: [{ ...entry, rule: 'R3' }] }).code).toBe(1);
    expect(run(files, { allowlist: [{ ...entry, file: 'packages/drivers/redis/ui/other.tsx' }] }).code).toBe(1);
    expect(run(files, { allowlist: [{ ...entry, specifier: './stale' }] }).code).toBe(1);
  });

  it('reports an exemption whose file disappeared as expired and fails', () => {
    const result = run({ [DRIVER_UI]: "import { useI18n } from '@datazen/ui';\n" }, { allowlist: [entry] });
    const withExpiry = run({ [DRIVER_UI]: "import { useI18n } from '@datazen/ui';\n" }, {
      allowlist: [entry],
      checkExpiredAllowlist: true,
    });
    expect(withExpiry.code).toBe(1);
    expect(withExpiry.err).toContain('expired exemption: R1');
    expect(withExpiry.err).toContain('the file no longer exists');
    // expiry detection is opt-in per run, so virtual-tree tests stay quiet
    expect(result.err).toBe('');
  });

  it('reports an exemption that no longer matches (reference decoupled) as expired', () => {
    const result = run({ [DRIVER_TEST]: "import { useI18n } from '@datazen/ui';\n" }, {
      allowlist: [entry],
      checkExpiredAllowlist: true,
    });
    expect(result.code).toBe(1);
    expect(result.err).toContain('the reference has been decoupled');
  });

  it('stays silent about exemptions that were used', () => {
    const result = run({ [DRIVER_TEST]: `vi.mock('${entry.specifier}');\n` }, {
      allowlist: [entry],
      checkExpiredAllowlist: true,
    });
    expect(result.code).toBe(0);
    expect(result.out).toContain('1 allow-listed reference(s) skipped');
    expect(result.err).toBe('');
  });

  it('shipped allow-list is exactly the two coordinator-ruled redis fixtures, no globs', () => {
    expect(ALLOWLIST).toHaveLength(2);
    for (const item of ALLOWLIST) {
      expect(item.rule).toBe('R1');
      expect(item.file).toBe('packages/drivers/redis/ui/__tests__/redisKeyWebContextMenu.test.tsx');
      expect(item.specifier).toMatch(/^(\.\.\/)+src\/[^*?]+$/);
      expect(item.reason.length).toBeGreaterThan(10);
      expect(item.milestone.length).toBeGreaterThan(3);
    }
    expect(ALLOWLIST.map((i) => i.specifier).sort()).toEqual([
      '../../../../../src/components/ui/WebContextMenu',
      '../../../../../src/stores/contextMenuStore',
    ]);
  });
});

describe('guard plumbing', () => {
  it('refuses to report success when nothing was scanned (exit 2)', () => {
    const result = run({});
    expect(result.code).toBe(2);
    expect(result.err).toContain('no source files scanned');
  });

  it('skips file types the contract says nothing about', () => {
    const result = run({
      'packages/drivers/redis/src/lib.rs': '//! ../../../../src/hooks/useI18n\n',
      'packages/drivers/redis/ui/style.css': "/* @import '../../../../src/styles/x.css'; */\n",
      'packages/drivers/redis/README.md': "import { x } from '../../../../src/lib/y';\n",
    });
    expect(result.code).toBe(0);
  });

  it('keeps R1/R2 blocking while R3 stays advisory pending the coordinator ruling', () => {
    expect([RULES.R1.blocking, RULES.R2.blocking, RULES.R3.blocking]).toEqual([true, true, false]);
  });

  it('runCli forwards argv and honours --root', () => {
    const err = [];
    const code = runCli({
      argv: ['node', 'check-driver-import-boundaries.mjs', '--root=/tmp/definitely-not-a-datazen-root'],
      log: () => {},
      error: (msg) => err.push(String(msg)),
    });
    expect(code).toBe(2);
    expect(err.join('\n')).toContain('no source files scanned');
  });

  it('runCli without --root uses the real repository and passes', () => {
    const out = [];
    const err = [];
    const code = runCli({
      argv: ['node', 'check-driver-import-boundaries.mjs'],
      log: (msg) => out.push(String(msg)),
      error: (msg) => err.push(String(msg)),
    });
    const report = out.join('\n');
    expect(err.join('')).toBe('');
    expect(code).toBe(0);
    // both shipped exemptions are live on the current baseline …
    expect(report).toContain('2 allow-listed reference(s) skipped');
    // … and the known host → driver references stay advisory-only.
    expect(report).toContain('R3 (advisory) src/windows/connection/DocumentConnectionView.tsx:25');
  });

  it('detects an expired exemption against the real file system too', () => {
    const err = [];
    const code = checkDriverImportBoundaries({
      allowlist: [
        ...ALLOWLIST,
        {
          rule: 'R1',
          file: 'packages/drivers/redis/ui/__tests__/deleted-fixture.test.tsx',
          specifier: '../../../../../src/stores/goneStore',
          reason: 'fixture that no longer exists',
          milestone: 'regression probe',
        },
      ],
      log: () => {},
      error: (msg) => err.push(String(msg)),
    });
    expect(code).toBe(1);
    expect(err.join('\n')).toContain('expired exemption: R1 packages/drivers/redis/ui/__tests__/deleted-fixture.test.tsx');
    expect(err.join('\n')).toContain('the file no longer exists');
  });
});
