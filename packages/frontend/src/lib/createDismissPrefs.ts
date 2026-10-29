export interface DismissPrefs {
  isDismissed: () => boolean;
  setDismissed: () => void;
  clearDismissed: () => void;
}

export function createDismissPrefs(storageKey: string): DismissPrefs {
  return {
    isDismissed(): boolean {
      try {
        return localStorage.getItem(storageKey) === '1';
      } catch {
        return false;
      }
    },

    setDismissed(): void {
      try {
        localStorage.setItem(storageKey, '1');
      } catch {
        // localStorage may be unavailable in tests
      }
    },

    clearDismissed(): void {
      try {
        localStorage.removeItem(storageKey);
      } catch {
        // ignore
      }
    },
  };
}
