#!/usr/bin/env node
import { isDirectInvocation } from './lib/isDirectInvocation.mjs';
/**
 * ci-tauri-build.mjs — run the Tauri CLI using .driver-features.json.
 *
 * Used by CI inside with-driver-inject so we never nest `bash -c` through
 * Node spawn (that loses the -c script argument on Windows).
 *
 * Invokes `node node_modules/@tauri-apps/cli/tauri.js` directly. Going through
 * `pnpm.cmd` on Windows re-parses argv in cmd.exe and strips quotes from
 * `--config '{"bundle":...}'`.
 *
 * Usage:
 *   node scripts/ci-tauri-build.mjs --target=x86_64-pc-windows-msvc
 *   node scripts/ci-tauri-build.mjs --target=...
 */

import { readFileSync, existsSync, writeFileSync, mkdirSync, cpSync, rmSync } from 'fs';
import { spawnSync } from 'child_process';
import { createRequire } from 'module';
import { tmpdir } from 'os';
import { resolve, dirname, join, basename } from 'path';
import { fileURLToPath } from 'url';
import { stagingMarkerPath } from './pack-ep.mjs';
import { DEFAULT_REPO, manifestUrlForVariant, normalizeVariant } from './release-variants.mjs';

const __dirname = dirname(fileURLToPath(import.meta.url));
const ROOT = resolve(__dirname, '..');
const require = createRequire(import.meta.url);

export const UPDATER_CONFIG = { bundle: { createUpdaterArtifacts: true } };

export const PRO_CONFIG = {};

/**
 * Cargo profile the build actually uses. tauri-cli 2.10.1 exposes no
 * `--profile` flag, so there is no way to select a custom profile through
 * `tauri build` — it always builds the `release` profile unless `--debug` is
 * passed. Kept as a single constant so the bundle paths and the upx-compress
 * lookup cannot drift from the directory the build really writes to.
 */
export const BUILD_PROFILE = 'release';

/**
 * Source trees behind the `bundle.resources` map in tauri.conf.json, resolved
 * against `root` (repo root).
 *
 * The bundler only copies these into the bundles it produces — NSIS installer,
 * macOS .app, deb/AppImage. It never lays them out next to the raw binary in
 * `target/<triple>/<profile>/`, and the Windows portable archive step stages
 * exactly that layout (DataZen.exe + prompts/ + builtin-ep/ + *.dll).
 */
export function bundleResourceSources(root = ROOT) {
  return [
    {
      // tauri.conf.json: "../datazen/resources/prompts" -> "prompts".
      // Committed source tree; the portable archive cannot ship without it.
      name: 'prompts',
      source: join(root, 'crates', 'datazen', 'resources', 'prompts'),
      required: true,
    },
    {
      // tauri.conf.json: "resources/builtin-ep" -> "builtin-ep". Staged only
      // when the Pro edition resolved an extension; community builds have no
      // tree here and the bundler embeds nothing for it.
      name: 'builtin-ep',
      source: join(root, 'crates', 'datazen-desktop', 'resources', 'builtin-ep'),
      required: false,
    },
  ];
}

/**
 * Copy the `bundle.resources` trees into the Cargo out dir, next to the raw
 * binary — byte-for-byte the layout the NSIS installer creates under $INSTDIR
 * (and what `resolve_prompts_dir`/the builtin-ep loader expect next to a
 * portable DataZen.exe).
 *
 * This must run inside this process, right after `tauri build` succeeds:
 * `with-driver-inject` invokes `resolve-pro --restore` as soon as this script
 * exits, and that removes the staged builtin-ep tree from the checkout, so the
 * packaging step that follows cannot recover it from source.
 *
 * The destination is replaced (not merged) so a stale file from a previous
 * build in a cached target dir cannot leak into a fresh portable archive.
 * A missing non-required source is skipped silently; a missing `prompts`
 * tree warns here — the packaging step remains the hard gate.
 *
 * @param {{
 *   target?: string | null,
 *   profile?: string,
 *   root?: string,
 *   log?: (message: string) => void,
 * }} [opts]
 */
