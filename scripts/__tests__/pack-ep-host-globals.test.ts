/** @vitest-environment node */
import { cpSync, existsSync, mkdirSync, readFileSync, rmSync, writeFileSync } from 'fs';
import { tmpdir } from 'os';
import { join } from 'path';
import { describe, expect, it } from 'vitest';
import {
  assertHostGlobalKeysAllowed,
  collectHostGlobalKeys,
  createDzxArchive,
  dzxFileName,
  HOST_GLOBAL_NAME,
  HOST_SHARED_MODULES,
  packEp,
  readHostGlobalTableKeys,
  ROOT,
  rewriteEpBundleFile,
  rewriteEpImportsToHostGlobals,
  stagePackageTree,
} from '../pack-ep.mjs';
import { sha256File, signEpPackage } from '../sign-ep.mjs';
import { parseHostGlobalTableKeys, writeFixtureExtension } from './pack-ep.fixtures';

// ─────────────────────────────────────────────────────────────────────────────
// Scope: what the packer rewrites, and what the shipped artifact must then prove
// ─────────────────────────────────────────────────────────────────────────────
//
// Packaging itself — args, staging, signing, layout — is in `pack-ep.test.ts`.
// These four blocks sit on the other side of that seam: the import rewriter, the
// shared-module registry it is allowed to rewrite, the asymmetry probe that keeps
// the registry honest, and the host-key invariant the built `.dzx` has to satisfy.
// Split out for size, not for subject: the seam is clean, so a reader of either file
// does not have to hold the other half in mind.
//
// The rationale for why none of this reads `packages/pro-extensions/` is written
// out once, in `pack-ep.test.ts`, and applies to the whole subject. In short: four
// cases in that file used to read the Pro checkout, two of them behind bare
// `if (!existsSync) return` guards, so in a Pro-less checkout they reported green
// for a check that never ran. Each was either a Host-owned rule (kept, re-pointed at
// a synthetic EP) or a Pro-owned one (moved to the Pro repo's own `verify-host-pin.mjs`,
// which runs on every Pro CI and has no path by which the Pro checkout is absent).

