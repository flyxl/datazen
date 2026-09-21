/** @vitest-environment node */
import { mkdtempSync, mkdirSync, readFileSync, existsSync, writeFileSync, rmSync } from 'fs';
import { execSync } from 'child_process';
import { tmpdir } from 'os';
import { isAbsolute, join } from 'path';
import { describe, expect, it } from 'vitest';
import {
  parseArgs,
  readProLock,
  pinProCheckout,
  writeCommunityCodegen,
  writeProCodegen,
  resolvePro,
  clearBuiltinEpStaging,
  stageProExtension,
  ensureProCheckout,
  GENERATED_PRO_TS,
} from '../resolve-pro.mjs';
import { DEFAULT_BUILTIN_EP_ROOT } from '../pack-ep.mjs';

function writeFixtureExtension(root: string) {
  mkdirSync(join(root, 'dist'), { recursive: true });
  writeFileSync(
    join(root, 'package.json'),
    `${JSON.stringify({ name: 'fixture-ep', version: '1.0.0' })}\n`,
  );
  writeFileSync(
    join(root, 'manifest.json'),
    `${JSON.stringify(
      {
        id: 'sql-editor-pro',
        name: 'SQL Editor Pro',
        version: '1.0.0-test',
        main: 'dist/index.esm.js',
      },
      null,
      2,
    )}\n`,
  );
  writeFileSync(join(root, 'dist/index.esm.js'), 'export function activate() {}\n');
}

/**
 * Throwaway sandbox for every landing spot these flows write to.
 *
 * The production landing spots (`src-tauri/resources/builtin-ep/`, the pack-ep
 * work dir under `artifacts/` and the gitignored `src/extensions/generated-pro.ts`)
 * all live inside the real repo and are gitignored, so a test that writes there
 * is invisible to `git status` while still deleting a developer's local Pro
 * staging tree or leaving a Pro-edition codegen behind for later tsc/vite steps.
 * Redirecting them keeps the suite read-only with respect to the repo.
 */
function makeSandbox() {
  const root = mkdtempSync(join(tmpdir(), 'resolve-pro-sandbox-'));
  return {
    root,
    stageDir: join(root, 'builtin-ep', 'sql-editor-pro'),
    codegenPath: join(root, 'generated-pro.ts'),
    outDir: join(root, 'artifacts'),
    rm: () => rmSync(root, { recursive: true, force: true }),
  };
}

describe('resolve-pro parseArgs', () => {
  it('defaults to community edition', () => {
    const res = parseArgs([]);
    expect(res.edition).toBe('community');
    expect(res.restore).toBe(false);
  });

  it('detects --pro and --edition=pro', () => {
    expect(parseArgs(['--pro']).edition).toBe('pro');
    expect(parseArgs(['--edition=pro']).edition).toBe('pro');
  });

  it('detects --restore', () => {
    const res = parseArgs(['--restore']);
    expect(res.restore).toBe(true);
    expect(res.edition).toBe('community');
  });

  it('parses custom path and git parameters', () => {
    const res = parseArgs([
      '--edition=pro',
      '--pro-path=/custom/path',
      '--pro-git=git@github.com:custom/repo.git',
      '--codegen-only',
    ]);
    expect(res.edition).toBe('pro');
    expect(res.proPath).toBe('/custom/path');
    expect(res.proGit).toBe('git@github.com:custom/repo.git');
    expect(res.codegenOnly).toBe(true);
  });

  it('parses --pro-ref and defaults it to null', () => {
    expect(parseArgs([]).proRef).toBeNull();
    expect(parseArgs(['--pro-ref=abc1234']).proRef).toBe('abc1234');
  });

  it('reads the pinned Pro ref from the lock file', () => {
    const lock = readProLock();
    expect(lock.ref).toMatch(/^[0-9a-f]{40}$/);
    expect(lock.git).toContain('datazen-extension-sql-editor-pro');
  });

  it('treats a missing lock file as unpinned rather than throwing', () => {
    expect(readProLock('/nonexistent/pro-extension.lock.json')).toEqual({
      git: null,
      ref: null,
    });
  });
});

