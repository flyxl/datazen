const STORAGE_KEY = 'datazen:schema-diff-limitations-dismissed';

export function isSchemaDiffLimitationsDismissed(): boolean {
  try {
    return localStorage.getItem(STORAGE_KEY) === '1';
  } catch {
    return false;
  }
}

export function setSchemaDiffLimitationsDismissed(): void {
  try {
    localStorage.setItem(STORAGE_KEY, '1');
  } catch {
    // localStorage may be unavailable in tests
  }
}

export function clearSchemaDiffLimitationsDismissed(): void {
  try {
    localStorage.removeItem(STORAGE_KEY);
  } catch {
    // ignore
  }
}
