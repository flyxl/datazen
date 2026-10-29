/**
 * The shared updater surface (form-split F6 prep).
 *
 * Channel/variant/manifest logic is pure build-constant reasoning and lives
 * here; the plugin-updater *flows* (check / download / install / relaunch)
 * are app-owned (`platform/desktop/updater.ts`) and are reached through the
 * `HostSettingsPort` methods added by the F6 prep — the F2 re-export shim to
 * the desktop zone is gone, so this module compiles inside a host-free
 * `packages/frontend`.
 */

import { DATAZEN_UPDATER_CHANNEL, DATAZEN_VARIANT } from '../extensions/generated';
import { IS_DESKTOP_BUILD } from '../platform/env';
import { requireHostSettings } from '../platform/contracts';
import type { UpdateCheckResult, UpdateProgress } from '../platform/contracts';

export type { UpdateCheckResult, UpdateProgress } from '../platform/contracts';

/**
 * How this build receives updates.
 *
 * - `auto` — this SKU publishes signed updater artifacts plus its own manifest,
 *   so the in-app updater is safe to run.
 * - `manual` — a desktop build whose SKU publishes no channel; the user
 *   downloads the installer for that SKU from GitHub Releases.
 * - `none` — a form without an updater (browser dev / web), so there is
 *   nothing to show.
 */
export type UpdateChannel = 'auto' | 'manual' | 'none';

// ── Channel / variant (pure build-constant reasoning) ────────────────────────

/**
 * Which update path this build must use.
 *
 * Why SKUs are involved at all: `tauri.conf.json` compiles one updater endpoint
 * into every build, and the Tauri updater picks a manifest entry by *platform*
 * alone — it has no notion of SKU. Before the SKU was threaded through, an
 * `all` or `akulaku` install read Basic's `latest.json` and was silently
 * replaced by a Basic build, losing every driver Basic does not ship. Now each
 * SKU is built against its own manifest (see `scripts/ci-tauri-build.mjs`), and
 * a SKU with no published channel (`custom` / private builds) never
 * self-updates at all — it points at GitHub Releases instead.
 */
export function getUpdateChannel(): UpdateChannel {
  // The original branch, byte-equivalent: a browser-dev run of the desktop
  // bundle folds to 'none' at compile time (ruling #5 — the constant lives in
  // the shared `platform/env`, pure `import.meta.env`, no host runtime).
  if (!IS_DESKTOP_BUILD) return 'none';
  return hasUpdaterChannel() ? 'auto' : 'manual';
}

/** Release SKU this build was produced as ('basic' | 'all' | 'akulaku' | 'custom'). */
export function currentVariant(): string {
  return DATAZEN_VARIANT;
}

/**
 * Whether this SKU publishes signed updater artifacts and its own manifest.
 *
 * Computed at build time by `scripts/resolve-drivers.mjs` from
 * `scripts/release-variants.mjs` (the single source of truth for SKUs), so the
 * frontend gate cannot drift from what CI actually publishes.
 */
export function hasUpdaterChannel(): boolean {
  return DATAZEN_UPDATER_CHANNEL;
}

/** Returns false when the in-app updater must not run. */
export function isUpdaterSupported(): boolean {
  return getUpdateChannel() === 'auto';
}

// ── Manifest defence (pure functions, unchanged from the desktop original) ───

/** Normalize a SKU name so `all` and `-all` compare equal. */
function normalizeSku(value: string): string {
  return value.trim().toLowerCase().replace(/^-+/, '');
}

/**
 * The manifest's top-level `variant`, as written by
 * `scripts/generate-updater-latest-json.mjs`. Missing, non-string or blank all
 * collapse to `null`.
 */
export function readManifestVariant(rawJson: unknown): string | null {
  if (typeof rawJson !== 'object' || rawJson === null) return null;
  const value = (rawJson as Record<string, unknown>).variant;
  if (typeof value !== 'string') return null;
  const trimmed = value.trim();
  return trimmed === '' ? null : trimmed;
}

/**
 * Whether this manifest belongs to the build that is reading it.
 *
 * The endpoint is a compile-time constant, so drift in the build scripts or in
 * `tauri.conf.json` lets a variant read another SKU's manifest — and that update
 * replaces it with a build missing drivers. The plugin hands us the manifest
 * verbatim (`rawJson` is the parsed response body; tauri-plugin-updater stores
 * `res.json()` as-is), so the SKU can be re-checked here as a second line of
 * defence that does not depend on the endpoint having been configured right.
 *
 * A manifest with no `variant` splits two ways. Basic keeps the manifest name
 * from earlier releases, so manifests published before this field existed are
 * still legitimate — rejecting them would strand every installed Basic build.
 * Variant manifests only came into existence with per-SKU channels, so a missing
 * field there can only mean the manifest belongs to someone else.
 */
export function manifestBelongsToBuild(
  rawJson: unknown,
  variant: string = currentVariant(),
): boolean {
  const found = readManifestVariant(rawJson);
  if (found === null) return normalizeSku(variant) === 'basic';
  return normalizeSku(found) === normalizeSku(variant);
}

// ── Port accessors (the app-owned flows) ─────────────────────────────────────

/** Check for an update through the host's updater (app binding). */
export function checkForUpdates(): Promise<UpdateCheckResult> {
  return requireHostSettings().checkForUpdates();
}

/** Download and install the pending update through the host's updater. */
export function downloadAndInstallUpdate(
  onProgress?: (progress: UpdateProgress) => void,
): Promise<UpdateCheckResult> {
  return requireHostSettings().downloadAndInstallUpdate(onProgress);
}

/**
 * Silent startup check; installs when an update is available and setting is on.
 *
 * Kept shared (not app-only) because its reasoning — channel gating and the
 * diagnosable skip log — is form-agnostic; only the install half delegates to
 * the port. `main.tsx` keeps calling the desktop original directly.
 */
export async function maybeCheckOnStartup(enabled: boolean): Promise<void> {
  const channel = getUpdateChannel();
  if (!enabled || channel !== 'auto') {
    // Diagnosable on purpose: "why did this build not check for updates?" is
    // answered by the channel, and the SKU says which manifest it would read.
    console.info(
      `[updater] startup check skipped: setting=${enabled} channel=${channel} variant=${currentVariant()}`,
    );
    return;
  }

  const result = await downloadAndInstallUpdate();
  if (result.status === 'error') {
    console.warn('[updater] startup check failed:', result.message);
  }
}
