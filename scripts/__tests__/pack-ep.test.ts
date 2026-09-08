/** @vitest-environment node */
import { cpSync, existsSync, mkdirSync, readFileSync, rmSync, writeFileSync } from 'fs';
import { tmpdir } from 'os';
import { join } from 'path';
import { unzipSync } from 'fflate';
import { describe, expect, it } from 'vitest';
import {
  assertPackageLayout,
  createDzxArchive,
  dzxFileName,
  listZipEntries,
  packEp,
  parsePackArgs,
  readManifestVersion,
  REQUIRED_PACKAGE_PATHS,
  stagePackageTree,
  syncLocales,
} from '../pack-ep.mjs';
import {
  buildSignaturePayload,
  parsePrivateKeyMaterial,
  parseSignArgs,
  sha256File,
  sha256Hex,
  signEpPackage,
} from '../sign-ep.mjs';
import { generateKeyPairSync } from 'crypto';

function writeFixtureExtension(root: string) {
  mkdirSync(join(root, 'dist'), { recursive: true });
  mkdirSync(join(root, 'src/locales'), { recursive: true });
  writeFileSync(
    join(root, 'manifest.json'),
    `${JSON.stringify(
      {
        id: '@datazen/extension-fixture',
        name: 'Fixture EP',
        version: '9.9.9',
        main: 'dist/index.esm.js',
        engines: { datazen: '>=0.1.2', extensionPointsVersion: '1.0.0' },
      },
      null,
      2,
    )}\n`,
  );
  writeFileSync(join(root, 'dist/index.esm.js'), 'export function activate() {}\n');
  writeFileSync(
    join(root, 'src/locales/en.ts'),
    "export const en = { 'fixture.key': 'Fixture' };\n",
  );
}

describe('sign-ep', () => {
  it('writes signature.sig with stable file hashes', () => {
    const dir = join(tmpdir(), `sign-ep-test-${Date.now()}`);
    writeFixtureExtension(dir);
    const { outPath, sigDoc } = signEpPackage({ packageDir: dir });
    expect(existsSync(outPath)).toBe(true);
    expect(sigDoc.files['manifest.json'].sha256).toBe(sha256File(join(dir, 'manifest.json')));
    expect(sigDoc.files['dist/index.esm.js'].sha256).toBe(
      sha256File(join(dir, 'dist/index.esm.js')),
    );
    expect(sigDoc.signature).toMatch(/^[A-Za-z0-9+/=]+$/);
  });
});

describe('pack-ep staging and .dzx archive', () => {
  it('stages signed package tree with required layout', () => {
    const src = join(tmpdir(), `pack-ep-src-${Date.now()}`);
    const staged = join(tmpdir(), `pack-ep-staged-${Date.now()}`);
    writeFixtureExtension(src);
    stagePackageTree(src, staged);
    assertPackageLayout(staged);
    for (const rel of REQUIRED_PACKAGE_PATHS) {
      expect(existsSync(join(staged, rel))).toBe(true);
    }
    expect(existsSync(join(staged, 'locales/en.ts'))).toBe(true);
    const sig = JSON.parse(readFileSync(join(staged, 'signature.sig'), 'utf8'));
    expect(sig.algorithm).toBe('Ed25519');
  });

  it('creates .dzx zip with manifest, bundle, signature, and locales', () => {
    const src = join(tmpdir(), `pack-ep-dzx-src-${Date.now()}`);
    const staged = join(tmpdir(), `pack-ep-dzx-staged-${Date.now()}`);
    const outDir = join(tmpdir(), `pack-ep-dzx-out-${Date.now()}`);
    mkdirSync(outDir, { recursive: true });
    writeFixtureExtension(src);
    stagePackageTree(src, staged);

    const dzxPath = join(outDir, dzxFileName('fixture-ep', '9.9.9'));
    createDzxArchive(staged, dzxPath);
    expect(existsSync(dzxPath)).toBe(true);

    const entries = unzipSync(readFileSync(dzxPath));
    const names = Object.keys(entries).sort();
    expect(names).toEqual(
      expect.arrayContaining([
        'manifest.json',
        'dist/index.esm.js',
        'signature.sig',
        'locales/en.ts',
      ]),
    );

    const manifest = JSON.parse(Buffer.from(entries['manifest.json']).toString('utf8'));
    expect(manifest.version).toBe('9.9.9');
    const sig = JSON.parse(Buffer.from(entries['signature.sig']).toString('utf8'));
    expect(sig.files['dist/index.esm.js']).toBeTruthy();
  });

  it('packEp --skip-build packages from an existing staged tree', () => {
    const src = join(tmpdir(), `pack-ep-skip-src-${Date.now()}`);
    const staged = join(tmpdir(), `pack-ep-skip-staged-${Date.now()}`);
    const outDir = join(tmpdir(), `pack-ep-skip-out-${Date.now()}`);
    writeFixtureExtension(src);
    stagePackageTree(src, staged);

    const result = packEp({
      extension: 'fixture-ep',
      extensionDir: staged,
      mode: 'dzx',
      outDir,
      skipBuild: true,
      log: () => {},
    });

    expect(result.dzxPath).toBe(join(outDir, 'fixture-ep-9.9.9.dzx'));
    expect(existsSync(result.dzxPath!)).toBe(true);
  });
});

