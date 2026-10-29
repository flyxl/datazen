/**
 * Desktop updater flows (form-split F6 prep, slimmed from the F2 original).
 *
 * The channel/variant/manifest reasoning moved back to the shared
 * `src/lib/updater.ts` (pure build-constant logic, host-free); what remains
 * here is the plugin-updater machinery: check, download+install, relaunch and
 * the silent startup check. Shared code reaches these through the
 * `HostSettingsPort` updater methods added by the F6 prep; `main.tsx` (the app
 * entry) keeps importing this module directly. B8: updater is desktop-only —
 * the app zone plus `capabilities.updater` (F5) decide visibility.
 */

import { check } from '@tauri-apps/plugin-updater';
import { relaunch } from '@tauri-apps/plugin-process';
import {
  currentVariant,
  getUpdateChannel,
  isUpdaterSupported,
  manifestBelongsToBuild,
  readManifestVariant,
} from '@datazen/frontend/src/lib/updater';
import type { UpdateCheckResult, UpdateProgress } from '@datazen/frontend/src/platform/contracts';

export type { UpdateCheckResult, UpdateProgress } from '@datazen/frontend/src/platform/contracts';

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