describe('rewriteEpImportsToHostGlobals (track B blob loading)', () => {
  const SAMPLE = [
    'import { a as b, c } from "@datazen/extension-points";',
    'import D, { x } from "react";',
    'import * as ns from "@codemirror/view";',
    'import "@datazen/ui";',
    'export { p as q } from "@codemirror/state";',
    'const m = await import("@codemirror/lint");',
    'import rel from "./relative.js";',
    'import abs from "/abs/path.js";',
  ].join('\n');

  it('rewrites every bare import form to the host singleton table', () => {
    const { code, rewritten } = rewriteEpImportsToHostGlobals(SAMPLE);
    expect(rewritten.sort()).toEqual(
      [
        '@codemirror/lint',
        '@codemirror/state',
        '@codemirror/view',
        '@datazen/extension-points',
        '@datazen/ui',
        'react',
      ].sort(),
    );
    // No bare shared-specifier import/export remains.
    for (const spec of HOST_SHARED_MODULES) {
      expect(code).not.toMatch(
        new RegExp(`from\\s*["']${spec.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')}["']`),
      );
    }
    // Relative + absolute specifiers pass through untouched.
    expect(code).toContain('import rel from "./relative.js";');
    expect(code).toContain('import abs from "/abs/path.js";');
    // Default-import interop keeps `.default ?? namespace`.
    expect(code).toContain('.default ?? ');
    // The host global name is referenced, not a bare module.
    expect(code).toContain(`globalThis.${HOST_GLOBAL_NAME}["react"]`);
  });

  it('named default re-export preserves the exported name', () => {
    const { code } = rewriteEpImportsToHostGlobals('export { default as D, v } from "react";');
    expect(code).toContain('export {');
    expect(code).toContain('as D');
    expect(code).toContain('as v');
  });

  it('is idempotent — a rewritten bundle has nothing left to rewrite', () => {
    const once = rewriteEpImportsToHostGlobals(SAMPLE).code;
    const twice = rewriteEpImportsToHostGlobals(once);
    expect(twice.rewritten).toEqual([]);
    expect(twice.code).toBe(once);
  });

  it('throws on unmapped bare specifiers instead of shipping a crashing bundle', () => {
    expect(() => rewriteEpImportsToHostGlobals('import x from "@tauri-apps/api/core";')).toThrow(
      /unmapped bare.*__DATAZEN_HOST__/,
    );
    expect(() => rewriteEpImportsToHostGlobals('export * from "react";')).toThrow(
      /cannot rewrite 'export \* from/,
    );
  });

  it('drops stale sourcemap refs invalidated by the rewrite', () => {
    const { code } = rewriteEpImportsToHostGlobals(
      'const a = 1;\n//# sourceMappingURL=index.esm.js.map\n',
    );
    expect(code).not.toContain('sourceMappingURL');
  });

  it('rewriteEpBundleFile rewrites a bundle file in place', () => {
    const dir = join(tmpdir(), `rewrite-file-${Date.now()}`);
    mkdirSync(dir, { recursive: true });
    const file = join(dir, 'index.esm.js');
    writeFileSync(file, 'import { a } from "@datazen/extension-points";\nexport const v = a;\n');
    const { rewritten } = rewriteEpBundleFile(file);
    expect(rewritten).toEqual(['@datazen/extension-points']);
    expect(readFileSync(file, 'utf8')).toContain(`globalThis.${HOST_GLOBAL_NAME}`);
  });

  it('stagePackageTree ships a rewritten bundle (no stale map, signed after rewrite)', () => {
    const src = join(tmpdir(), `rewrite-stage-src-${Date.now()}`);
    const staged = join(tmpdir(), `rewrite-stage-staged-${Date.now()}`);
    writeFixtureExtension(src);
    writeFileSync(
      join(src, 'dist/index.esm.js'),
      'import { a } from "@datazen/extension-points";\nexport const v = a;\n',
    );
    writeFileSync(join(src, 'dist/index.esm.js.map'), '{"version":3}');
    stagePackageTree(src, staged, { log: () => {} });
    const bundle = readFileSync(join(staged, 'dist/index.esm.js'), 'utf8');
    expect(bundle).toContain(`globalThis.${HOST_GLOBAL_NAME}`);
    // The stale map is not staged (rewrite invalidates its mappings).
    expect(existsSync(join(staged, 'dist/index.esm.js.map'))).toBe(false);
    // Signature covers the rewritten bytes.
    const sig = JSON.parse(readFileSync(join(staged, 'signature.sig'), 'utf8'));
    const { sha256File: sha } = { sha256File };
    expect(sig.files['dist/index.esm.js'].sha256).toBe(sha(join(staged, 'dist/index.esm.js')));
    rmSync(src, { recursive: true, force: true });
    rmSync(staged, { recursive: true, force: true });
  });
});

/**
 * Track G1 anti-drift guard. The host entry table (`apps/desktop/src/main.tsx`) and the
 * pack-time allow-list (`HOST_SHARED_MODULES`) are two halves of one
 * mechanism: pack-ep rewrites every bare import to
 * `globalThis.__DATAZEN_HOST__[spec]`, so a specifier present in one list but
 * not the other either crashes the blob-loaded bundle (whitelist-only) or
 * becomes dead weight the host never hands out (table-only). They are edited
 * as one change, and these tests make that mandatory.
 */
