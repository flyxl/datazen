#!/usr/bin/env node
/**
 * pack-ep.mjs — build, sign, and package privileged extensions (.dzx) or stage
 * them as Tauri builtin static resources.
 *
 * Usage:
 *   node scripts/pack-ep.mjs --extension=sql-editor-pro
 *   node scripts/pack-ep.mjs --extension=sql-editor-pro --mode=dzx --out=artifacts/
 *   node scripts/pack-ep.mjs --extension=sql-editor-pro --mode=stage
 *   node scripts/pack-ep.mjs --extension=sql-editor-pro --mode=both
 *   node scripts/pack-ep.mjs --dir=/path/to/extension --mode=dzx --skip-build
 */

import { execSync } from 'child_process';
import {
  cpSync,
  existsSync,
  mkdirSync,
  readFileSync,
  readdirSync,
  rmSync,
  writeFileSync,
} from 'fs';
import { dirname, join, relative, resolve } from 'path';
import { fileURLToPath } from 'url';
import { zipSync } from 'fflate';
import { signEpPackage } from './sign-ep.mjs';

const __dirname = dirname(fileURLToPath(import.meta.url));
export const ROOT = resolve(__dirname, '..');
export const DEFAULT_PRO_DEST = resolve(ROOT, 'packages/pro-extensions/sql-editor-pro');
export const DEFAULT_BUILTIN_EP_ROOT = resolve(ROOT, 'src-tauri/resources/builtin-ep');
export const DEFAULT_DZX_OUT_DIR = resolve(ROOT, 'artifacts');

/** Relative paths that must exist in a staged / packed extension tree. */
export const REQUIRED_PACKAGE_PATHS = [
  'manifest.json',
  'dist/index.esm.js',
  'signature.sig',
];

/** Optional locale sources copied into locales/ when present. */
export const LOCALE_SOURCE_DIR = 'src/locales';

export function parsePackArgs(argv = process.argv.slice(2)) {
  let extension = 'sql-editor-pro';
  let extensionDir = null;
  let mode = 'both';
  let outDir = DEFAULT_DZX_OUT_DIR;
  let stageDir = null;
  let skipBuild = false;

  for (const arg of argv) {
    if (arg.startsWith('--extension=')) {
      extension = arg.slice('--extension='.length);
    } else if (arg.startsWith('--dir=')) {
      extensionDir = arg.slice('--dir='.length);
    } else if (arg.startsWith('--mode=')) {
      mode = arg.slice('--mode='.length);
    } else if (arg.startsWith('--out=')) {
      outDir = arg.slice('--out='.length);
    } else if (arg.startsWith('--stage-dir=')) {
      stageDir = arg.slice('--stage-dir='.length);
    } else if (arg === '--skip-build') {
      skipBuild = true;
    }
  }

  const resolvedExtensionDir =
    extensionDir ?? (extension === 'sql-editor-pro' ? DEFAULT_PRO_DEST : join(ROOT, 'packages/pro-extensions', extension));
  const resolvedStageDir =
    stageDir ?? join(DEFAULT_BUILTIN_EP_ROOT, extension);

  return {
    extension,
    extensionDir: resolve(resolvedExtensionDir),
    mode,
    outDir: resolve(outDir),
    stageDir: resolve(resolvedStageDir),
    skipBuild,
  };
}

export function readManifestVersion(manifestPath) {
  const manifest = JSON.parse(readFileSync(manifestPath, 'utf8'));
  if (!manifest.version || typeof manifest.version !== 'string') {
    throw new Error(`[pack-ep] manifest.json missing string "version" at ${manifestPath}`);
  }
  return manifest.version;
}

export function buildExtensionLibrary(extensionDir, { log = console.log } = {}) {
  if (!existsSync(join(extensionDir, 'package.json'))) {
    throw new Error(`[pack-ep] extension package.json not found under ${extensionDir}`);
  }
  log(`[pack-ep] building extension library in ${extensionDir}`);
  execSync('npx vite build', {
    cwd: extensionDir,
    stdio: 'inherit',
    env: process.env,
  });
  const bundle = join(extensionDir, 'dist/index.esm.js');
  if (!existsSync(bundle)) {
    throw new Error(`[pack-ep] build did not produce dist/index.esm.js under ${extensionDir}`);
  }
}

export function syncLocales(extensionDir, targetRoot, { log = console.log } = {}) {
  const localesOut = join(targetRoot, 'locales');
  mkdirSync(localesOut, { recursive: true });

  const explicitLocalesDir = join(extensionDir, 'locales');
  if (existsSync(explicitLocalesDir)) {
    cpSync(explicitLocalesDir, localesOut, { recursive: true });
    log(`[pack-ep] copied locales/ from ${explicitLocalesDir}`);
    return;
  }

  const srcLocales = join(extensionDir, LOCALE_SOURCE_DIR);
  if (!existsSync(srcLocales)) {
    return;
  }

  for (const name of readdirSync(srcLocales)) {
    if (!/\.(ts|js|json)$/.test(name)) continue;
    cpSync(join(srcLocales, name), join(localesOut, name));
  }
  log(`[pack-ep] copied locale sources from ${srcLocales}`);
}