describe('resolve-pro checkout pinning', () => {
  /** Build a throwaway repo with two commits and return both shas. */
  function makeRepo(): { dir: string; first: string; second: string } {
    const dir = mkdtempSync(join(tmpdir(), 'pro-pin-'));
    const run = (cmd: string) => execSync(cmd, { cwd: dir, stdio: 'pipe' });
    run('git init -q');
    run('git config user.email t@t.t');
    run('git config user.name t');
    writeFileSync(join(dir, 'a.txt'), 'one\n');
    run('git add . && git commit -qm one');
    const first = execSync('git rev-parse HEAD', { cwd: dir, encoding: 'utf-8' }).trim();
    writeFileSync(join(dir, 'a.txt'), 'two\n');
    run('git add . && git commit -qm two');
    const second = execSync('git rev-parse HEAD', { cwd: dir, encoding: 'utf-8' }).trim();
    return { dir, first, second };
  }

  it('checks out the pinned commit and reports the resulting sha', () => {
    const { dir, first, second } = makeRepo();
    try {
      expect(second).not.toBe(first);
      const head = pinProCheckout(dir, first, { log: () => {} });
      expect(head).toBe(first);
      expect(execSync('git rev-parse HEAD', { cwd: dir, encoding: 'utf-8' }).trim()).toBe(first);
    } finally {
      rmSync(dir, { recursive: true, force: true });
    }
  });

  it('is a no-op when no ref is pinned', () => {
    const { dir, second } = makeRepo();
    try {
      expect(pinProCheckout(dir, null, { log: () => {} })).toBeNull();
      expect(execSync('git rev-parse HEAD', { cwd: dir, encoding: 'utf-8' }).trim()).toBe(second);
    } finally {
      rmSync(dir, { recursive: true, force: true });
    }
  });

  it('fails loudly when the pinned ref does not exist', () => {
    const { dir } = makeRepo();
    try {
      expect(() =>
        pinProCheckout(dir, '0000000000000000000000000000000000000000', { log: () => {} }),
      ).toThrow();
    } finally {
      rmSync(dir, { recursive: true, force: true });
    }
  });
});

describe('resolve-pro codegen output', () => {
  it('writes community codegen with fallback', () => {
    const dir = mkdtempSync(join(tmpdir(), 'resolve-pro-test-'));
    const file = join(dir, 'generated-pro.ts');
    writeCommunityCodegen(file);
    expect(existsSync(file)).toBe(true);
    const content = readFileSync(file, 'utf-8');
    expect(content).toContain("export const DATAZEN_EDITION = 'community'");
    expect(content).toContain('export function initProExtensions(): void');
    expect(content).not.toContain('@datazen/extension-sql-editor-pro');
  });

  it('writes pro codegen with builtin-ep runtime loader', () => {
    const dir = mkdtempSync(join(tmpdir(), 'resolve-pro-test-'));
    const file = join(dir, 'generated-pro.ts');
    writeProCodegen(file);
    expect(existsSync(file)).toBe(true);
    const content = readFileSync(file, 'utf-8');
    expect(content).toContain("export const DATAZEN_EDITION = 'pro'");
    // Track B: dynamic hot-plug through hostExtensionLoader + signature gate,
    // resolved from the staged `builtin-ep` resource — no static alias import.
    expect(content).toContain('hostExtensionLoader');
    expect(content).toContain('loadFromUrl(');
    expect(content).toContain('verifyExtensionPackage');
    expect(content).toContain("'builtin-ep'");
    expect(content).not.toContain('@datazen/extension-sql-editor-pro');
  });

  it('preserves existing generated-pro.ts when codegenOnly is run without explicit edition', () => {
    const sb = makeSandbox();
    try {
      writeFileSync(sb.codegenPath, 'preserved\n');
      const res = resolvePro({
        codegenOnly: true,
        stageDir: sb.stageDir,
        codegenPath: sb.codegenPath,
      });
      // Should preserve existing file when no explicit edition passed
      expect(res).toBeDefined();
      expect(readFileSync(sb.codegenPath, 'utf-8')).toBe('preserved\n');
    } finally {
      sb.rm();
    }
  });
});

