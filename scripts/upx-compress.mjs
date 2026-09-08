#!/usr/bin/env node
/**
 * upx-compress.mjs
 *
 * Compresses compiled release binaries using UPX to significantly reduce installer
 * and executable size (typically ~60% reduction on Windows).
 *
 * Invoked automatically by Tauri's `beforeBundleCommand` prior to installer generation,
 * or can be run standalone.
 *
 * Platform policy:
 *  - Windows: Fully supported. Compresses target executable with LZMA before NSIS/zip packaging.
 *  - macOS: Skipped. UPX breaks Apple Code Signing, Hardened Runtime & Notarization.
 *  - Linux: Skipped by default. Tauri's AppImage/deb packaging runs `patchelf` on the ELF binary,
 *           which fails on UPX-compressed headers. Set DATAZEN_UPX_FORCE=1 to override.
 */

import { existsSync, readdirSync, statSync } from 'fs';
import { resolve, dirname, join, basename } from 'path';
import { fileURLToPath } from 'url';
import { execSync } from 'child_process';

const __dirname = dirname(fileURLToPath(import.meta.url));
export const ROOT = resolve(__dirname, '..');

export function isUpxAvailable() {
  try {
    execSync('upx --version', { stdio: 'ignore' });
    return true;
  } catch {
    return false;
  }
}

export function shouldCompressPlatform(platform = process.platform, env = process.env) {
  if (env.DATAZEN_UPX_FORCE === '1') {
    return { shouldRun: true, reason: 'forced by DATAZEN_UPX_FORCE=1' };
  }
  if (platform === 'win32') {
    return { shouldRun: true, reason: 'Windows platform supported' };
  }
  if (platform === 'darwin') {
    return {
      shouldRun: false,
      reason: 'macOS skipped (UPX breaks Code Signing & Notarization)',
    };
  }
  return {
    shouldRun: false,
    reason: 'Linux skipped by default (conflicts with Tauri patchelf bundler). Set DATAZEN_UPX_FORCE=1 to force.',
  };
}

export function findTargetExecutables(root = ROOT, platform = process.platform) {
  const targetDir = resolve(root, 'target');
  if (!existsSync(targetDir)) return [];

  const candidates = [];
  const isWindows = platform === 'win32' || process.env.DATAZEN_TARGET_OS === 'windows';

  // Search paths: target/release and target/<target-triple>/release
  const searchDirs = [];
  const defaultRelease = join(targetDir, 'release');
  if (existsSync(defaultRelease)) {
    searchDirs.push(defaultRelease);
  }

  try {
    const entries = readdirSync(targetDir);
    for (const entry of entries) {
      if (entry === 'release' || entry === 'debug') continue;
      const subRelease = join(targetDir, entry, 'release');
      if (existsSync(subRelease) && statSync(subRelease).isDirectory()) {
        searchDirs.push(subRelease);
      }
    }
  } catch {
    // ignore directory read error
  }

  for (const dir of searchDirs) {
    try {
      const files = readdirSync(dir);
      for (const file of files) {
        const fullPath = join(dir, file);
        if (!statSync(fullPath).isFile()) continue;

        const lower = file.toLowerCase();
        if (isWindows) {
          if (
            lower.endsWith('.exe') &&
            (lower.startsWith('datazen') || lower.startsWith('datazen pro'))
          ) {
            candidates.push(fullPath);
          }
        } else {
          if (
            (lower === 'datazen' || lower === 'datazen pro' || lower === 'datazen-pro') &&
            !file.includes('.')
          ) {
            candidates.push(fullPath);
          }
        }
      }
    } catch {
      // ignore directory access error
    }
  }

  return Array.from(new Set(candidates));
}

export function compressExecutable(filePath, { log = console.log, exec = execSync } = {}) {
  const name = basename(filePath);
  const sizeBefore = statSync(filePath).size;
  log(`[upx] compressing: ${name} (${(sizeBefore / 1024 / 1024).toFixed(2)} MB)...`);

  try {
    exec(`upx --lzma --best "${filePath}"`, { stdio: 'inherit' });
    const sizeAfter = statSync(filePath).size;
    const ratio = (((sizeBefore - sizeAfter) / sizeBefore) * 100).toFixed(1);
    log(
      `[upx] ✓ ${name}: ${(sizeBefore / 1024 / 1024).toFixed(2)} MB → ${(sizeAfter / 1024 / 1024).toFixed(2)} MB (-${ratio}%)`,
    );
    return { success: true, sizeBefore, sizeAfter };
  } catch (err) {
    log(`[upx] ⚠ warning: UPX compression failed for ${name}: ${err.message}`);
    return { success: false, error: err.message };
  }
}

export function runUpxCompression({
  platform = process.platform,
  env = process.env,
  root = ROOT,
  log = console.log,
} = {}) {
  const policy = shouldCompressPlatform(platform, env);
  if (!policy.shouldRun) {
    log(`[upx] ${policy.reason}`);
    return { skipped: true, reason: policy.reason };
  }

  if (!isUpxAvailable()) {
    log('[upx] UPX executable not found in PATH; skipping compression.');
    return { skipped: true, reason: 'upx not found in PATH' };
  }

  const executables = findTargetExecutables(root, platform);
  if (executables.length === 0) {
    log('[upx] no target release binaries found to compress.');
    return { skipped: true, reason: 'no executables found' };
  }

  const results = [];
  for (const exe of executables) {
    results.push(compressExecutable(exe, { log }));
  }

  return { skipped: false, results };
}

// CLI entry point
const isMain = process.argv[1] && fileURLToPath(import.meta.url) === resolve(process.argv[1]);
if (isMain) {
  try {
    runUpxCompression();
  } catch (err) {
    console.error('[upx] unexpected error:', err);
    // Non-fatal: do not fail build pipeline if compression throws
    process.exit(0);
  }
}