describe('host shared module registry (G1 anti-drift guard)', () => {
  const hostTableKeys = parseHostGlobalTableKeys(
    readFileSync(join(ROOT, 'apps/desktop/src/main.tsx'), 'utf8'),
  );

  it('parses both quoted specifiers and bare identifier keys from the host table', () => {
    expect(hostTableKeys).toContain('react');
    expect(hostTableKeys).toContain('@codemirror/view');
  });

  it('registers exactly 11 shared modules on both sides', () => {
    expect(hostTableKeys).toHaveLength(11);
    expect(HOST_SHARED_MODULES).toHaveLength(11);
  });

  it('registers @codemirror/language and @codemirror/commands on both sides', () => {
    for (const spec of ['@codemirror/language', '@codemirror/commands']) {
      expect(hostTableKeys).toContain(spec);
      expect(HOST_SHARED_MODULES).toContain(spec);
    }
  });

  it('host table and HOST_SHARED_MODULES hold identical, duplicate-free key sets', () => {
    expect(new Set(hostTableKeys).size).toBe(hostTableKeys.length);
    expect([...hostTableKeys].sort()).toEqual([...HOST_SHARED_MODULES].sort());
  });

  it('every allow-listed specifier is actually published by the host table', () => {
    // The subset direction is the one that costs a user a crash: an entry that
    // exists only in the declaration rewrites to `__DATAZEN_HOST__['spec']`,
    // the host never hands that key out, and the EP dies on load with a signed
    // artifact. The reverse direction (a published key nothing imports) is dead
    // weight, not a crash, so the subset assertion is the load-bearing half.
    const published = new Set(hostTableKeys);
    const declaredOnly = HOST_SHARED_MODULES.filter((spec) => !published.has(spec));
    expect(
      declaredOnly,
      `HOST_SHARED_MODULES declares ${declaredOnly.join(', ')} but apps/desktop/src/main.tsx never ` +
        `publishes them — a Pro import of those would ship signed and crash on load`,
    ).toEqual([]);
  });

  it('rewrites a Pro import of the newly shared modules through the host table', () => {
    const { code, rewritten } = rewriteEpImportsToHostGlobals(
      [
        'import { foldService } from "@codemirror/language";',
        'import { defaultKeymap } from "@codemirror/commands";',
      ].join('\n'),
    );
    expect(rewritten.sort()).toEqual(['@codemirror/commands', '@codemirror/language']);
    expect(code).toContain(`globalThis.${HOST_GLOBAL_NAME}["@codemirror/language"]`);
    expect(code).toContain(`globalThis.${HOST_GLOBAL_NAME}["@codemirror/commands"]`);
  });

  it('still throws for a specifier missing from the narrow list', () => {
    const drifted = HOST_SHARED_MODULES.filter((spec) => spec !== '@codemirror/language');
    expect(() =>
      rewriteEpImportsToHostGlobals('import { foldService } from "@codemirror/language";', {
        modules: drifted,
      }),
    ).toThrow(/unmapped bare import from "@codemirror\/language"/);
  });
});

/**
 * [tester] Probe-integrity guard for the G1 red line — the Host's half.
 *
 * Two layers are *supposed* to disagree: the extension's build externalizes
 * every `@codemirror/*` (wide regex) while the Host's `HOST_SHARED_MODULES` stays
 * a narrow literal list. That gap is the probe — it is what makes a newly
 * imported shared package fail the build instead of being silently bundled as a
 * second copy of a host singleton (cross-realm identity split, no error at load).
 *
 * The registry-parity cases above read only `apps/desktop/src/main.tsx` and
 * `scripts/pack-ep.mjs`, so they stay green if someone "tidies up" the two sides
 * into one rule. The case below closes that hole from the Host side **without
 * reading the extension's checkout**: it derives the admitted set from the
 * Host's own `package.json` — every `@codemirror/*` the Host depends on that the
 * Host table does not publish — and asserts the rewriter hard-fails each one.
 *
 * The assertion about the extension's own `vite.config.ts` (that it still carries
 * the wide `/^@codemirror\//` rule, and that the range is still strictly wider
 * than the allow-list) reads the extension's build config, so it now lives in
 * the extension's own repository, where that file is always present.
 */