export function mirrorBundleResources({
  target = null,
  profile = BUILD_PROFILE,
  root = ROOT,
  log = console.log,
} = {}) {
  const outDir = target ? join(root, 'target', target, profile) : join(root, 'target', profile);
  const results = [];
  for (const resource of bundleResourceSources(root)) {
    const destination = join(outDir, resource.name);
    if (!existsSync(resource.source)) {
      if (resource.required) {
        log(`[ci-tauri-build] WARNING: bundle resource source missing: ${resource.source}`);
      }
      results.push({ ...resource, destination, mirrored: false });
      continue;
    }
    rmSync(destination, { recursive: true, force: true });
    cpSync(resource.source, destination, { recursive: true, force: true });
    log(`[ci-tauri-build] mirrored bundle resource ${resource.name} -> ${destination}`);
    results.push({ ...resource, destination, mirrored: true });
  }
  return { outDir, results };
}

/**
 * When `1`, the frontend build drops `tsc --noEmit`: the release matrix would
 * otherwise re-typecheck 11 times, and the driver warmup job already runs one
 * typecheck over the *union* driver set (a strict superset of every variant).
 */
export const TYPECHECK_ONCE_ENV = 'DATAZEN_CI_TYPECHECK_ONCE';

/** `pnpm build` without the `tsc --noEmit` pass. */
export const FAST_FRONTEND_BUILD_COMMAND = 'pnpm build:bundle';

/** Relative paths that must exist under the staged Pro extension tree. */
export const REQUIRED_PRO_STAGED_PATHS = ['manifest.json', 'dist/index.esm.js', 'signature.sig'];

export function builtinEpStagingDir(root = ROOT) {
  return join(root, 'crates/datazen-desktop', 'resources', 'builtin-ep', 'sql-editor-pro');
}

/**
 * Pre-flight check for Pro edition builds: verify the staged builtin-ep tree
 * exists *before* the ~10 min Tauri build starts. Returns the list of missing
 * relative paths (empty = ready). Emits a `::notice::` line per missing file
 * so failures are visible in check-run annotations without admin log access.
 *
 * Presence of the three required files is not sufficient on its own: a pack
 * that failed before its final staging step leaves all three in place, holding
 * the *previous* build's bytes and signature. The sibling `.incomplete` marker
 * is what distinguishes that from a tree this build produced, so a marked tree
 * is reported as not ready rather than shipped.
 */
export function checkProStagingReady({ root = ROOT, log = console.log } = {}) {
  const staging = builtinEpStagingDir(root);
  const missing = REQUIRED_PRO_STAGED_PATHS.filter((rel) => !existsSync(join(staging, rel)));
  const marker = stagingMarkerPath(staging);
  if (existsSync(marker)) {
    missing.push(basename(marker));
    log(
      `::error::[pro-staging] ${marker} exists — the last pack-ep run over ` +
        `${staging} failed, so the tree there is stale and must not be shipped. ` +
        `Re-run 'node scripts/resolve-pro.mjs --edition=pro'.`,
    );
  }
  if (missing.length > 0) {
    log(`::notice::[pro-staging] missing staged files under ${staging}: ${missing.join(', ')}`);
  } else {
    log(`[pro-staging] staged tree ready at ${staging}`);
  }
  return missing;
}

export function resolveTauriCli(root = ROOT) {
  return require.resolve('@tauri-apps/cli/tauri.js', { paths: [root] });
}

export function writeUpdaterConfigFile(dir = join(tmpdir(), 'datazen-ci-tauri')) {
  return writeTauriConfigFile({ updater: true, isPro: false, dir });
}

/**
 * Build the `--config` overlay for this variant.
 *
 * The updater endpoint lives in `tauri.conf.json`, which is shipped once for
 * every SKU — so left alone, an `-all` / `-akulaku` build would read the Basic
 * manifest and install a Basic build over itself (losing every driver Basic
 * does not ship). For SKUs that publish their own manifest we therefore
 * override `plugins.updater.endpoints` here.
 *
 * Tauri merges `--config` with JSON Merge Patch (RFC 7396) — `json_patch::merge`
 * called from tauri-utils `config::parse::read_from` — which recurses into
 * objects and *replaces* arrays. Emitting only `endpoints` therefore swaps just
 * the URL list and keeps `pubkey` and `windows.installMode` from the base
 * config; emitting a whole `plugins.updater` object is what would silently break
 * signature verification the moment it drifted from the base file.
 *
 * Basic gets no override: its endpoint is already the one in the base config,
 * so Basic builds stay byte-identical to what shipped before — which is what
 * lets already-installed Basic users keep updating.
 *
 * `variant` accepts the release-matrix spelling (`-all`); `repo` only affects
 * the URL text and is never resolved over the network here.
 */