describe('pack-ep helpers', () => {
  it('listZipEntries preserves relative paths', () => {
    const root = join(tmpdir(), `pack-ep-zip-${Date.now()}`);
    mkdirSync(join(root, 'dist'), { recursive: true });
    writeFileSync(join(root, 'manifest.json'), '{}');
    writeFileSync(join(root, 'dist/index.esm.js'), 'export {}');
    const entries = listZipEntries(root);
    expect(Object.keys(entries).sort()).toEqual(['dist/index.esm.js', 'manifest.json']);
  });

  it('buildSignaturePayload sorts file keys deterministically', () => {
    const payload = buildSignaturePayload({
      'dist/index.esm.js': 'bbb',
      'manifest.json': 'aaa',
    });
    expect(payload.indexOf('"dist/index.esm.js"')).toBeLessThan(payload.indexOf('"manifest.json"'));
  });
});

describe('pack-ep integration with sql-editor-pro (when present)', () => {
  it('builds, signs, and writes .dzx for sql-editor-pro', () => {
    const extDir = join(process.cwd(), 'packages/pro-extensions/sql-editor-pro');
    if (!existsSync(join(extDir, 'package.json'))) {
      return;
    }

    const outDir = join(tmpdir(), `pack-ep-pro-${Date.now()}`);
    mkdirSync(outDir, { recursive: true });

    const result = packEp({
      extension: 'sql-editor-pro',
      extensionDir: extDir,
      mode: 'dzx',
      outDir,
      log: () => {},
    });

    expect(result.dzxPath).toMatch(/sql-editor-pro-.*\.dzx$/);
    expect(existsSync(result.dzxPath!)).toBe(true);

    const entries = unzipSync(readFileSync(result.dzxPath!));
    expect(Object.keys(entries)).toEqual(
      expect.arrayContaining(['manifest.json', 'dist/index.esm.js', 'signature.sig']),
    );

    rmSync(outDir, { recursive: true, force: true });
  }, 120_000);
});

