import { beforeEach, describe, expect, it } from 'vitest';
import { createDismissPrefs } from '../createDismissPrefs';

describe('createDismissPrefs', () => {
  const KEY = 'datazen:test-dismiss-prefs';
  const { isDismissed, setDismissed, clearDismissed } = createDismissPrefs(KEY);

  beforeEach(() => {
    localStorage.clear();
  });

  it('returns false when preference is unset', () => {
    expect(isDismissed()).toBe(false);
  });

  it('returns true after setDismissed', () => {
    setDismissed();
    expect(isDismissed()).toBe(true);
    expect(localStorage.getItem(KEY)).toBe('1');
  });

  it('returns false after clearDismissed', () => {
    setDismissed();
    clearDismissed();
    expect(isDismissed()).toBe(false);
    expect(localStorage.getItem(KEY)).toBeNull();
  });
});
