#!/usr/bin/env node
/**
 * publish-pro-prebuilt.mjs — Build, sign, package, and upload the SQL Editor
 * Pro extension as a prebuilt tarball to the Pro repo's GitHub Release.
 *
 * Host CI then downloads the tarball instead of cloning the private repo and
 * building from source — no SSH deploy key needed for the Pro extension.
 *
 * Prerequisites:
 *   - Node.js + pnpm installed locally
 *   - `gh` CLI authenticated (or GH_TOKEN set) with access to the Pro repo
 *   - DATAZEN_EP_SIGNING_PRIVATE_KEY set (or falls back to test key)
 *
 * Usage:
 *   node scripts/publish-pro-prebuilt.mjs                     # full build + upload
 *   node scripts/publish-pro-prebuilt.mjs --skip-build        # package + upload only
 *   node scripts/publish-pro-prebuilt.mjs --dry-run           # build + package, no upload
 *   node scripts/publish-pro-prebuilt.mjs --pro-path=/path    # custom pro source dir
 *   node scripts/publish-pro-prebuilt.mjs --repo=owner/repo   # custom pro repo
 *
 * Output:
 *   - sql-editor-pro-{VERSION}.tar.gz  (cleaned up after upload)
 *   - Release tag: pro-{VERSION} on the Pro repo
 */

import { execSync } from 'child_process';
import { existsSync, readFileSync, statSync, unlinkSync } from 'fs';
import { resolve, dirname } from 'path';
import { fileURLToPath } from 'url';
import { packEp } from './pack-ep.mjs';

const __dirname = dirname(fileURLToPath(import.meta.url));
const ROOT = resolve(__dirname, '..');
const DEFAULT_PRO_DIR = resolve(ROOT, 'packages/pro-extensions/sql-editor-pro');
const PRO_REPO_DEFAULT = 'flyxl/datazen-extension-sql-editor-pro';

// ── Parse CLI args ───────────────────────────────────────────────────────────
let skipBuild = false;
let dryRun = false;
let proPath = DEFAULT_PRO_DIR;
let proRepo = PRO_REPO_DEFAULT;

for (const arg of process.argv.slice(2)) {
  if (arg === '--skip-build') skipBuild = true;
  else if (arg === '--dry-run') dryRun = true;
  else if (arg.startsWith('--pro-path=')) proPath = resolve(arg.slice('--pro-path='.length));
  else if (arg.startsWith('--repo=')) proRepo = arg.slice('--repo='.length);
}

// ── 1. Read manifest version ─────────────────────────────────────────────────
const manifestPath = resolve(proPath, 'manifest.json');
if (!existsSync(manifestPath)) {
  console.error(`[publish-pro-prebuilt] manifest.json not found at ${manifestPath}`);
  process.exit(1);
}
const manifest = JSON.parse(readFileSync(manifestPath, 'utf-8'));
const version = manifest.version;
const tag = `pro-${version}`;
const tarballName = `sql-editor-pro-${version}.tar.gz`;
const tarballPath = resolve(ROOT, tarballName);

console.log(`[publish-pro-prebuilt] Extension: sql-editor-pro v${version}`);
console.log(`[publish-pro-prebuilt] Pro repo: ${proRepo}`);
console.log(`[publish-pro-prebuilt] Release tag: ${tag}`);

// ── 2. Build (optional) ──────────────────────────────────────────────────────
if (!skipBuild) {
  console.log(`\n[publish-pro-prebuilt] Building extension in ${proPath}...`);

  // Install dependencies
  const hasLock = existsSync(resolve(proPath, 'pnpm-lock.yaml'));
  const installCmd = hasLock ? 'pnpm install --frozen-lockfile' : 'pnpm install';
  execSync(installCmd, { cwd: proPath, stdio: 'inherit', env: process.env });

  // Build the extension
  execSync('npx vite build', { cwd: proPath, stdio: 'inherit', env: process.env });

  // Verify build output
  const bundle = resolve(proPath, 'dist/index.esm.js');
  if (!existsSync(bundle)) {
    console.error('[publish-pro-prebuilt] Build failed: dist/index.esm.js not found');
    process.exit(1);
  }
  console.log('[publish-pro-prebuilt] Build complete ✓');
}

// ── 3. Use packEp to stage + sign ────────────────────────────────────────────
console.log(`\n[publish-pro-prebuilt] Staging and signing package...`);

const stagingDir = resolve(ROOT, '.pro-prebuilt-staging');
const result = packEp({
  extension: 'sql-editor-pro',
  extensionDir: proPath,
  mode: 'stage',
  stageDir: stagingDir,
  skipBuild: true, // already built above
});

console.log(`[publish-pro-prebuilt] Staged to ${stagingDir}`);

// ── 4. Create tarball ────────────────────────────────────────────────────────
console.log(`\n[publish-pro-prebuilt] Creating tarball: ${tarballName}`);

try {
  execSync(
    `tar -czf "${tarballPath}" -C "${stagingDir}" .`,
    { stdio: 'pipe' }
  );
} catch (err) {
  console.error(`[publish-pro-prebuilt] tar failed:`, err.message);
  process.exit(1);
}

const { size } = statSync(tarballPath);
console.log(`[publish-pro-prebuilt] Tarball: ${(size / 1024).toFixed(1)} KB`);

// Show contents
try {
  const listing = execSync(`tar -tzf "${tarballPath}"`, { encoding: 'utf-8' }).trim();
  console.log(`[publish-pro-prebuilt] Contents:\n${listing.split('\n').map(l => '  ' + l).join('\n')}`);
} catch {}

if (dryRun) {
  console.log(`\n[publish-pro-prebuilt] Dry run — tarball at: ${tarballPath}`);
  // Cleanup staging
  execSync(`rm -rf "${stagingDir}"`, { stdio: 'pipe' });
  process.exit(0);
}

// ── 5. Upload to GitHub Release ──────────────────────────────────────────────
console.log(`\n[publish-pro-prebuilt] Uploading to GitHub Release...`);

try {
  // Delete existing release (ignore if not found)
  execSync(
    `gh release delete "${tag}" --repo "${proRepo}" --yes 2>/dev/null || true`,
    { stdio: 'pipe', env: process.env }
  );

  // Create release and upload asset
  execSync(
    [
      `gh release create "${tag}" "${tarballPath}"`,
      `--repo "${proRepo}"`,
      `--title "Prebuilt v${version}"`,
      `--notes "Prebuilt SQL Editor Pro extension for host CI."`,
      `--latest=false`,
    ].join(' '),
    { stdio: 'inherit', env: process.env }
  );

  console.log(`\n[publish-pro-prebuilt] ✅ Uploaded to ${proRepo} release "${tag}"`);
  console.log(`[publish-pro-prebuilt] Download URL:`);
  console.log(`  https://github.com/${proRepo}/releases/download/${tag}/${tarballName}`);
} catch (err) {
  console.error(`[publish-pro-prebuilt] Upload failed:`, err.message);
  process.exit(1);
}

// ── 6. Cleanup ───────────────────────────────────────────────────────────────
try { unlinkSync(tarballPath); } catch {}
try { execSync(`rm -rf "${stagingDir}"`, { stdio: 'pipe' }); } catch {}

console.log('[publish-pro-prebuilt] Done ✓');
