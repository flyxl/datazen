import { afterEach, describe, expect, it, vi } from 'vitest';
import type { TranslationKey } from '../../locales/zh-CN';
import en from '../../locales/en';
import { enCopy } from '../enCopy';

/**
 * `enCopy()` is the single runtime choke point behind 原则六 第 3 类锚点
 * (dictionary read-back, redis-assert-policy BUG-002). These cases exist so the
 * helper cannot be quietly reverted to a bare `en[key]`: a miss must throw,
 * because `getByRole('button', { name: undefined })` is how a guard test turns
 * into an always-passing query.
 */
describe('enCopy', () => {
  afterEach(() => {
    // Only `vi.mock` / `vi.unmock` calls are hoisted; `resetModules` is the
    // runtime way to drop the mocked dictionary between cases here.
    vi.resetModules();
  });

  it('returns the wording currently shipped in the host dictionary', () => {
    // Keys the three rewritten host suites locate by, so a rename that only
    // updates the dictionary still resolves here instead of going red.
    for (const key of [
      'common.error',
      'common.close',
      'common.retry',
      'menu.appName',
      'menu.file',
      'common.importConnections',
    ] as const) {
      const value = enCopy(key);
      expect(value).toBe(en[key]);
      expect(value.trim().length).toBeGreaterThan(0);
    }
  });

  it('throws instead of degrading the locator when the key is gone', () => {
    // A renamed or deleted entry is exactly what a static `en[key]` would hand
    // back as `undefined`. Cast: the point is the runtime contract, and
    // `__tests__` are outside `npx tsc --noEmit` (see the helper's own note).
    const renamed = 'common.errorZZZ' as TranslationKey;
    expect(() => enCopy(renamed)).toThrow(/common\.errorZZZ/);
  });

  it('throws when a dictionary value is present but blank', async () => {
    // Defensive branch: parity checks keep every shipped value non-empty today,
    // so reach it through a mocked dictionary rather than by editing a locale.
    vi.resetModules();
    vi.doMock('../../locales/en', () => ({ default: { 'common.close': '   ' } }));
    const { enCopy: fresh } = await import('../enCopy');
    expect(() => fresh('common.close')).toThrow(/blank/);
    vi.doUnmock('../../locales/en');
  });
});
