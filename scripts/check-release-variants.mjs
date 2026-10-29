#!/usr/bin/env node
import { isDirectInvocation } from './lib/isDirectInvocation.mjs';
/**
 * Release-variant consistency guard.
 *
 * Every release SKU must publish, and every build must read, its own updater
 * manifest. That claim is spread over four files that know nothing about each
 * other — the release matrix, the app config, the build script and the packaging
 * templates — so a change on one side used to be able to silently break the
 * others. The shape of that failure is what makes it worth a guard:
 *
 *   - an `-all` build pointing at Basic's manifest installs Basic over itself and
 *     drops drivers, with no error anywhere;
 *   - a manifest missing one platform makes `check()` report "up to date"
 *     forever for that platform;
 *   - `plugins.updater.pubkey` lives only in `tauri.conf.json` (the build script
 *     overrides `endpoints` alone), so losing it breaks signature verification
 *     for every SKU at once.
 *
 * Run: node scripts/check-release-variants.mjs
 * Exit 0 when every side agrees; exit 1 with a per-problem report otherwise.
 *
 * `scripts/release-variants.mjs` is the single source of truth for SKU names,
 * manifest names and platform sets; this script only checks that the other files
 * still agree with it.
 */
import { existsSync, readFileSync } from 'fs';
import { dirname, resolve } from 'path';
import { fileURLToPath } from 'url';
import { parse as parseYaml } from 'yaml';
import {
  VARIANT_NAMES,
  artifactSuffixForVariant,
  manifestNameForVariant,
  manifestUrlForVariant,
  platformsForVariant,
} from './release-variants.mjs';

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), '..');

const WORKFLOW = '.github/workflows/release.yml';
const TAURI_CONF = 'apps/desktop/src-tauri/tauri.conf.json';

/**
 * Packaging templates that are pasted into Homebrew / WinGet releases. They must
 * name a Basic artifact: those channels ship the default build only, and an
 * `-all` / `-akulaku` artifact there would install drivers the channel never
 * advertises.
 *
 * The frozen `0.1.1` manifests under `packaging/winget/manifests/` are
 * deliberately excluded: they record what was submitted for that version, and
 * 0.1.1 genuinely shipped `-nsis` names before `534e9633` reduced
 * `canonical_name()` to the file extension.
 */
const PACKAGING_TEMPLATES = {
  'packaging/winget/Flyxl.DataZen.yaml': /DataZen-[\d.]+-windows-x64(-portable)?\.(exe|zip)/,
  'packaging/homebrew/datazen.rb': /DataZen-#\{version\}-macos-(arm64|x64)\.dmg/,
};

/**
 * Map a release-matrix `os_label` to the updater platform key it produces.
 * Returns null for labels this guard does not recognise.
 *
 * @param {string} label
 * @returns {string | null}
 */
export function platformKeyForOsLabel(label) {
  const [os, arch] = String(label).split('-');
  if (os === 'windows' && arch === 'x64') return 'windows-x86_64';
  if (os === 'linux' && arch === 'x64') return 'linux-x86_64';
  if (os === 'macos' && arch === 'arm64') return 'darwin-aarch64';
  if (os === 'macos' && arch === 'x64') return 'darwin-x86_64';
  return null;
}

/**
 * @param {string} root
 * @param {string} rel
 * @returns {unknown | null}
 */
function readYaml(root, rel) {
  const text = readFileSync(resolve(root, rel), 'utf-8');
  return parseYaml(text);
}

/**
 * @param {string} root
 * @returns {string[]} one human-readable line per problem; empty when consistent
 */