export function stagePackageTree(sourceDir, targetDir, { log = console.log } = {}) {
  if (existsSync(targetDir)) {
    rmSync(targetDir, { recursive: true, force: true });
  }
  mkdirSync(targetDir, { recursive: true });

  cpSync(join(sourceDir, 'manifest.json'), join(targetDir, 'manifest.json'));
  mkdirSync(join(targetDir, 'dist'), { recursive: true });
  cpSync(join(sourceDir, 'dist/index.esm.js'), join(targetDir, 'dist/index.esm.js'));
  if (existsSync(join(sourceDir, 'dist/index.esm.js.map'))) {
    cpSync(join(sourceDir, 'dist/index.esm.js.map'), join(targetDir, 'dist/index.esm.js.map'));
  }
  syncLocales(sourceDir, targetDir, { log });
  signEpPackage({ packageDir: targetDir });
}

export function assertPackageLayout(packageDir) {
  const missing = REQUIRED_PACKAGE_PATHS.filter((rel) => !existsSync(join(packageDir, rel)));
  if (missing.length > 0) {
    throw new Error(`[pack-ep] incomplete package at ${packageDir}; missing: ${missing.join(', ')}`);
  }
}

export function listZipEntries(rootDir, currentDir = rootDir, acc = {}) {
  for (const name of readdirSync(currentDir, { withFileTypes: true })) {
    const abs = join(currentDir, name.name);
    const rel = relative(rootDir, abs).split('\\').join('/');
    if (name.isDirectory()) {
      listZipEntries(rootDir, abs, acc);
    } else {
      acc[rel] = readFileSync(abs);
    }
  }
  return acc;
}

export function createDzxArchive(packageDir, outFile) {
  assertPackageLayout(packageDir);
  const entries = listZipEntries(packageDir);
  const zipped = zipSync(entries, { level: 9 });
  mkdirSync(dirname(outFile), { recursive: true });
  writeFileSync(outFile, Buffer.from(zipped));
  return outFile;
}

export function dzxFileName(extensionId, version) {
  return `${extensionId}-${version}.dzx`;
}

/**
 * @param {{
 *   extension?: string,
 *   extensionDir?: string,
 *   mode?: 'dzx' | 'stage' | 'both',
 *   outDir?: string,
 *   stageDir?: string,
 *   skipBuild?: boolean,
 *   log?: (...args: unknown[]) => void,
 * }} [opts]
 */
export function packEp(opts = {}) {
  const parsed = parsePackArgs();
  const extension = opts.extension ?? parsed.extension;
  const extensionDir = resolve(opts.extensionDir ?? parsed.extensionDir);
  const mode = opts.mode ?? parsed.mode;
  const outDir = resolve(opts.outDir ?? parsed.outDir);
  const stageDir = resolve(opts.stageDir ?? join(DEFAULT_BUILTIN_EP_ROOT, extension));
  const skipBuild = opts.skipBuild ?? parsed.skipBuild;
  const log = opts.log ?? console.log.bind(console);

  if (!existsSync(extensionDir)) {
    throw new Error(`[pack-ep] extension directory not found: ${extensionDir}`);
  }

  if (!skipBuild) {
    buildExtensionLibrary(extensionDir, { log });
  } else if (!existsSync(join(extensionDir, 'dist/index.esm.js'))) {
    throw new Error(
      `[pack-ep] --skip-build requires existing dist/index.esm.js under ${extensionDir}`,
    );
  }

  const workDir = join(outDir, `.pack-ep-staging-${extension}`);
  stagePackageTree(extensionDir, workDir, { log });
  assertPackageLayout(workDir);

  const manifestVersion = readManifestVersion(join(workDir, 'manifest.json'));
  const dzxName = dzxFileName(extension, manifestVersion);
  const dzxPath = join(outDir, dzxName);

  const result = {
    extension,
    extensionDir,
    stageDir,
    workDir,
    manifestVersion,
    dzxPath: null,
    staged: false,
  };

  if (mode === 'stage' || mode === 'both') {
    stagePackageTree(workDir, stageDir, { log });
    result.staged = true;
    log(`[pack-ep] staged signed extension to ${stageDir}`);
  }

  if (mode === 'dzx' || mode === 'both') {
    createDzxArchive(workDir, dzxPath);
    result.dzxPath = dzxPath;
    log(`[pack-ep] wrote ${dzxPath}`);
  }

  if (mode !== 'stage' && mode !== 'dzx' && mode !== 'both') {
    throw new Error(`[pack-ep] unknown mode "${mode}" (expected dzx, stage, or both)`);
  }

  rmSync(workDir, { recursive: true, force: true });
  return result;
}

const isMain = process.argv[1] && fileURLToPath(import.meta.url) === resolve(process.argv[1]);
if (isMain) {
  try {
    packEp();
  } catch (err) {
    console.error(err);
    process.exit(1);
  }
}
