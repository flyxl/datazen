#!/usr/bin/env node
/**
 * release-variants.mjs — single source of truth for release SKUs ("variants").
 *
 * ## Why this exists
 *
 * `apps/desktop/src-tauri/tauri.conf.json` compiles **one** global updater endpoint into
 * every build, and the Tauri updater picks a manifest entry by *platform* only
 * (`darwin-aarch64`, `windows-x86_64`, …). Neither the manifest nor the running
 * app carried any notion of which SKU an artifact was built for, so a user who
 * installed `-all` / `-akulaku` was served the Basic manifest and silently
 * replaced by a Basic build — losing every driver Basic does not ship.
 *
 * The fix has three legs, and all three read their facts from this module:
 *
 *   1. **Runtime gate** — `resolve-drivers.mjs` bakes `DATAZEN_VARIANT` and the
 *      `hasUpdaterChannel()` verdict into `packages/frontend/src/extensions/generated.ts`;
 *      `packages/frontend/src/lib/updater.ts` self-updates only when that verdict is true and
 *      otherwise shows a manual-download card.
 *   2. **Per-SKU endpoint** — `ci-tauri-build.mjs` injects the SKU's own
 *      manifest URL, so a variant never even reads Basic's manifest.
 *   3. **Per-SKU manifest** — `generate-updater-latest-json.mjs` writes one
 *      manifest per SKU from that SKU's own artifacts.
 *
 * A SKU with no entry here (local/dev builds, custom private SKUs) gets
 * `DATAZEN_VARIANT=custom`: no endpoint override (the runtime gate already
 * refuses to self-update) and no manifest to publish.
 *
 * ## Platform keys
 *
 * `platforms` is authoritative: the generator fails a release when a SKU is
 * missing one of its own platforms, because a partial manifest makes that SKU's
 * users silently stop receiving updates. Akulaku has no Linux leg by design.
 */

/** SKU used by local/dev builds: deliberately has no updater channel. */
export const DEFAULT_VARIANT = 'custom';

/** Canonical GitHub repo whose releases host the manifests. */
export const DEFAULT_REPO = 'flyxl/datazen';

/**
 * @typedef {{
 *   manifest: string,
 *   platforms: readonly string[],
 * }} ReleaseVariantConfig
 */

/** @type {Readonly<Record<string, ReleaseVariantConfig>>} */
export const RELEASE_VARIANTS = Object.freeze({
  /**
   * Default release SKU (4 core drivers). Keeps the historical `latest.json`
   * name and its 4 platform keys so already-installed Basic builds — whose
   * endpoint is compiled in — keep working untouched.
   */
  basic: {
    manifest: 'latest.json',
    platforms: ['darwin-aarch64', 'darwin-x86_64', 'windows-x86_64', 'linux-x86_64'],
  },
  /** Full native driver set (all path drivers). */
  all: {
    manifest: 'latest-all.json',
    platforms: ['darwin-aarch64', 'darwin-x86_64', 'windows-x86_64', 'linux-x86_64'],
  },
  /** Custom deployment SKU (core + MongoDB + Kiwi + Superset); Windows/macOS only. */
  akulaku: {
    manifest: 'latest-akulaku.json',
    platforms: ['darwin-aarch64', 'darwin-x86_64', 'windows-x86_64'],
  },
});

/** Release SKU names, in publish order (Basic first so its channel never waits). */
export const VARIANT_NAMES = Object.freeze(Object.keys(RELEASE_VARIANTS));

/**
 * Normalize a raw variant string from CI/env/matrix into a SKU name.
 *
 * Tolerates the `-all` spelling used by `matrix.variant_suffix` (the leading
 * hyphen is part of the *artifact* name, not the SKU name). Unknown or empty
 * values collapse to {@link DEFAULT_VARIANT}, which is the safe direction: an
 * unrecognized SKU loses auto-update rather than being offered someone else's
 * build.
 *
 * @param {string | null | undefined} raw
 * @returns {string}
 */
export function normalizeVariant(raw) {
  if (raw == null) return DEFAULT_VARIANT;
  const value = String(raw).trim().toLowerCase().replace(/^-+/, '');
  return value === '' ? DEFAULT_VARIANT : value;
}

/**
 * @param {string | null | undefined} variant
 * @returns {ReleaseVariantConfig | null}
 */
export function variantConfig(variant) {
  return RELEASE_VARIANTS[normalizeVariant(variant)] ?? null;
}

/**
 * Whether this SKU publishes an updater channel the app may self-update from.
 * This is the same predicate the runtime uses, so a SKU can never be offered a
 * manifest it has no artifacts for.
 *
 * @param {string | null | undefined} variant
 */
export function hasUpdaterChannel(variant) {
  return variantConfig(variant) != null;
}

/**
 * Manifest asset name for a SKU, or `null` when the SKU has no channel.
 * @param {string | null | undefined} variant
 * @returns {string | null}
 */
export function manifestNameForVariant(variant) {
  return variantConfig(variant)?.manifest ?? null;
}

/**
 * Manifest URL GitHub serves for a SKU, or `null` when it has no channel.
 * @param {string | null | undefined} variant
 * @param {string} [repo]
 * @returns {string | null}
 */
export function manifestUrlForVariant(variant, repo = DEFAULT_REPO) {
  const name = manifestNameForVariant(variant);
  return name ? `https://github.com/${repo}/releases/latest/download/${name}` : null;
}

/**
 * Platform keys a SKU must publish. Empty array = no channel.
 * @param {string | null | undefined} variant
 * @returns {readonly string[]}
 */
export function platformsForVariant(variant) {
  return variantConfig(variant)?.platforms ?? [];
}

/**
 * Infix every artifact of this SKU carries, from the canonical naming scheme
 * `DataZen-{Version}-{Platform}-{Arch}[-{Variant}].{ext}`. Basic is `''` so its
 * historical asset names never change.
 *
 * @param {string | null | undefined} variant
 */
export function artifactSuffixForVariant(variant) {
  const name = normalizeVariant(variant);
  return name === 'basic' ? '' : `-${name}`;
}