export function updaterConfigOverlay(variant, repo = DEFAULT_REPO) {
  const endpoints = updaterEndpointsForVariant(variant, repo);
  return endpoints ? { plugins: { updater: { endpoints } } } : null;
}

/**
 * Endpoint list for a SKU, or `null` when the SKU must not override anything
 * (Basic) or has no published channel at all (`custom` and unknown SKUs).
 *
 * @param {string | null | undefined} variant
 * @param {string} [repo]
 * @returns {string[] | null}
 */
export function updaterEndpointsForVariant(variant, repo = DEFAULT_REPO) {
  const name = normalizeVariant(variant);
  // Basic already points at `latest.json` in tauri.conf.json — no overlay, so
  // the base config stays the single definition of the default channel.
  if (name === 'basic') return null;
  const url = manifestUrlForVariant(name, repo);
  return url ? [url] : null;
}

/**
 * Write the Tauri `--config` override that carries the updater block and the
 * injected build hook. `dir` is an override so tests do not share a temp path.
 *
 * @param {{
 *   updater?: boolean,
 *   isPro?: boolean,
 *   beforeBuildCommand?: string | null,
 *   variant?: string | null,
 *   dir?: string,
 * }} [opts]
 */
export function writeTauriConfigFile({
  updater = false,
  isPro = false,
  beforeBuildCommand = null,
  variant = null,
  dir = join(tmpdir(), 'datazen-ci-tauri'),
} = {}) {
  mkdirSync(dir, { recursive: true });
  const sku = normalizeVariant(variant);
  const file = join(
    dir,
    `config-${isPro ? 'pro' : 'base'}-${updater ? 'updater' : 'plain'}-${sku}${
      beforeBuildCommand ? '-fastfe' : ''
    }.json`,
  );
  const config = {};
  if (updater) {
    Object.assign(config, UPDATER_CONFIG);
  }
  if (isPro) {
    Object.assign(config, PRO_CONFIG);
  }
  const overlay = updaterConfigOverlay(sku);
  if (overlay) {
    Object.assign(config, overlay);
  }
  if (beforeBuildCommand) {
    config.build = { beforeBuildCommand };
  }
  writeFileSync(file, `${JSON.stringify(config)}\n`);
  return file;
}

/**
 * @param {{
 *   target?: string | null,
 *   updater?: boolean,
 *   edition?: string,
 *   features?: string[],
 *   configPath?: string | null,
 *   updaterConfigPath?: string | null,
 *   beforeBuildCommand?: string | null,
 *   variant?: string | null,
 *   extraArgs?: string[],
 * }} [opts]
 */
export function buildTauriArgs({
  target = null,
  updater = false,
  edition = 'community',
  features = [],
  configPath = null,
  updaterConfigPath = null,
  beforeBuildCommand = null,
  variant = null,
  extraArgs = [],
} = {}) {
  const args = ['build'];
  // 桌面 crate 自 W7-B 起 `default = ["drivers"]`：脚本化 SKU 构建必须先关
  // default，再靠下方显式 `-f <.driver-features.json>` 精确复现特征集，
  // 保证 basic/all/webdriver 的驱动集与 default=[] 时代逐项相同。
  //
  // `--no-default-features` 必须走尾部的 `--` 交给 runner（cargo），不能当 Tauri
  // CLI 自己的开关：CLI 2.10.1 起 build/dev 都不再接受它（`error: unexpected
  // argument '--no-default-features'`），直接把 `pnpm tauri:build:webdriver`
  // 和 release 的 SKU 构建打死。
  if (target) {
    args.push('--target', target);
  }
  const isPro = edition === 'pro';
  // A non-Basic SKU needs a config overlay even without `--updater`: the
  // endpoint must never point at another SKU's manifest.
  const needsVariantOverlay = updaterConfigOverlay(variant) != null;
  if (updater || isPro || beforeBuildCommand || needsVariantOverlay) {
    args.push(
      '--config',
      configPath ??
        updaterConfigPath ??
        writeTauriConfigFile({ updater, isPro, beforeBuildCommand, variant }),
    );
  }
  if (Array.isArray(features) && features.length > 0) {
    args.push('-f', features.join(','));
  }
  if (Array.isArray(extraArgs) && extraArgs.length > 0) {
    args.push(...extraArgs);
  }
  // runner 参数放在最后，`--` 之后的一切都原样交给 cargo。
  args.push('--', '--no-default-features');
  return args;
}

