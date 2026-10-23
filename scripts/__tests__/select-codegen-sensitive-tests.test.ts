/** @vitest-environment node */
import { existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { join, resolve } from 'node:path';
import { tmpdir } from 'node:os';
import { describe, expect, it } from 'vitest';

import {
  CODEGEN_TARGETS,
  globToRegExp,
  isValueImportClause,
  parseVitestTestConfig,
  selectCodegenSensitiveTests,
} from '../select-codegen-sensitive-tests.mjs';

const REPO_ROOT = resolve(import.meta.dirname, '../..');
const realConfigText = readFileSync(join(REPO_ROOT, 'vitest.config.ts'), 'utf-8');

describe('isValueImportClause decides module execution, not type visibility', () => {
  it('erases the forms esbuild erases', () => {
    expect(isValueImportClause('type { X }')).toBe(false);
    expect(isValueImportClause('type X')).toBe(false);
    expect(isValueImportClause('{ type A, type B }')).toBe(false);
    expect(isValueImportClause('{ type A as B }')).toBe(false);
  });

  it('keeps every form that executes the target module', () => {
    expect(isValueImportClause('{ X }')).toBe(true);
    expect(isValueImportClause('{ type A, B }')).toBe(true); // mixed: one value binding is enough
    expect(isValueImportClause('Default')).toBe(true);
    expect(isValueImportClause('Default, { type A }')).toBe(true);
    expect(isValueImportClause('* as ns')).toBe(true);
    expect(isValueImportClause('*')).toBe(true);
    expect(isValueImportClause('{}')).toBe(true); // side-effect form still executes
  });

  it('does not mistake a binding named typeX for the type marker', () => {
    expect(isValueImportClause('{ typeX }')).toBe(true);
  });
});

describe('parseVitestTestConfig reads the real config as text', () => {
  const parsed = parseVitestTestConfig(realConfigText);

  it('finds the test-file include block and only that one', () => {
    expect(parsed.include.length).toBeGreaterThan(0);
    expect(parsed.include).toContain('src/**/*.test.{ts,tsx}');
    // The coverage include array is a different block and must not leak in:
    // none of its entries name test files.
    expect(parsed.include.every((e) => e.includes('.test.'))).toBe(true);
  });

  it('carries the alias map the walk must resolve through', () => {
    expect(parsed.aliases['@datazen/driver-sdk']).toBe('packages/driver-sdk/src/index.ts');
    expect(Object.keys(parsed.aliases).length).toBeGreaterThan(1);
  });

  it('finds the setup files, whose sensitivity would defeat the narrowing', () => {
    expect(parsed.setupFiles).toContain('./src/test/setup.ts');
  });
});

describe('globToRegExp supports the shapes vitest.config.ts uses', () => {
  it('matches depth-0 and nested files for **/ patterns', () => {
    const re = globToRegExp('src/**/*.test.{ts,tsx}');
    expect(re.test('src/a.test.ts')).toBe(true);
    expect(re.test('src/x/y/z.test.tsx')).toBe(true);
    expect(re.test('src/a.ts')).toBe(false);
    expect(re.test('pkg/src/a.test.ts')).toBe(false);
  });
});

interface FixtureOptions {
  /** Extra files to write, as [relative path, content] pairs. */
  extra?: ReadonlyArray<[string, string]>;
  /** Overrides the vitest.config.ts text written into the fixture. */
  configText?: string;
}

/** A miniature repo exercising every edge kind the classifier distinguishes. */
function makeFixtureTree(id: string, options: FixtureOptions = {}): string {
  const root = mkdtempSync(join(tmpdir(), `dz-codegen-sensitive-${id}-`));
  const files: ReadonlyArray<[string, string]> = [
    [
      'vitest.config.ts',
      options.configText ??
        [
          "import { defineConfig } from 'vitest/config';",
          "import { resolve } from 'path';",
          'export default defineConfig({',
          '  resolve: {',
          '    alias: {',
          "      '@datazen/x': resolve(__dirname, 'packages/x/src/index.ts'),",
          '    },',
          '  },',
          '  test: {',
          "    setupFiles: ['./src/test/setup.ts'],",
          '    include: [',
          "      'src/**/*.test.{ts,tsx}',",
          '    ],',
          '  },',
          '});',
        ].join('\n'),
    ],
    ['src/extensions/generated.ts', 'export const G = 1;\n'],
    ['src/test/setup.ts', "import { expect } from 'vitest';\n"],
    // value chain: value.ts executes generated; a value importer of value.ts
    // therefore observes the driver set.
    [
      'src/lib/value.ts',
      "import { G } from '../extensions/generated';\nexport const V = G;\n",
    ],
    [
      'src/__tests__/value.test.ts',
      "import { V } from '../lib/value';\nit('sees V', () => {});\n",
    ],
    [
      'src/__tests__/dynamic.test.ts',
      "it('lazily loads value', async () => { await import('../lib/value'); });\n",
    ],
    [
      'src/__tests__/alias.test.ts',
      "import { V } from '@datazen/x';\nit('sees alias V', () => {});\n",
    ],
    [
      'packages/x/src/index.ts',
      "import { G } from '../../../src/extensions/generated';\nexport const V = G;\n",
    ],
    // erased chains: neither typeonly.ts nor allmarked.ts executes generated,
    // so their importers run identical code under every driver set.
    [
      'src/lib/typeonly.ts',
      "import type { G } from '../extensions/generated';\nexport const T = 1;\n",
    ],
    [
      'src/__tests__/typeonly.test.ts',
      "import { T } from '../lib/typeonly';\nit('sees T', () => {});\n",
    ],
    [
      'src/lib/allmarked.ts',
      "import { type G } from '../extensions/generated';\nexport const A = 1;\n",
    ],
    [
      'src/__tests__/allmarked.test.ts',
      "import { A } from '../lib/allmarked';\nit('sees A', () => {});\n",
    ],
    ...(options.extra ?? []),
  ];
  for (const [rel, content] of files) {
    const abs = join(root, rel);
    mkdirSync(join(abs, '..'), { recursive: true });
    writeFileSync(abs, content);
  }
  return root;
}

describe('selectCodegenSensitiveTests on a fixture tree', () => {
  it('selects value-reachable tests and excludes erased-edge importers', () => {
    const root = makeFixtureTree('clean');
    try {
      const { files, testFileCount } = selectCodegenSensitiveTests({ root, log: () => {} });
      expect(files).toEqual([
        'src/__tests__/alias.test.ts',
        'src/__tests__/dynamic.test.ts',
        'src/__tests__/value.test.ts',
      ]);
      expect(testFileCount).toBe(5);
    } finally {
      rmSync(root, { recursive: true, force: true });
    }
  });

  it('throws when a setup file reaches the codegen target', () => {
    const root = makeFixtureTree('setup-sensitive', {
      extra: [['src/test/setup.ts', "import { G } from '../extensions/generated';\n"]],
    });
    try {
      expect(() => selectCodegenSensitiveTests({ root, log: () => {} })).toThrow(/setup file/);
    } finally {
      rmSync(root, { recursive: true, force: true });
    }
  });

  it('throws when an include pattern matches nothing on disk', () => {
    const root = makeFixtureTree('stale-pattern', {
      configText: [
        "import { defineConfig } from 'vitest/config';",
        'export default defineConfig({',
        '  test: {',
        '    include: [',
        "      'src/**/*.test.{ts,tsx}',",
        "      'nowhere/**/*.test.{ts,tsx}',",
        '    ],',
        '  },',
        '});',
      ].join('\n'),
    });
    try {
      expect(() => selectCodegenSensitiveTests({ root, log: () => {} })).toThrow(
        /matched no file/,
      );
    } finally {
      rmSync(root, { recursive: true, force: true });
    }
  });

  it('throws when no test reaches the codegen target at all', () => {
    const root = makeFixtureTree('empty-selection', {
      configText: [
        "import { defineConfig } from 'vitest/config';",
        'export default defineConfig({',
        '  test: {',
        '    include: [',
        "      'src/**/*.test.{ts,tsx}',",
        '    ],',
        '  },',
        '});',
      ].join('\n'),
      extra: [
        // Overwrite the value chain with an erased chain: nothing executes
        // generated.ts any more, which is a broken state, not an empty run.
        ['src/lib/value.ts', "import type { G } from '../extensions/generated';\nexport const V = 1;\n"],
      ],
    });
    try {
      expect(() => selectCodegenSensitiveTests({ root, log: () => {} })).toThrow(
        /no test file reaches the codegen targets/,
      );
    } finally {
      rmSync(root, { recursive: true, force: true });
    }
  });
});

describe('selectCodegenSensitiveTests on the real tree', () => {
  const { files, testFileCount } = selectCodegenSensitiveTests({
    root: REPO_ROOT,
    log: () => {},
  });

  it('selects a strict subset of the collected suite', () => {
    expect(files.length).toBeGreaterThan(0);
    expect(files.length).toBeLessThan(testFileCount);
    for (const file of files) expect(existsSync(join(REPO_ROOT, file))).toBe(true);
  });

  it('contains the canonical driver-set-sensitive case and only real test files', () => {
    // cellEditorCandidates is the case the driver-set gate exists for: its
    // expectations read DB_REGISTRY quote characters, which basic does not
    // register (see the `sqlserver` literal in that file's history).
    expect(files).toContain('src/lib/__tests__/cellEditorCandidates.test.ts');
    // An .mjs guard test cannot import the TS codegen, so it must never be
    // selected; if it ever is, the walker is matching on text, not edges.
    expect(files).not.toContain('scripts/__tests__/check-version-consistency.test.mjs');
    expect(files.every((f) => /\.test\.(ts|tsx|mjs)$/.test(f))).toBe(true);
  });

  it('keeps the codegen target list pointing at files that exist', () => {
    for (const target of CODEGEN_TARGETS) {
      expect(existsSync(join(REPO_ROOT, target))).toBe(true);
    }
  });
});