describe('[tester] resolve-pro staging and edition flows', () => {
  it('test_tester_default_staging_and_codegen_paths_stay_repo_relative', () => {
    // Value-only assertion: the sandbox redirects below must keep the
    // production defaults, and nothing may be written to the real paths.
    expect(isAbsolute(GENERATED_PRO_TS)).toBe(true);
    expect(GENERATED_PRO_TS.endsWith(join('src', 'extensions', 'generated-pro.ts'))).toBe(true);
    expect(isAbsolute(DEFAULT_BUILTIN_EP_ROOT)).toBe(true);
    expect(DEFAULT_BUILTIN_EP_ROOT.endsWith(join('src-tauri', 'resources', 'builtin-ep'))).toBe(
      true,
    );
  });

  it('test_tester_resolvePro_community_writes_codegen_and_clears_staging', () => {
    const sb = makeSandbox();
    try {
      const staging = sb.stageDir;
      mkdirSync(staging, { recursive: true });
      writeFileSync(join(staging, 'manifest.json'), '{}');

      const res = resolvePro({
        edition: 'community',
        stageDir: sb.stageDir,
        codegenPath: sb.codegenPath,
      });
      expect(res).toEqual({ edition: 'community', active: false });
      expect(existsSync(sb.codegenPath)).toBe(true);
      expect(readFileSync(sb.codegenPath, 'utf-8')).toContain("DATAZEN_EDITION = 'community'");
      expect(existsSync(staging)).toBe(false);
    } finally {
      sb.rm();
    }
  });

  it('test_tester_resolvePro_restore_alias_for_community', () => {
    const sb = makeSandbox();
    try {
      const res = resolvePro({
        restore: true,
        stageDir: sb.stageDir,
        codegenPath: sb.codegenPath,
      });
      expect(res.edition).toBe('community');
      expect(res.active).toBe(false);
      expect(readFileSync(sb.codegenPath, 'utf-8')).toContain("DATAZEN_EDITION = 'community'");
    } finally {
      sb.rm();
    }
  });

  it('test_tester_ensureProCheckout_prefers_explicit_pro_path', () => {
    const extDir = mkdtempSync(join(tmpdir(), 'resolve-pro-path-'));
    try {
      writeFileSync(join(extDir, 'package.json'), '{}');
      const path = ensureProCheckout({ proPath: extDir, codegenOnly: true });
      expect(path).toBe(extDir);
    } finally {
      rmSync(extDir, { recursive: true, force: true });
    }
  });

  it('test_tester_ensureProCheckout_returns_existing_checkout_when_present', () => {
    const sb = makeSandbox();
    try {
      const existing = join(sb.root, 'pro-extensions', 'sql-editor-pro');
      mkdirSync(existing, { recursive: true });
      writeFileSync(join(existing, 'package.json'), '{}');
      const path = ensureProCheckout({
        codegenOnly: true,
        proDest: existing,
        tmpFallbackDir: join(sb.root, 'no-fallback'),
      });
      expect(path).toBe(existing);
    } finally {
      sb.rm();
    }
  });

  it('test_tester_ensureProCheckout_returns_null_in_codegen_only_without_checkout', () => {
    const sb = makeSandbox();
    try {
      const path = ensureProCheckout({
        codegenOnly: true,
        proDest: join(sb.root, 'pro-extensions', 'sql-editor-pro'),
        tmpFallbackDir: join(sb.root, 'no-fallback'),
      });
      expect(path).toBeNull();
    } finally {
      sb.rm();
    }
  });

  it('test_tester_clearBuiltinEpStaging_removes_extension_tree', () => {
    const sb = makeSandbox();
    try {
      mkdirSync(sb.stageDir, { recursive: true });
      writeFileSync(join(sb.stageDir, 'marker.txt'), 'x');
      clearBuiltinEpStaging('sql-editor-pro', { stageDir: sb.stageDir });
      expect(existsSync(sb.stageDir)).toBe(false);
    } finally {
      sb.rm();
    }
  });

  it('test_tester_stageProExtension_stages_signed_tree', () => {
    const sb = makeSandbox();
    const extDir = join(sb.root, 'fixture-ext');
    try {
      writeFixtureExtension(extDir);
      const result = stageProExtension({
        extensionDir: extDir,
        skipBuild: true,
        mode: 'stage',
        log: () => {},
        stageDir: sb.stageDir,
        outDir: sb.outDir,
      });
      expect(result.staged).toBe(true);
      expect(result.stageDir).toBe(sb.stageDir);
      expect(existsSync(join(sb.stageDir, 'manifest.json'))).toBe(true);
      expect(existsSync(join(sb.stageDir, 'dist/index.esm.js'))).toBe(true);
      expect(existsSync(join(sb.stageDir, 'signature.sig'))).toBe(true);
    } finally {
      sb.rm();
    }
  });

  it('test_tester_resolvePro_pro_codegenOnly_without_staging', () => {
    const sb = makeSandbox();
    const extDir = join(sb.root, 'fixture-ext');
    try {
      writeFixtureExtension(extDir);
      const res = resolvePro({
        edition: 'pro',
        codegenOnly: true,
        proPath: extDir,
        stageDir: sb.stageDir,
        codegenPath: sb.codegenPath,
      });
      expect(res).toEqual({ edition: 'pro', active: true, path: extDir });
      expect(readFileSync(sb.codegenPath, 'utf-8')).toContain("DATAZEN_EDITION = 'pro'");
      expect(existsSync(sb.stageDir)).toBe(false);
    } finally {
      sb.rm();
    }
  });

  it('test_tester_resolvePro_uses_already_staged_tree_without_cloning', () => {
    // CI builds the extension once and hands the signed tree to every variant as
    // an artifact. resolve-pro must use that tree verbatim rather than cloning
    // the private repo again in each matrix job.
    const sb = makeSandbox();
    const prevGit = process.env.DATAZEN_PRO_GIT;
    delete process.env.DATAZEN_PRO_GIT;
    try {
      writeFixtureExtension(sb.stageDir);
      const bundle = join(sb.stageDir, 'dist/index.esm.js');
      const staged = readFileSync(bundle, 'utf-8');

      const res = resolvePro({
        edition: 'pro',
        stageDir: sb.stageDir,
        codegenPath: sb.codegenPath,
      });
      expect(res).toMatchObject({ edition: 'pro', active: true, prebuilt: true });
      // A clone would have overwritten the staged bundle.
      expect(readFileSync(bundle, 'utf-8')).toBe(staged);
    } finally {
      if (prevGit !== undefined) process.env.DATAZEN_PRO_GIT = prevGit;
      sb.rm();
    }
  });

  it('test_tester_resolvePro_pro_stages_builtin_ep_for_runtime_loading', () => {
    const extDir = join(process.cwd(), 'packages/pro-extensions/sql-editor-pro');
    if (!existsSync(join(extDir, 'package.json'))) {
      return;
    }
    const sb = makeSandbox();
    try {
      const res = resolvePro({
        edition: 'pro',
        proPath: extDir,
        stageDir: sb.stageDir,
        codegenPath: sb.codegenPath,
        outDir: sb.outDir,
      });
      expect(res).toMatchObject({ edition: 'pro', active: true, path: extDir });
      // Track B: the rewritten + signed bundle is staged as a Tauri resource and
      // the codegen loads it dynamically — no static alias reference.
      expect(existsSync(join(sb.stageDir, 'dist/index.esm.js'))).toBe(true);
      expect(existsSync(join(sb.stageDir, 'signature.sig'))).toBe(true);
      const stagedBundle = readFileSync(join(sb.stageDir, 'dist/index.esm.js'), 'utf-8');
      expect(stagedBundle).toContain('__DATAZEN_HOST__');
      expect(readFileSync(sb.codegenPath, 'utf-8')).not.toContain(
        '@datazen/extension-sql-editor-pro',
      );
    } finally {
      sb.rm();
    }
  }, 120_000);

  it('test_tester_resolvePro_unknown_edition_throws', () => {
    expect(() => resolvePro({ edition: 'enterprise' as 'pro' })).toThrow(/unknown edition/);
  });

  it('test_tester_parseArgs_respects_DATAZEN_EDITION_env', () => {
    const prev = process.env.DATAZEN_EDITION;
    process.env.DATAZEN_EDITION = 'pro';
    try {
      expect(parseArgs([]).edition).toBe('pro');
    } finally {
      if (prev === undefined) {
        delete process.env.DATAZEN_EDITION;
      } else {
        process.env.DATAZEN_EDITION = prev;
      }
    }
  });
});