describe('externalize / allow-list asymmetry probe (G1 red line)', () => {
  it('keeps HOST_SHARED_MODULES a literal list, free of regexes', () => {
    expect(HOST_SHARED_MODULES.every((entry: unknown) => typeof entry === 'string')).toBe(true);
    expect(HOST_SHARED_MODULES).not.toContain('/^@codemirror\\//');
  });

  it('rejects every @codemirror the Host depends on but does not publish', () => {
    // Derived from Host data alone — the extension's checkout is not consulted,
    // so this runs (and can fail) in any Host CI job.
    const hostPkg: unknown = JSON.parse(readFileSync(join(ROOT, 'package.json'), 'utf8'));
    const deps = hostPkg as {
      dependencies?: Record<string, string>;
      devDependencies?: Record<string, string>;
    };
    const hostCodeMirror = Object.keys({ ...deps.dependencies, ...deps.devDependencies })
      .filter((name) => name.startsWith('@codemirror/'))
      .sort();
    // The Host must depend on some CodeMirror packages at all, or the set below
    // is empty and every assertion in it is vacuously true.
    expect(hostCodeMirror.length).toBeGreaterThan(0);

    const narrow = new Set(HOST_SHARED_MODULES);
    const published = new Set(readHostGlobalTableKeys());
    const admitted = hostCodeMirror.filter((spec) => !narrow.has(spec));
    expect(
      admitted.length,
      'the Host must depend on CodeMirror outside the allow-list',
    ).toBeGreaterThan(0);

    for (const spec of admitted) {
      // Unpublished AND unlisted: a bare import of it can only become a key the
      // Host never hands out, so the rewriter has to refuse it.
      expect(
        published.has(spec),
        `${spec} is not in the allow-list, so it cannot be published`,
      ).toBe(false);
      expect(() => rewriteEpImportsToHostGlobals(`import { probe } from "${spec}";`)).toThrow(
        new RegExp(`unmapped bare import from "${spec.replace('/', '\\/')}"`),
      );
    }
  });
});

/**
 * BUG-002 — artifact-level host key invariant.
 *
 * The narrow allow-list gate (`rewriteEpImportsToHostGlobals`) only ever sees
 * the input *of the pack-time rewrite*. The Pro build's own `renderChunk` runs
 * FIRST and already rewrites every `/^@codemirror\//` import — named, default
 * and namespace alike — into `globalThis.__DATAZEN_HOST__['…']` without ever
 * consulting the allow-list. By the time the host gate runs, there is no bare
 * import left to reject: the log line `rewrote bare imports:` is permanently
 * empty. An unmapped key therefore shipped signed and blew up at load time
 * (`const { foldGutter } = undefined`).
 *
 * These cases exercise the artifact, not the rewrite input, because that is
 * the only layer the bypass cannot reach. Side-effect imports already hard-failed
 * (the host gate's regex does match them), which is exactly why the defect hid
 * for so long: the control group works, the named-import group does not.
 */
