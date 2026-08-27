import { describe, expect, it } from 'vitest';
import {
  formatConnectionImportSuccess,
  translateConnectionShareError,
} from '../connectionShareError';

const t = (key: string, params?: Record<string, string | number>) => {
  if (!params) return key;
  return Object.entries(params).reduce((text, [k, v]) => text.replace(`{${k}}`, String(v)), key);
};

describe('connectionShareError', () => {
  it('maps known import validation errors to i18n keys', () => {
    expect(
      translateConnectionShareError(
        'Password is required for encrypted connection import',
        t,
        'fallback',
      ),
    ).toBe('connShare.error.encryptedImportPasswordRequired');

    expect(
      translateConnectionShareError(
        'DataZen decryption failed: wrong password or corrupt file',
        t,
        'fallback',
      ),
    ).toBe('connShare.error.decryptionFailed');

    expect(
      translateConnectionShareError('Unrecognized connection import format', t, 'fallback'),
    ).toBe('connShare.error.unrecognizedFormat');
  });

  it('returns fallback for empty messages', () => {
    expect(translateConnectionShareError('   ', t, 'fallback')).toBe('fallback');
  });

  it('passes through unknown messages', () => {
    expect(translateConnectionShareError('Some new backend error', t, 'fallback')).toBe(
      'Some new backend error',
    );
  });

  it('formats import success with all counters', () => {
    expect(
      formatConnectionImportSuccess(
        { imported: 2, overwritten: 1, groupsAdded: 3, skipped: [] },
        t,
      ),
    ).toBe('connShare.importSuccess');

    expect(
      formatConnectionImportSuccess(
        { imported: 2, overwritten: 1, groupsAdded: 3, skipped: ['oracle'] },
        t,
      ),
    ).toBe('connShare.importSuccessWithSkipped');
  });
});
