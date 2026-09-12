/** @vitest-environment node */
import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, it } from 'vitest';

const root = resolve(import.meta.dirname, '../..');
const releaseWorkflow = readFileSync(resolve(root, '.github/workflows/release.yml'), 'utf8');
const windowsConfig = JSON.parse(
  readFileSync(resolve(root, 'src-tauri/tauri.windows.conf.json'), 'utf8'),
);
const windowsReleaseDocs = [
  '.github/workflows/release.yml',
  'README.md',
  'README.zh-CN.md',
  'docs/development/packaging.md',
  'docs/development/updater.md',
  'site/download.html',
  'site/zh/download.html',
  'site/manual.html',
  'site/zh/manual.html',
];

describe('Windows release packaging', () => {
  it('only asks Tauri to build the NSIS installer on Windows', () => {
    expect(windowsConfig.bundle.targets).toEqual(['nsis']);
  });

  it.each(windowsReleaseDocs)('%s no longer advertises MSI packages', (relativePath) => {
    const contents = readFileSync(resolve(root, relativePath), 'utf8');
    expect(contents).not.toMatch(/\bmsi\b/i);
  });

  it('publishes an installer-free portable archive with runtime resources', () => {
    expect(releaseWorkflow).toContain('Package Windows portable archive');
    // New naming: DataZen-{version}-{osLabel}[-variant]-portable.zip
    expect(releaseWorkflow).toContain('DataZen-$version-$osLabel-portable.zip');
    expect(releaseWorkflow).toContain('Copy-Item -LiteralPath $prompts');
    // Portable includes Pro extension resources
    expect(releaseWorkflow).toContain('builtin-ep');
    expect(releaseWorkflow).toContain('Copy-Item -LiteralPath $builtinEp');
  });

  it('builds Pro edition by default in release workflow and excludes pure community builds', () => {
    // Assert all matrix entries have edition: "pro" and needs_pro: true
    expect(releaseWorkflow).toContain('edition: "pro"');
    expect(releaseWorkflow).toContain('needs_pro: true');
    expect(releaseWorkflow).not.toMatch(/edition:\s*"community"/);
    expect(releaseWorkflow).toMatch(/variant_suffix:\s*"-all"/);
  });

  it('drops Akulaku Linux matrix entries', () => {
    // Akulaku should only have Windows and macOS — no ubuntu / linux
    const akulakuBlock = releaseWorkflow.slice(
      releaseWorkflow.indexOf('# ── Akulaku'),
      releaseWorkflow.indexOf('# ── Akulaku') + 500,
    );
    expect(akulakuBlock).not.toContain('ubuntu-22.04');
    expect(akulakuBlock).not.toContain('linux-x64');
  });

  it('uses canonical artifact naming: DataZen-{Version}-{Platform}-{Arch}[-{Variant}]-{Type}.{ext}', () => {
    expect(releaseWorkflow).toContain('Rename artifacts with canonical names');
    // Canonical name function
    expect(releaseWorkflow).toContain('DataZen-${VERSION}-${PLATFORM}-${ARCH}');
    // Pro verification step checks actual bundled output (.app / deb), not staging dir
    expect(releaseWorkflow).toContain('Verify Pro extension is bundled in app');
    expect(releaseWorkflow).toContain('builtin-ep/sql-editor-pro/dist/index.esm.js');
  });

  it('downloads the prebuilt Pro tarball with a dedicated token and emits notices on fallback', () => {
    expect(releaseWorkflow).toContain('Download prebuilt Pro extension (fast path)');
    // Dedicated PAT secret: GITHUB_TOKEN cannot read the private Pro repo's releases
    expect(releaseWorkflow).toContain('PRO_PREBUILT_TOKEN');
    expect(releaseWorkflow).toContain('Authorization: Bearer');
    // Private-repo assets must resolve via the API (browser CDN URL 404s with Bearer)
    expect(releaseWorkflow).toContain('Accept: application/octet-stream');
    expect(releaseWorkflow).toContain('releases/tags/');
    // curl failure reason must surface in annotations without admin log access
    expect(releaseWorkflow).toContain('::notice::Prebuilt download failed');
    // Verify step emits notices so the next failure is diagnosable from annotations
    expect(releaseWorkflow).toContain('::notice::[pro-verify]');
  });
});
