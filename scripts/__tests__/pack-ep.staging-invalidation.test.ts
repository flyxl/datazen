/** @vitest-environment node */
/**
 * A failed pack must not leave behind a signed tree that a later
 * `resolve-pro --edition=pro` can short-circuit onto.
 *
 * The short-circuit ("a signed tree is already staged under builtin-ep and
 * nobody named a source, so use it verbatim") is load-bearing for CI: the
 * release matrix builds the extension once in `prepare-pro-extension` and hands
 * the tree to all 11 variant jobs. It is also exactly the path that turns a
 * *failed* build into a silent success: `packEp` only rewrites
 * `builtin-ep/<ext>` at its very last step, so a build that dies earlier (vite
 * build, or the artifact-level host-key gate) leaves the previous tree — and its
 * previous signature — fully intact and reusable. `resolve-pro` then exits 0 and
 * the developer (or reviewer) reads a green run as "verified", when nothing was
 * verified at all.
 */
import { execFileSync } from 'child_process';
import { existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'fs';
import { tmpdir } from 'os';
import { join } from 'path';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { checkProStagingReady } from '../ci-tauri-build.mjs';
import { packEp, stagedTreeComplete, stagingMarkerPath } from '../pack-ep.mjs';
import { downloadPrebuiltEp, resolvePro, stagedTreeUsable } from '../resolve-pro.mjs';

const GOOD_BUNDLE = 'export function activate() { return 1; }\n';
/** `@codemirror/search` is in neither HOST_SHARED_MODULES nor the host table. */
const BYPASSING_BUNDLE =
  'const f = __DATAZEN_HOST__["@codemirror/search"];\nexport function activate() { return f; }\n';

function writeExtension(root: string, bundle: string) {
  mkdirSync(join(root, 'dist'), { recursive: true });
  writeFileSync(join(root, 'package.json'), `${JSON.stringify({ name: 'fixture-ep' })}\n`);
  writeFileSync(
    join(root, 'manifest.json'),
    `${JSON.stringify({ id: 'sql-editor-pro', name: 'SQL Editor Pro', version: '1.0.0', main: 'dist/index.esm.js' })}\n`,
  );
  writeFileSync(join(root, 'dist/index.esm.js'), bundle);
}

/** Build a `tar -czf` of `{ manifest.json, dist/index.esm.js }` for the prebuilt path. */
function writePrebuiltTarball(root: string, bundle: string): string {
  const src = join(root, 'prebuilt-src');
  mkdirSync(join(src, 'dist'), { recursive: true });
  writeFileSync(
    join(src, 'manifest.json'),
    `${JSON.stringify({ id: 'sql-editor-pro', name: 'SQL Editor Pro', version: '9.9.9-prebuilt', main: 'dist/index.esm.js' })}\n`,
  );
  writeFileSync(join(src, 'dist/index.esm.js'), bundle);
  const tarball = join(root, 'prebuilt.tar.gz');
  execFileSync('tar', ['-czf', tarball, '-C', src, '.']);
  return `file://${tarball}`;
}

describe('a failed pack must not leave a reusable signed tree', () => {
  let sb: { root: string; stageDir: string; codegenPath: string; outDir: string };

  beforeEach(() => {
    const root = mkdtempSync(join(tmpdir(), 'staging-invalidation-'));
    sb = {
      root,
      // Mirrors the production layout so ci-tauri-build's `root`-relative probe
      // resolves to the same tree the rest of these cases exercise.
      stageDir: join(root, 'apps/desktop/src-tauri', 'resources', 'builtin-ep', 'sql-editor-pro'),
      codegenPath: join(root, 'generated-pro.ts'),
      outDir: join(root, 'artifacts'),
    };
  });

  afterEach(() => {
    vi.restoreAllMocks();
    rmSync(sb.root, { recursive: true, force: true });
  });

  /** Stage a good, signed tree exactly the way a normal `resolve-pro --edition=pro` does. */
  function stageGoodTree(): string {
    const extDir = join(sb.root, 'good-ext');
    writeExtension(extDir, GOOD_BUNDLE);
    packEp({
      extension: 'sql-editor-pro',
      extensionDir: extDir,
      mode: 'stage',
      skipBuild: true,
      stageDir: sb.stageDir,
      outDir: sb.outDir,
      log: () => {},
    });
    return readFileSync(join(sb.stageDir, 'dist/index.esm.js'), 'utf-8');
  }

  /** A pack that dies at the artifact gate, leaving `stageDir` untouched. */
  function runFailingPack(): void {
    const badExtDir = join(sb.root, 'bad-ext');
    writeExtension(badExtDir, BYPASSING_BUNDLE);
    expect(() =>
      packEp({
        extension: 'sql-editor-pro',
        extensionDir: badExtDir,
        mode: 'stage',
        skipBuild: true,
        stageDir: sb.stageDir,
        outDir: sb.outDir,
        log: () => {},
      }),
    ).toThrow(/@codemirror\/search/);
  }

  it('a successful pack leaves the staged tree complete and reusable', () => {
    stageGoodTree();
    expect(existsSync(join(sb.stageDir, 'signature.sig'))).toBe(true);
    expect(stagedTreeComplete(sb.stageDir)).toBe(true);
    expect(stagedTreeUsable(sb.stageDir)).toBe(true);
  });

  it('a failed pack leaves the previous signed tree in place but marks it incomplete', () => {
    const good = stageGoodTree();
    const goodSignature = readFileSync(join(sb.stageDir, 'signature.sig'), 'utf-8');

    runFailingPack();

    // The hazard is real: nothing removed the old tree, and it is still a
    // complete, signed, self-consistent package.
    expect(existsSync(join(sb.stageDir, 'dist/index.esm.js'))).toBe(true);
    expect(readFileSync(join(sb.stageDir, 'dist/index.esm.js'), 'utf-8')).toBe(good);
    expect(readFileSync(join(sb.stageDir, 'signature.sig'), 'utf-8')).toBe(goodSignature);

    // ...but it is no longer eligible for reuse.
    expect(existsSync(stagingMarkerPath(sb.stageDir))).toBe(true);
    expect(stagedTreeComplete(sb.stageDir)).toBe(false);
    expect(stagedTreeUsable(sb.stageDir)).toBe(false);
  });

  it('resolvePro does not short-circuit onto the stale tree after a failed pack', () => {
    stageGoodTree();
    runFailingPack();

    const prebuiltUrl = writePrebuiltTarball(sb.root, GOOD_BUNDLE);
    const logs: string[] = [];
    vi.spyOn(console, 'log').mockImplementation((...args: unknown[]) => {
      logs.push(args.map(String).join(' '));
    });

    const res = resolvePro({
      edition: 'pro',
      stageDir: sb.stageDir,
      codegenPath: sb.codegenPath,
      prebuiltUrl,
    });

    // Before the fix this branch was taken: "already staged, skipping download",
    // prebuilt: true, stale bytes kept. That is the false green.
    expect(logs.join('\n')).not.toMatch(/already staged, skipping download/);
    expect(logs.join('\n')).toMatch(/incomplete/i);
    // The stale tree was replaced, not reused.
    expect(res).toMatchObject({ edition: 'pro', active: true, prebuilt: true });
    expect(readFileSync(join(sb.stageDir, 'manifest.json'), 'utf-8')).toContain('9.9.9-prebuilt');
    expect(stagedTreeComplete(sb.stageDir)).toBe(true);
    expect(stagedTreeUsable(sb.stageDir)).toBe(true);
  });

  it('a failed prebuilt download does not leave a half-extracted tree reusable', () => {
    stageGoodTree();
    expect(() =>
      downloadPrebuiltEp({
        prebuiltUrl: `file://${join(sb.root, 'does-not-exist.tar.gz')}`,
        stageDir: sb.stageDir,
      }),
    ).toThrow();
    expect(stagedTreeComplete(sb.stageDir)).toBe(false);
    expect(stagedTreeUsable(sb.stageDir)).toBe(false);
  });

  it('a downloaded tree never inherits a stale signature from the previous one', () => {
    stageGoodTree();
    expect(existsSync(join(sb.stageDir, 'signature.sig'))).toBe(true);

    // A prebuilt tarball that legitimately ships no signature must not end up
    // paired with the *previous* tree's signature: that signature covers bytes
    // that are no longer on disk.
    const src = join(sb.root, 'unsigned-prebuilt');
    mkdirSync(join(src, 'dist'), { recursive: true });
    writeFileSync(join(src, 'manifest.json'), '{"id":"sql-editor-pro","version":"0.0.1"}\n');
    writeFileSync(join(src, 'dist/index.esm.js'), GOOD_BUNDLE);
    const tarball = join(sb.root, 'unsigned.tar.gz');
    execFileSync('tar', ['-czf', tarball, '-C', src, '.']);

    downloadPrebuiltEp({ prebuiltUrl: `file://${tarball}`, stageDir: sb.stageDir });

    expect(existsSync(join(sb.stageDir, 'signature.sig'))).toBe(false);
    expect(readFileSync(join(sb.stageDir, 'manifest.json'), 'utf-8')).toContain('0.0.1');
  });

  it('CI semantics: a complete staged tree is still shared verbatim by every variant', () => {
    const good = stageGoodTree();
    const prevGit = process.env.DATAZEN_PRO_GIT;
    delete process.env.DATAZEN_PRO_GIT;
    try {
      // Exactly what the release matrix does: no source named, tree handed over
      // by the `pro-extension` artifact. Must short-circuit, not rebuild.
      for (let variant = 0; variant < 3; variant += 1) {
        const res = resolvePro({
          edition: 'pro',
          stageDir: sb.stageDir,
          codegenPath: sb.codegenPath,
        });
        expect(res).toMatchObject({ edition: 'pro', active: true, prebuilt: true });
        expect(readFileSync(join(sb.stageDir, 'dist/index.esm.js'), 'utf-8')).toBe(good);
        expect(stagedTreeComplete(sb.stageDir)).toBe(true);
      }
    } finally {
      if (prevGit !== undefined) process.env.DATAZEN_PRO_GIT = prevGit;
    }
  });

  it('a complete staged tree is still reused when a prebuilt url is offered', () => {
    const good = stageGoodTree();
    const prebuiltUrl = writePrebuiltTarball(sb.root, GOOD_BUNDLE);
    const logs: string[] = [];
    vi.spyOn(console, 'log').mockImplementation((...args: unknown[]) => {
      logs.push(args.map(String).join(' '));
    });

    const res = resolvePro({
      edition: 'pro',
      stageDir: sb.stageDir,
      codegenPath: sb.codegenPath,
      prebuiltUrl,
    });

    expect(logs.join('\n')).toMatch(/already staged, skipping download/);
    expect(res).toMatchObject({ edition: 'pro', active: true, prebuilt: true });
    expect(readFileSync(join(sb.stageDir, 'dist/index.esm.js'), 'utf-8')).toBe(good);
  });

  it('ci-tauri-build refuses to ship a tree a failed pack left behind', () => {
    stageGoodTree();
    expect(checkProStagingReady({ root: sb.root, log: () => {} })).toEqual([]);

    runFailingPack();

    const notices: string[] = [];
    const missing = checkProStagingReady({ root: sb.root, log: (m: string) => notices.push(m) });
    expect(missing).toContain('sql-editor-pro.incomplete');
    expect(notices.join('\n')).toMatch(/stale/);
  });

  it('dzx-only packs never touch the staged tree or its marker', () => {
    const good = stageGoodTree();
    const extDir = join(sb.root, 'good-ext');
    packEp({
      extension: 'sql-editor-pro',
      extensionDir: extDir,
      mode: 'dzx',
      skipBuild: true,
      stageDir: sb.stageDir,
      outDir: sb.outDir,
      log: () => {},
    });
    expect(stagedTreeComplete(sb.stageDir)).toBe(true);
    expect(readFileSync(join(sb.stageDir, 'dist/index.esm.js'), 'utf-8')).toBe(good);
  });
});
