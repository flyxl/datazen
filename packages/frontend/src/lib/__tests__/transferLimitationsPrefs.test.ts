import { beforeEach, describe, expect, it } from 'vitest';
import {
  clearTransferLimitationsDismissed,
  isTransferLimitationsDismissed,
  setTransferLimitationsDismissed,
} from '../transferLimitationsPrefs';

describe('transferLimitationsPrefs', () => {
  beforeEach(() => {
    localStorage.clear();
  });

  it('returns false when preference is unset', () => {
    expect(isTransferLimitationsDismissed()).toBe(false);
  });

  it('returns true after setTransferLimitationsDismissed', () => {
    setTransferLimitationsDismissed();
    expect(isTransferLimitationsDismissed()).toBe(true);
    expect(localStorage.getItem('datazen:transfer-limitations-dismissed')).toBe('1');
  });

  it('returns false after clearTransferLimitationsDismissed', () => {
    setTransferLimitationsDismissed();
    clearTransferLimitationsDismissed();
    expect(isTransferLimitationsDismissed()).toBe(false);
    expect(localStorage.getItem('datazen:transfer-limitations-dismissed')).toBeNull();
  });
});
