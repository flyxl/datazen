const STORAGE_KEY = 'datazen:transfer-limitations-dismissed';

export function isTransferLimitationsDismissed(): boolean {
  try {
    return localStorage.getItem(STORAGE_KEY) === '1';
  } catch {
    return false;
  }
}

export function setTransferLimitationsDismissed(): void {
  try {
    localStorage.setItem(STORAGE_KEY, '1');
  } catch {
    // localStorage may be unavailable in tests
  }
}

export function clearTransferLimitationsDismissed(): void {
  try {
    localStorage.removeItem(STORAGE_KEY);
  } catch {
    // ignore
  }
}
