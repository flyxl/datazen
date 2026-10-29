/** @vitest-environment node */
import { cpSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join, resolve } from 'node:path';
import { afterEach, describe, expect, it } from 'vitest';
import { collectReleaseVariantIssues, platformKeyForOsLabel } from '../check-release-variants.mjs';

const root = resolve(import.meta.dirname, '../..');

/**
 * Copies of the files the guard reads, so a mutation test cannot touch the repo.
 * Copies are taken from the working tree rather than hand-written, so a fixture
 * can never be the thing that goes stale.
 */
const GUARDED_FILES = [
  '.github/workflows/release.yml',
  'apps/desktop/src-tauri/tauri.conf.json',
  'packaging/winget/Flyxl.DataZen.yaml',
  'packaging/homebrew/datazen.rb',
];

const temporaryDirs: string[] = [];

afterEach(() => {
  while (temporaryDirs.length > 0) {
    rmSync(temporaryDirs.pop()!, { recursive: true, force: true });
  }
});

/**
 * @param {Record<string, (text: string) => string>} edits
 * @returns {string} root of the mutated copy
 */
function copyWithEdits(edits: Record<string, (text: string) => string>) {
  const dir = mkdtempSync(join(tmpdir(), 'datazen-release-variants-'));
  temporaryDirs.push(dir);
  for (const rel of GUARDED_FILES) {
    const target = join(dir, rel);
    mkdirSync(dirname(target), { recursive: true });
    cpSync(join(root, rel), target);
  }
  for (const [rel, edit] of Object.entries(edits)) {
    const path = join(dir, rel);
    const before = readFileSync(path, 'utf-8');
    const after = edit(before);
    // Without this the suite would pass whenever an edit silently stopped
    // matching — a mutation nobody applied proves nothing.
    expect(after, `fixture edit for ${rel} did not change anything`).not.toBe(before);
    writeFileSync(path, after);
  }
  return dir;
}

describe('check-release-variants', () => {
  it('reports the repository as consistent', () => {
    expect(collectReleaseVariantIssues(root)).toEqual([]);
  });

  it('maps release matrix os_labels onto updater platform keys', () => {
    expect(platformKeyForOsLabel('macos-arm64')).toBe('darwin-aarch64');
    expect(platformKeyForOsLabel('macos-x64')).toBe('darwin-x86_64');
    expect(platformKeyForOsLabel('windows-x64')).toBe('windows-x86_64');
    expect(platformKeyForOsLabel('linux-x64')).toBe('linux-x86_64');
    // Unrecognised labels must be reported rather than silently skipped, which
    // is how a renamed label would quietly drop a platform from every manifest.
    expect(platformKeyForOsLabel('linux-arm64')).toBeNull();
    expect(platformKeyForOsLabel('')).toBeNull();
  });

  it('catches a matrix leg whose artifact suffix disagrees with its SKU', () => {
    const dir = copyWithEdits({
      '.github/workflows/release.yml': (text) =>
        text.replace('variant_suffix: "-all"', 'variant_suffix: "-akulaku"'),
    });

    const issues = collectReleaseVariantIssues(dir);

    expect(issues).toHaveLength(1);
    expect(issues[0]).toContain('variant_suffix is "-akulaku"');
    expect(issues[0]).toContain('produces "-all" artifacts');
  });

  it('catches a default endpoint that is no longer Basic', () => {
    // Basic gets no build-time override, so tauri.conf.json *is* its channel.
    const dir = copyWithEdits({
      'apps/desktop/src-tauri/tauri.conf.json': (text) =>
        text.replace('download/latest.json', 'download/latest-all.json'),
    });

    const issues = collectReleaseVariantIssues(dir);

    expect(issues).toHaveLength(1);
    expect(issues[0]).toContain(
      'must include https://github.com/flyxl/datazen/releases/latest/download/latest.json',
    );
  });

  it('catches a missing updater pubkey', () => {
    // ci-tauri-build.mjs overrides `endpoints` only, so this is the single
    // pubkey for every SKU: losing it breaks signature verification everywhere.
    const dir = copyWithEdits({
      'apps/desktop/src-tauri/tauri.conf.json': (text) =>
        text.replace(/\n\s*"pubkey": "[^"]*",/, ''),
    });

    const issues = collectReleaseVariantIssues(dir);

    expect(issues).toHaveLength(1);
    expect(issues[0]).toContain('plugins.updater.pubkey is missing');
  });

  it('catches a SKU that lost a platform leg', () => {
    const dir = copyWithEdits({
      '.github/workflows/release.yml': (text) =>
        text.replaceAll('os_label: macos-arm64', 'os_label: windows-x64'),
    });

    const issues = collectReleaseVariantIssues(dir);

    expect(issues.some((issue) => issue.includes('darwin-aarch64'))).toBe(true);
    expect(issues.every((issue) => issue.includes('no build leg produces that artifact'))).toBe(
      true,
    );
  });

  it('catches a packaging template that names a variant artifact', () => {
    const dir = copyWithEdits({
      'packaging/winget/Flyxl.DataZen.yaml': (text) =>
        text.replace('DataZen-0.1.1-windows-x64.exe', 'DataZen-0.1.1-windows-x64-all.exe'),
    });

    const issues = collectReleaseVariantIssues(dir);

    expect(issues.some((issue) => issue.includes('references the "all" SKU'))).toBe(true);
  });

  it('catches a packaging template still using the retired -nsis name', () => {
    // 534e9633 reduced canonical_name() to the file extension; this URL 404s.
    const dir = copyWithEdits({
      'packaging/winget/Flyxl.DataZen.yaml': (text) =>
        text.replace('DataZen-0.1.1-windows-x64.exe', 'DataZen-0.1.1-windows-x64-nsis.exe'),
    });

    const issues = collectReleaseVariantIssues(dir);

    expect(issues.some((issue) => issue.includes('"-nsis" artifact name'))).toBe(true);
  });

  it('catches a packaging template that names no Basic artifact at all', () => {
    // The cask lists both macOS architectures, so every `.dmg` reference has to
    // go: leaving one intact would keep the template matching the expected shape.
    const dir = copyWithEdits({
      'packaging/homebrew/datazen.rb': (text) => text.replaceAll('.dmg', '.pkg'),
    });

    const issues = collectReleaseVariantIssues(dir);

    expect(issues.some((issue) => issue.includes('does not reference a Basic artifact'))).toBe(
      true,
    );
  });

  it('catches a manifest job that stopped publishing a SKU', () => {
    const dir = copyWithEdits({
      '.github/workflows/release.yml': (text) =>
        text.replace('for VARIANT in basic all akulaku', 'for VARIANT in basic all'),
    });

    const issues = collectReleaseVariantIssues(dir);

    expect(issues).toHaveLength(1);
    expect(issues[0]).toContain('must publish every SKU');
  });
});