export function spawnTauri(args, { cwd = ROOT, env = process.env, log = console.log } = {}) {
  const cli = resolveTauriCli(cwd);
  log(`[ci-tauri-build] ${process.execPath} ${cli} ${args.join(' ')}`);
  const result = spawnSync(process.execPath, [cli, ...args], {
    cwd,
    stdio: 'inherit',
    shell: false,
    env,
    windowsHide: true,
  });
  if (result.error) {
    console.error('[ci-tauri-build] spawn failed:', result.error);
  }
  return result;
}

function main() {
  const argv = process.argv.slice(2);
  const targetArg = argv.find((a) => a.startsWith('--target='));
  const target = targetArg ? targetArg.slice('--target='.length) : null;
  const variantArg = argv.find((a) => a.startsWith('--variant='));
  const beforeBuildCommand =
    process.env[TYPECHECK_ONCE_ENV] === '1' ? FAST_FRONTEND_BUILD_COMMAND : null;
  const isPro =
    argv.includes('--pro') ||
    argv.includes('--edition=pro') ||
    process.env.DATAZEN_EDITION === 'pro';
  const edition = isPro ? 'pro' : 'community';

  const knownPrefixes = ['--target=', '--edition=', '--variant='];
  const knownFlags = new Set(['--pro', '--community', '--updater']);
  const extraArgs = argv.filter((a) => {
    if (knownFlags.has(a)) return false;
    if (knownPrefixes.some((p) => a.startsWith(p))) return false;
    return true;
  });

  const featuresPath = resolve(ROOT, '.driver-features.json');
  if (!existsSync(featuresPath)) {
    console.error('[ci-tauri-build] missing .driver-features.json — run resolve-drivers first');
    process.exit(1);
  }

  const { features, variant: featuresVariant } = JSON.parse(readFileSync(featuresPath, 'utf-8'));

  // The release SKU decides which updater endpoint this build compiles in.
  // resolve-drivers stamps it into .driver-features.json; an explicit
  // --variant= wins, and DATAZEN_VARIANT covers a features file written before
  // that field existed.
  const variantArgValue = variantArg ? variantArg.slice('--variant='.length) : null;
  const variant = normalizeVariant(
    variantArgValue ?? featuresVariant ?? process.env.DATAZEN_VARIANT,
  );
  console.log(`[ci-tauri-build] release variant: ${variant}`);

  // Fail fast: a missing staged tree means the .deb/.app would ship without
  // the Pro extension — better to stop here than after a 10-min build.
  if (edition === 'pro') {
    const missing = checkProStagingReady();
    if (missing.length > 0) {
      console.error(
        `[ci-tauri-build] pro staging incomplete, missing: ${missing.join(', ')} ` +
          `(expected under ${builtinEpStagingDir()})`,
      );
      process.exit(1);
    }
  }

  // A SKU with its own channel must never be built against another SKU's
  // manifest. Getting the endpoint wrong is silent and only shows up as a
  // downgraded install on a user's machine, so it is logged loudly here.
  const endpoints = updaterEndpointsForVariant(variant);
  if (endpoints) {
    console.log(`[ci-tauri-build] updater endpoint: ${endpoints[0]}`);
  }

  const args = buildTauriArgs({
    target,
    updater: argv.includes('--updater'),
    edition,
    features,
    beforeBuildCommand,
    variant,
    extraArgs,
  });
  const result = spawnTauri(args);
  if (result.status === 0) {
    // The bundler embeds bundle.resources into the bundles it produces only;
    // mirror them next to the raw binary while every source tree is still on
    // disk. This must happen before this process exits — the wrapping
    // with-driver-inject then runs `resolve-pro --restore`, which deletes the
    // staged builtin-ep tree the Windows portable packaging step needs.
    mirrorBundleResources({ target });
  }
  process.exit(result.status ?? 1);
}

if (isDirectInvocation(import.meta.url)) {
  main();
}