describe('[bug-002] artifact host-key invariant', () => {
  /**
   * A bundle shaped like the Pro `renderChunk` output: bare specifiers are
   * already `__DATAZEN_HOST__` member accesses by the time pack-ep sees them.
   * `quote` mirrors the Pro plugin (single) vs the pack-ep rewriter (double).
   */
  function writeHostGlobalBundle(root: string, specs: string[], quote: "'" | '"' = "'") {
    const q = quote;
    const lines = specs.map(
      (spec, i) => `const { fn${i} } = globalThis.${HOST_GLOBAL_NAME}[${q}${spec}${q}];`,
    );
    writeFixtureExtension(root);
    writeFileSync(
      join(root, 'dist/index.esm.js'),
      `${lines.join('\n')}\nexport function activate() { return [${specs
        .map((_, i) => `fn${i}`)
        .join(', ')}]; }\n`,
    );
  }

  const UNMAPPED = '@codemirror/search';

  it('collects host keys from both quote styles the two rewriters emit', () => {
    const code = [
      `const a = globalThis.${HOST_GLOBAL_NAME}["@codemirror/state"];`,
      `const b = globalThis.${HOST_GLOBAL_NAME}['@codemirror/view'];`,
      `const c = globalThis.${HOST_GLOBAL_NAME}['@codemirror/state'];`,
    ].join('\n');
    expect(collectHostGlobalKeys(code)).toEqual(['@codemirror/state', '@codemirror/view']);
  });

  it('collects the keys the pack-ep rewriter itself produces', () => {
    const { code } = rewriteEpImportsToHostGlobals(
      'import { a } from "@codemirror/language";\nimport * as ns from "react";\n',
    );
    expect(collectHostGlobalKeys(code)).toEqual(['@codemirror/language', 'react']);
  });

  it('assertHostGlobalKeysAllowed accepts a key set inside the allow-list', () => {
    const code = `const a = globalThis.${HOST_GLOBAL_NAME}["react"];`;
    expect(assertHostGlobalKeysAllowed(code)).toEqual(['react']);
  });

  it('assertHostGlobalKeysAllowed rejects an unmapped key in either quote style', () => {
    expect(() =>
      assertHostGlobalKeysAllowed(`const a = globalThis.${HOST_GLOBAL_NAME}['${UNMAPPED}'];`),
    ).toThrow(/unmapped host table key "@codemirror\/search"/);
    expect(() =>
      assertHostGlobalKeysAllowed(`const a = globalThis.${HOST_GLOBAL_NAME}["${UNMAPPED}"];`),
    ).toThrow(/unmapped host table key "@codemirror\/search"/);
  });

  it('assertHostGlobalKeysAllowed fails closed on a non-literal key', () => {
    expect(() =>
      assertHostGlobalKeysAllowed(`const a = globalThis.${HOST_GLOBAL_NAME}[someVar];`),
    ).toThrow(/non-literal/);
    // Reading the whole table claims no specific key but cannot be verified
    // either — same discipline, so a future refactor cannot widen the hole.
    expect(() => assertHostGlobalKeysAllowed(`const t = globalThis.${HOST_GLOBAL_NAME};`)).toThrow(
      /non-literal/,
    );
  });

  it('assertHostGlobalKeysAllowed honours a renamed host global', () => {
    const code = `const a = globalThis.__DZ_OTHER__['@codemirror/state'];`;
    expect(() => assertHostGlobalKeysAllowed(code, { globalName: '__DZ_OTHER__' })).not.toThrow();
    // A different global's keys must never be counted as ours — and code that
    // never touches our table claims no keys, so it passes with an empty set.
    expect(assertHostGlobalKeysAllowed(code)).toEqual([]);
  });

  it('collects dot-form host keys, not just bracket form', () => {
    // The shipped Pro bundle resolves React through `__DATAZEN_HOST__.react`.
    // A bracket-only scanner reports zero keys for it and lets any future
    // `__DATAZEN_HOST__.search` through signed, so the dot form is a key claim
    // like any other and must be checked.
    expect(
      collectHostGlobalKeys(`const R = globalThis.${HOST_GLOBAL_NAME}.react.default;`),
    ).toEqual(['react']);
    expect(
      assertHostGlobalKeysAllowed(`const R = globalThis.${HOST_GLOBAL_NAME}.react.default;`),
    ).toEqual(['react']);
    expect(() =>
      assertHostGlobalKeysAllowed(`const S = globalThis.${HOST_GLOBAL_NAME}.search;`),
    ).toThrow(/unmapped host table key "search"/);
    // A dot chain resolves only its first segment; `.default` is a member of
    // the React namespace, not a host table key.
    expect(
      collectHostGlobalKeys(`const D = globalThis.${HOST_GLOBAL_NAME}.react.default;`),
    ).not.toContain('default');
  });

  it('rejects an allow-listed key the host entry table never publishes', () => {
    // The second bypass class: the narrow list is a *declaration of intent*, the
    // `apps/desktop/src/main.tsx` table is what exists at runtime. An entry present only in
    // the declaration is an `undefined` deref at load, exactly like an unmapped
    // one — so it must be a hard failure too, not a silent pass.
    const code = `const a = globalThis.${HOST_GLOBAL_NAME}["react-dom/server"];`;
    expect(() =>
      assertHostGlobalKeysAllowed(code, {
        modules: [...HOST_SHARED_MODULES, 'react-dom/server'],
      }),
    ).toThrow(/is in HOST_SHARED_MODULES but the .* table in src\/main\.tsx never publishes it/);
  });

  it('matches keys exactly, never by prefix', () => {
    // A prefix rule would admit `react-anything` and quietly make the
    // enumerated allow-list decorative, while the real `react/jsx-runtime`
    // sub-path keeps working and looks like the rule is fine.
    const code = `const a = globalThis.${HOST_GLOBAL_NAME}["react-faux"];`;
    expect(() => assertHostGlobalKeysAllowed(code)).toThrow(/unmapped host table key "react-faux"/);
    // The genuine sub-path is allow-listed as its own entry, and passes.
    expect(
      assertHostGlobalKeysAllowed(`const a = globalThis.${HOST_GLOBAL_NAME}["react/jsx-runtime"];`),
    ).toEqual(['react/jsx-runtime']);
  });

  it('reads the real host entry table and finds 11 published keys', () => {
    const keys = readHostGlobalTableKeys();
    expect(keys).toHaveLength(11);
    expect(keys).toContain('react');
    expect(keys).toContain('@codemirror/commands');
  });

  it('fails closed when the host entry table cannot be read', () => {
    expect(() => readHostGlobalTableKeys(join(ROOT, 'scripts/pack-ep.mjs'))).toThrow(
      /artifact key invariant cannot be verified/,
    );
  });

  // --- the mutation tests: the real gate, exercised through the real entry points ---

  it('stagePackageTree refuses to sign a bundle holding an unmapped host key', () => {
    const src = join(tmpdir(), `bug002-stage-src-${Date.now()}`);
    const staged = join(tmpdir(), `bug002-stage-out-${Date.now()}`);
    writeHostGlobalBundle(src, [UNMAPPED]);

    expect(() => stagePackageTree(src, staged, { log: () => {} })).toThrow(
      /unmapped host table key "@codemirror\/search"/,
    );
    // No signature may be issued for a bundle the host table cannot satisfy —
    // a signed artifact is what makes the crash unauditable downstream.
    expect(existsSync(join(staged, 'signature.sig'))).toBe(false);
    rmSync(src, { recursive: true, force: true });
    rmSync(staged, { recursive: true, force: true });
  });

  it.each([["'"] as const, ['"'] as const])(
    'stagePackageTree catches an out-of-table key written with %s quotes',
    (quote) => {
      // A one-style scanner is a vacuous gate: it reports zero keys on an
      // artifact written in the other style and passes. Both rewriters are in
      // play (Pro `renderChunk` emits single quotes, pack-ep emits double), so
      // both styles must independently trip the gate.
      const src = join(tmpdir(), `bug002-quote-src-${Date.now()}`);
      const staged = join(tmpdir(), `bug002-quote-out-${Date.now()}`);
      writeHostGlobalBundle(src, [UNMAPPED], quote);
      expect(() => stagePackageTree(src, staged, { log: () => {} })).toThrow(
        /unmapped host table key "@codemirror\/search"/,
      );
      expect(existsSync(join(staged, 'signature.sig'))).toBe(false);
      rmSync(src, { recursive: true, force: true });
      rmSync(staged, { recursive: true, force: true });
    },
  );

  it('stagePackageTree signs the very same shape once the key is allow-listed', () => {
    const src = join(tmpdir(), `bug002-stage-ok-src-${Date.now()}`);
    const staged = join(tmpdir(), `bug002-stage-ok-out-${Date.now()}`);
    writeHostGlobalBundle(src, ['@codemirror/language']);

    stagePackageTree(src, staged, { log: () => {} });
    expect(existsSync(join(staged, 'signature.sig'))).toBe(true);
    const bundle = readFileSync(join(staged, 'dist/index.esm.js'), 'utf8');
    const shipped = collectHostGlobalKeys(bundle);
    expect(shipped).toEqual(['@codemirror/language']);
    for (const key of shipped) {
      expect(HOST_SHARED_MODULES).toContain(key);
    }
    rmSync(src, { recursive: true, force: true });
    rmSync(staged, { recursive: true, force: true });
  });

  it('packEp produces neither .dzx nor signature for an unmapped host key', () => {
    const src = join(tmpdir(), `bug002-pack-src-${Date.now()}`);
    const outDir = join(tmpdir(), `bug002-pack-out-${Date.now()}`);
    mkdirSync(outDir, { recursive: true });
    writeHostGlobalBundle(src, ['react', UNMAPPED]);

    expect(() =>
      packEp({
        extension: 'fixture-ep',
        extensionDir: src,
        mode: 'dzx',
        outDir,
        skipBuild: true,
        log: () => {},
      }),
    ).toThrow(/unmapped host table key "@codemirror\/search"/);
    expect(existsSync(join(outDir, dzxFileName('fixture-ep', '9.9.9')))).toBe(false);
    expect(existsSync(join(outDir, '.pack-ep-staging-fixture-ep/signature.sig'))).toBe(false);
    rmSync(src, { recursive: true, force: true });
    rmSync(outDir, { recursive: true, force: true });
  });

  it('stagePackageTree also refuses a tree whose bytes arrive pre-rewritten', () => {
    // The CI/prebuilt handoff: `rewriteImports: false` means the bytes were
    // already turned into host accesses by the extension's own build, so the
    // input gate has nothing left to look at. The artifact gate is the only
    // thing standing between that path and a signed, unloadable bundle.
    const src = join(tmpdir(), `bug002-prebuilt-src-${Date.now()}`);
    const staged = join(tmpdir(), `bug002-prebuilt-staged-${Date.now()}`);
    writeFixtureExtension(src);
    writeFileSync(
      join(src, 'dist/index.esm.js'),
      `const { foldGutter } = globalThis.${HOST_GLOBAL_NAME}['${UNMAPPED}'];\nexport { foldGutter };\n`,
    );
    expect(() => stagePackageTree(src, staged, { log: () => {}, rewriteImports: false })).toThrow(
      /unmapped host table key "@codemirror\/search"/,
    );
    expect(existsSync(join(staged, 'signature.sig'))).toBe(false);
    rmSync(src, { recursive: true, force: true });
    rmSync(staged, { recursive: true, force: true });
  });

  it('createDzxArchive refuses to ship an already-staged tree with an unmapped key', () => {
    // Defence in depth: `stagePackageTree` is the first gate, but a prebuilt
    // tree handed straight to the archiver must not slip past it. Build the
    // tree by hand so it genuinely never passed through the first gate.
    const src = join(tmpdir(), `bug002-dzx-src-${Date.now()}`);
    const staged = join(tmpdir(), `bug002-dzx-staged-${Date.now()}`);
    const outDir = join(tmpdir(), `bug002-dzx-out-${Date.now()}`);
    mkdirSync(outDir, { recursive: true });
    writeFixtureExtension(src);
    writeFileSync(
      join(src, 'dist/index.esm.js'),
      `const { foldGutter } = globalThis.${HOST_GLOBAL_NAME}['${UNMAPPED}'];\nexport { foldGutter };\n`,
    );
    cpSync(join(src, 'manifest.json'), join(staged, 'manifest.json'));
    mkdirSync(join(staged, 'dist'), { recursive: true });
    cpSync(join(src, 'dist/index.esm.js'), join(staged, 'dist/index.esm.js'));
    signEpPackage({ packageDir: staged });
    expect(existsSync(join(staged, 'signature.sig'))).toBe(true);

    const outFile = join(outDir, dzxFileName('fixture-ep', '9.9.9'));
    expect(() => createDzxArchive(staged, outFile)).toThrow(
      /unmapped host table key "@codemirror\/search"/,
    );
    expect(existsSync(outFile)).toBe(false);
    rmSync(src, { recursive: true, force: true });
    rmSync(staged, { recursive: true, force: true });
    rmSync(outDir, { recursive: true, force: true });
  });

  it('keeps the side-effect-import control green (the gate itself is not broken)', () => {
    // The group that already worked must keep working — a fix that only turns
    // everything red is not a fix.
    expect(() => rewriteEpImportsToHostGlobals(`import '${UNMAPPED}';`)).toThrow(
      /unmapped bare side-effect import "@codemirror\/search"/,
    );
    const src = join(tmpdir(), `bug002-sideeffect-src-${Date.now()}`);
    const staged = join(tmpdir(), `bug002-sideeffect-out-${Date.now()}`);
    writeFixtureExtension(src);
    writeFileSync(
      join(src, 'dist/index.esm.js'),
      `import '${UNMAPPED}';\nexport function activate() {}\n`,
    );
    expect(() => stagePackageTree(src, staged, { log: () => {} })).toThrow(
      /unmapped bare side-effect import "@codemirror\/search"/,
    );
    expect(existsSync(join(staged, 'signature.sig'))).toBe(false);
    rmSync(src, { recursive: true, force: true });
    rmSync(staged, { recursive: true, force: true });
  });
});