describe('[tester] pack-ep args, locales, and error paths', () => {
  it('test_tester_parsePackArgs_parses_mode_out_and_skip_build', () => {
    const parsed = parsePackArgs([
      '--extension=custom-ep',
      '--mode=stage',
      '--out=/tmp/out',
      '--stage-dir=/tmp/stage',
      '--skip-build',
    ]);
    expect(parsed.extension).toBe('custom-ep');
    expect(parsed.mode).toBe('stage');
    expect(parsed.outDir).toContain('/tmp/out');
    expect(parsed.stageDir).toContain('/tmp/stage');
    expect(parsed.skipBuild).toBe(true);
  });

  it('test_tester_readManifestVersion_requires_string_version', () => {
    const dir = join(tmpdir(), `pack-ep-manifest-${Date.now()}`);
    mkdirSync(dir, { recursive: true });
    writeFileSync(join(dir, 'manifest.json'), JSON.stringify({ id: 'x' }));
    expect(() => readManifestVersion(join(dir, 'manifest.json'))).toThrow(
      /missing string "version"/,
    );
  });

  it('test_tester_packEp_unknown_mode_throws', () => {
    const src = join(tmpdir(), `pack-ep-bad-mode-${Date.now()}`);
    writeFixtureExtension(src);
    expect(() =>
      packEp({
        extensionDir: src,
        mode: 'invalid' as 'dzx',
        skipBuild: true,
        log: () => {},
      }),
    ).toThrow(/unknown mode/);
  });

  it('test_tester_packEp_mode_both_writes_dzx_and_stages', () => {
    const src = join(tmpdir(), `pack-ep-both-src-${Date.now()}`);
    const stageDir = join(tmpdir(), `pack-ep-both-stage-${Date.now()}`);
    const outDir = join(tmpdir(), `pack-ep-both-out-${Date.now()}`);
    writeFixtureExtension(src);
    const result = packEp({
      extension: 'fixture-ep',
      extensionDir: src,
      mode: 'both',
      outDir,
      stageDir,
      skipBuild: true,
      log: () => {},
    });
    expect(result.staged).toBe(true);
    expect(result.dzxPath).toBe(join(outDir, 'fixture-ep-9.9.9.dzx'));
    expect(existsSync(result.dzxPath!)).toBe(true);
    expect(existsSync(join(stageDir, 'signature.sig'))).toBe(true);
    rmSync(outDir, { recursive: true, force: true });
    rmSync(stageDir, { recursive: true, force: true });
  });

  it('test_tester_syncLocales_copies_explicit_locales_directory', () => {
    const src = join(tmpdir(), `pack-ep-locales-src-${Date.now()}`);
    const target = join(tmpdir(), `pack-ep-locales-target-${Date.now()}`);
    mkdirSync(join(src, 'locales'), { recursive: true });
    writeFileSync(join(src, 'locales/en.json'), '{"k":"v"}');
    syncLocales(src, target, { log: () => {} });
    expect(existsSync(join(target, 'locales/en.json'))).toBe(true);
  });

  it('test_tester_assertPackageLayout_throws_when_incomplete', () => {
    const dir = join(tmpdir(), `pack-ep-incomplete-${Date.now()}`);
    mkdirSync(dir, { recursive: true });
    writeFileSync(join(dir, 'manifest.json'), '{}');
    expect(() => assertPackageLayout(dir)).toThrow(/incomplete package/);
  });
});

describe('[tester] sign-ep helpers and error paths', () => {
  it('test_tester_sha256Hex_matches_file_digest', () => {
    const dir = join(tmpdir(), `sign-ep-hex-${Date.now()}`);
    mkdirSync(dir, { recursive: true });
    const file = join(dir, 'sample.txt');
    writeFileSync(file, 'hello');
    expect(sha256Hex('hello')).toBe(sha256File(file));
  });

  it('test_tester_parsePrivateKeyMaterial_accepts_generated_ed25519_pem', () => {
    const { privateKey } = generateKeyPairSync('ed25519');
    const pem = privateKey.export({ type: 'pkcs8', format: 'pem' }).toString();
    expect(parsePrivateKeyMaterial(pem)).toBeDefined();
  });

  it('test_tester_signEpPackage_throws_when_signed_file_missing', () => {
    const dir = join(tmpdir(), `sign-ep-missing-${Date.now()}`);
    mkdirSync(dir, { recursive: true });
    writeFileSync(join(dir, 'manifest.json'), '{}');
    expect(() => signEpPackage({ packageDir: dir })).toThrow(/missing signed artifact/);
  });

  it('test_tester_parseSignArgs_reads_dir_and_out', () => {
    expect(parseSignArgs(['--dir=/pkg', '--out=/sig'])).toEqual({
      packageDir: '/pkg',
      outPath: '/sig',
    });
  });

  it('test_tester_parsePrivateKeyMaterial_rejects_invalid_seed_length', () => {
    expect(() => parsePrivateKeyMaterial(Buffer.from('short').toString('base64'))).toThrow(
      /32-byte Ed25519 seed/,
    );
  });
});
