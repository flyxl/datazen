/**
 * Desktop updater (form-split F2, absorbed from `src/lib/updater.ts`).
 * B8: updater is desktop-only — no shared port; the app zone plus
 * `capabilities.updater` (F5) decide visibility.
 */

import { check } from '@tauri-apps/plugin-updater';
import { relaunch } from '@tauri-apps/plugin-process';
import { DATAZEN_UPDATER_CHANNEL, DATAZEN_VARIANT } from '../../extensions/generated';
import { IS_DESKTOP_BUILD } from './env';

export type UpdateProgress =
  | { phase: 'checking' }
  | { phase: 'downloading'; downloaded: number; total: number | null }
  | { phase: 'installing' }
  | { phase: 'done'; version: string }
  | { phase: 'idle' };

export type UpdateCheckResult =
  | { status: 'upToDate' }
  | { status: 'available'; version: string }
  | { status: 'installed'; version: string }
  | { status: 'error'; message: string };

/**
 * How this build receives updates.
 *
 * - `auto` — this SKU publishes signed updater artifacts plus its own manifest,
 *   so the in-app updater is safe to run.
 * - `manual` — a desktop build whose SKU publishes no channel; the user
 *   downloads the installer for that SKU from GitHub Releases.
 * - `none` — not a desktop build (browser dev), so there is nothing to show.
 */
export type UpdateChannel = 'auto' | 'manual' | 'none';

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
  // F2 interim: browser dev of the desktop build keeps the 'none' branch via the
  // compile-time constant; the branch is deleted for good in F6 when the app
  // package owns this file and no other form imports it.
  if (!IS_DESKTOP_BUILD) return 'none';
  return hasUpdaterChannel() ? 'auto' : 'manual';
}

/** Returns false when the in-app updater must not run. */
export function isUpdaterSupported(): boolean {
  return getUpdateChannel() === 'auto';
}

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

/** Shown when a manifest fails the SKU check. Healthy builds never reach it. */
const MANIFEST_MISMATCH_MESSAGE =
  'This update belongs to a different DataZen build and was skipped — install the matching build from GitHub Releases';

/**
 * Last check before anything is downloaded or installed.
 *
 * Returns the reason to refuse, or `null` to proceed. Both `checkForUpdates` and
 * `downloadAndInstallUpdate` route through it, so neither path can be left
 * unguarded.
 */
function refuseForeignManifest(rawJson: unknown): string | null {
  if (manifestBelongsToBuild(rawJson)) return null;
  console.warn(
    `[updater] refusing update: manifest variant=${readManifestVariant(rawJson) ?? '<missing>'} build variant=${currentVariant()}`,
  );
  return MANIFEST_MISMATCH_MESSAGE;
}

export async function checkForUpdates(): Promise<UpdateCheckResult> {
  if (!isUpdaterSupported()) {
    return { status: 'error', message: 'Updater is not available in this build' };
  }

  try {
    const update = await check();
    if (!update) {
      return { status: 'upToDate' };
    }
    const refusal = refuseForeignManifest(update.rawJson);
    if (refusal) {
      return { status: 'error', message: refusal };
    }
    return { status: 'available', version: update.version };
  } catch (e) {
    return {
      status: 'error',
      message: e instanceof Error ? e.message : String(e),
    };
  }
}

export async function downloadAndInstallUpdate(
  onProgress?: (progress: UpdateProgress) => void,
): Promise<UpdateCheckResult> {
  if (!isUpdaterSupported()) {
    return { status: 'error', message: 'Updater is not available in this build' };
  }

  onProgress?.({ phase: 'checking' });

  try {
    const update = await check();
    if (!update) {
      onProgress?.({ phase: 'idle' });
      return { status: 'upToDate' };
    }

    const refusal = refuseForeignManifest(update.rawJson);
    if (refusal) {
      onProgress?.({ phase: 'idle' });
      return { status: 'error', message: refusal };
    }

    let downloaded = 0;
    let total: number | null = null;

    await update.downloadAndInstall((event) => {
      switch (event.event) {
        case 'Started':
          total = event.data.contentLength ?? null;
          downloaded = 0;
          onProgress?.({ phase: 'downloading', downloaded, total });
          break;
        case 'Progress':
          downloaded += event.data.chunkLength;
          onProgress?.({ phase: 'downloading', downloaded, total });
          break;
        case 'Finished':
          onProgress?.({ phase: 'installing' });
          break;
      }
    });

    onProgress?.({ phase: 'done', version: update.version });
    await relaunch();
    return { status: 'installed', version: update.version };
  } catch (e) {
    onProgress?.({ phase: 'idle' });
    return {
      status: 'error',
      message: e instanceof Error ? e.message : String(e),
    };
  }
}

/** Silent startup check; installs when an update is available and setting is on. */
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
