import en from '../locales/en';
import type { TranslationKey } from '../locales/zh-CN';

/**
 * 字典回读（expected wording read back out of the en dictionary）的唯一合法入口。
 *
 * Reading the expected wording out of the dictionary is what keeps a test green
 * across a copy change — but a *bare* `en[key]` miss yields `undefined`, and
 * testing-library treats `getByRole('button', { name: undefined })` as "no name
 * constraint at all". The locator then degrades into an always-matching query:
 * the assertion stays green while the contract it used to check is gone, and
 * whether it happens to fail depends on how many sibling elements the page
 * renders, not on the behaviour under test.
 *
 * Neither TypeScript nor the test runner catches the miss: `tsconfig.json`
 * excludes every `__tests__` directory from `npx tsc --noEmit`, and vitest never
 * type-checks. So the check has to happen at run time, in one place:
 *
 * - key absent from the host dictionary (renamed / deleted) → fail;
 * - value blank or whitespace-only → fail;
 * - otherwise return the current copy, still with zero hard-coded English.
 */
export function enCopy(key: TranslationKey): string {
  const value: string | undefined = en[key];
  if (value === undefined) {
    throw new Error(
      `en dictionary miss: "${key}" is not in src/locales/en (a renamed or deleted entry must not silently turn this locator into an unbounded query)`,
    );
  }
  if (value.trim().length === 0) {
    throw new Error(`en dictionary blank: "${key}" resolves to an empty string`);
  }
  return value;
}
