import { check } from '@tauri-apps/plugin-updater';
import { relaunch } from '@tauri-apps/plugin-process';

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

function isTauriDesktop(): boolean {
  return '__TAURI_INTERNALS__' in globalThis;
}

/** Returns false when updater plugin is unavailable (e.g. dev build). */
export function isUpdaterSupported(): boolean {
  return isTauriDesktop();
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
  if (!enabled || !isUpdaterSupported()) return;

  const result = await downloadAndInstallUpdate();
  if (result.status === 'error') {
    console.warn('[updater] startup check failed:', result.message);
  }
}
