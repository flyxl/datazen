import { create } from 'zustand';
import {
  checkForUpdates as runUpdateCheck,
  downloadAndInstallUpdate as runUpdateInstall,
  relaunchToApplyUpdate as runRelaunchToApplyUpdate,
} from '../lib/updater';
import type { UpdateProgress } from '../platform/contracts';

const UPDATE_CHECK_INTERVAL_MS = 24 * 60 * 60 * 1000;

interface UpdateReminderState {
  lastCheckAttemptAt: number | null;
  availableVersion: string | null;
  pendingRestartVersion: string | null;
  isChecking: boolean;
  isInstalling: boolean;
  progress: UpdateProgress | null;
  error: string | null;
  checkForUpdates: () => Promise<void>;
  checkForUpdatesIfDue: () => Promise<void>;
  installUpdate: () => Promise<void>;
  restartToApplyUpdate: () => Promise<void>;
}

export const useUpdateReminderStore = create<UpdateReminderState>((set, get) => ({
  lastCheckAttemptAt: null,
  availableVersion: null,
  pendingRestartVersion: null,
  isChecking: false,
  isInstalling: false,
  progress: null,
  error: null,

  async checkForUpdatesIfDue() {
    const lastAttempt = get().lastCheckAttemptAt;
    const now = Date.now();
    // A backward clock adjustment must not defer checks indefinitely.
    if (
      lastAttempt !== null &&
      now >= lastAttempt &&
      now - lastAttempt < UPDATE_CHECK_INTERVAL_MS
    ) {
      return;
    }
    await get().checkForUpdates();
  },

  async checkForUpdates() {
    if (get().isChecking || get().isInstalling) return;
    // Record actual attempts, including failures, before awaiting the host.
    // Busy checks leave the deadline intact so a later wake-up can retry.
    set({ isChecking: true, lastCheckAttemptAt: Date.now() });
    try {
      const result = await runUpdateCheck();
      if (result.status === 'available') {
        set({ availableVersion: result.version, error: null });
      } else if (result.status === 'upToDate') {
        set({ availableVersion: null, error: null });
      } else if (result.status === 'error') {
        console.warn('[updater] scheduled check failed:', result.message);
      }
    } catch (error) {
      console.warn('[updater] scheduled check failed:', error);
    } finally {
      set({ isChecking: false });
    }
  },

  async installUpdate() {
    const version = get().availableVersion;
    if (!version || get().isInstalling) return;
    set({ isInstalling: true, error: null, progress: { phase: 'checking' } });
    try {
      const result = await runUpdateInstall((progress) => set({ progress }));
      if (result.status === 'upToDate') {
        set({ availableVersion: null, progress: null });
      } else if (result.status === 'error') {
        set({ error: result.message, progress: null });
      } else if (result.status === 'installed') {
        // Installed and staged; user chooses when to restart (applies on next
        // restart if deferred).
        set({ availableVersion: null, pendingRestartVersion: result.version, progress: null });
      }
    } catch (error) {
      set({ error: error instanceof Error ? error.message : String(error), progress: null });
    } finally {
      set({ isInstalling: false });
    }
  },

  async restartToApplyUpdate() {
    if (!get().pendingRestartVersion) return;
    try {
      await runRelaunchToApplyUpdate();
    } catch (error) {
      set({ error: error instanceof Error ? error.message : String(error) });
    }
  },
}));