export function collectReleaseVariantIssues(root = ROOT) {
  const issues = [];
  const workflowPath = resolve(root, WORKFLOW);
  const confPath = resolve(root, TAURI_CONF);

  // ── 1. The release matrix must build exactly the SKUs and platforms declared
  // for release, with artifact suffixes matching those SKUs.
  let workflow = null;
  if (!existsSync(workflowPath)) {
    issues.push(`${WORKFLOW} is missing — the release matrix cannot be verified`);
  } else {
    try {
      workflow = readYaml(root, WORKFLOW);
    } catch (error) {
      issues.push(`${WORKFLOW} is not valid YAML: ${error.message}`);
    }
  }

  const matrix = workflow?.jobs?.build?.strategy?.matrix?.include;
  if (workflow && !Array.isArray(matrix)) {
    issues.push(`${WORKFLOW}: jobs.build.strategy.matrix.include is missing`);
  } else if (Array.isArray(matrix)) {
    for (const [index, entry] of matrix.entries()) {
      const leg = `${entry?.platform ?? `leg #${index}`} ${entry?.os_label ?? ''}`.trim();
      if (!VARIANT_NAMES.includes(entry?.variant)) {
        issues.push(
          `${WORKFLOW}: ${leg} declares variant "${entry?.variant}" which is not a release SKU ` +
            `(${VARIANT_NAMES.join(', ')})`,
        );
        continue;
      }
      // The suffix names the artifacts and the SKU names the manifest; they are
      // edited on different lines, and drift produces artifacts no manifest looks for.
      const expected = artifactSuffixForVariant(entry.variant);
      if (entry.variant_suffix !== expected) {
        issues.push(
          `${WORKFLOW}: ${leg} variant_suffix is "${entry.variant_suffix}" but SKU ` +
            `"${entry.variant}" produces "${expected}" artifacts`,
        );
      }
    }

    const unknownLabels = matrix
      .map((entry) => entry?.os_label)
      .filter((label) => platformKeyForOsLabel(label) === null);
    for (const label of unknownLabels) {
      issues.push(
        `${WORKFLOW}: os_label "${label}" maps to no updater platform key, so platform ` +
          `coverage cannot be verified`,
      );
    }
    for (const variant of VARIANT_NAMES) {
      for (const platform of platformsForVariant(variant)) {
        const declared = matrix.some(
          (entry) =>
            entry?.variant === variant && platformKeyForOsLabel(entry?.os_label) === platform,
        );
        if (!declared) {
          issues.push(
            `${WORKFLOW}: SKU "${variant}" publishes ${manifestNameForVariant(variant)} with a ` +
              `${platform} entry, but no build leg produces that artifact`,
          );
        }
      }
    }
  }

  // ── 2 & 3. `tauri.conf.json` is the base every SKU inherits: the build script
  // overrides `endpoints` only, so the default endpoint must stay Basic's and the
  // pubkey must stay present or no SKU can verify a signature.
  let conf = null;
  if (!existsSync(confPath)) {
    issues.push(`${TAURI_CONF} is missing — the updater base config cannot be verified`);
  } else {
    try {
      conf = JSON.parse(readFileSync(confPath, 'utf-8'));
    } catch (error) {
      issues.push(`${TAURI_CONF} is not valid JSON: ${error.message}`);
    }
  }

  if (conf) {
    const updater = conf?.plugins?.updater;
    const endpoints = updater?.endpoints;
    const basicManifestUrl = manifestUrlForVariant('basic');
    if (!Array.isArray(endpoints) || endpoints.length === 0) {
      issues.push(
        `${TAURI_CONF}: plugins.updater.endpoints is missing — every SKU would fall back to ` +
          `whatever the plugin defaults to`,
      );
    } else if (!endpoints.includes(basicManifestUrl)) {
      issues.push(
        `${TAURI_CONF}: plugins.updater.endpoints must include ${basicManifestUrl} (Basic's ` +
          `manifest). Basic receives no override at build time, so this array *is* its channel: ` +
          `got ${JSON.stringify(endpoints)}`,
      );
    }
    if (typeof updater?.pubkey !== 'string' || updater.pubkey.trim() === '') {
      issues.push(
        `${TAURI_CONF}: plugins.updater.pubkey is missing. scripts/ci-tauri-build.mjs overrides ` +
          `plugins.updater.endpoints only, so this is the sole pubkey for every SKU — without it ` +
          `signature verification fails on all of them`,
      );
    }
  }

  // ── 4. Packaging templates are copied into Homebrew / WinGet by hand, so a
  // stale artifact name there is only noticed when an install 404s.
  for (const [rel, expectedShape] of Object.entries(PACKAGING_TEMPLATES)) {
    const abs = resolve(root, rel);
    if (!existsSync(abs)) {
      issues.push(`${rel} is listed as a packaging template but does not exist`);
      continue;
    }
    const text = readFileSync(abs, 'utf-8');
    for (const variant of VARIANT_NAMES) {
      if (variant === 'basic') continue;
      // `-all.` / `-akulaku.` / `-all-` — any position, since the suffix sits
      // before the extension for installers and mid-name for the portable zip.
      if (text.includes(`-${variant}.`) || text.includes(`-${variant}-`)) {
        issues.push(
          `${rel}: references the "${variant}" SKU, but this channel ships Basic only`,
        );
      }
    }
    if (/DataZen-[\d.]+-[a-z0-9]+-[a-z0-9]+-nsis\./.test(text)) {
      issues.push(
        `${rel}: references a "-nsis" artifact name. canonical_name() in ${WORKFLOW} appends ` +
          `only the file extension (DataZen-{Version}-{Platform}-{Arch}.exe), so that URL 404s`,
      );
    }
    if (!expectedShape.test(text)) {
      issues.push(
        `${rel}: does not reference a Basic artifact matching ${expectedShape} — expected ` +
          `DataZen-{Version}-{macos-arm64.dmg|macos-x64.dmg|windows-x64.exe}`,
      );
    }
  }

  // ── 5. The manifest job must iterate every SKU and take manifest names from
  // the source of truth rather than a second list that can drift.
  const updaterJob = workflow?.jobs?.['release-updater-json'];
  if (workflow && !updaterJob) {
    issues.push(`${WORKFLOW}: job release-updater-json is missing — no SKU would publish a manifest`);
  } else if (updaterJob) {
    const run = JSON.stringify(updaterJob.steps ?? []);
    const loop = `for VARIANT in ${VARIANT_NAMES.join(' ')}`;
    if (!run.includes(loop)) {
      issues.push(`${WORKFLOW}: release-updater-json must publish every SKU ("${loop}")`);
    }
    if (!run.includes('manifestNameForVariant')) {
      issues.push(
        `${WORKFLOW}: release-updater-json must resolve manifest names via ` +
          `manifestNameForVariant() in scripts/release-variants.mjs instead of a hardcoded list`,
      );
    }
  }

  return issues;
}

/**
 * @param {string} root
 * @param {(message: string) => void} log
 * @returns {number} process exit code
 */
export function checkReleaseVariants(root = ROOT, log = console.log) {
  let issues;
  try {
    issues = collectReleaseVariantIssues(root);
  } catch (error) {
    log(`[check-release-variants] could not read the release configuration: ${error.message}`);
    return 1;
  }

  if (issues.length === 0) {
    log(
      `[check-release-variants] ok (${VARIANT_NAMES.length} SKUs: ` +
        `${VARIANT_NAMES.join('/')}, ${Object.keys(PACKAGING_TEMPLATES).length} packaging templates)`,
    );
    return 0;
  }

  log(`[check-release-variants] ${issues.length} problem(s):`);
  for (const issue of issues) {
    log(`  ✗ ${issue}`);
  }
  return 1;
}

if (isDirectInvocation(import.meta.url)) {
  process.exit(checkReleaseVariants());
}
