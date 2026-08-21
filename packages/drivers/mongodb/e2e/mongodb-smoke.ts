/**
 * MongoDB driver E2E smoke (optional).
 *
 * Skips unless E2E_MONGODB_URI is set. See README.md in this directory.
 */
import { expect } from '@wdio/globals';

function shouldSkip(): boolean {
  if (process.env.E2E_SKIP_MONGODB === '1') {
    console.warn('⏩ Skipping MongoDB E2E: E2E_SKIP_MONGODB=1');
    return true;
  }
  if (!process.env.E2E_MONGODB_URI?.trim()) {
    console.warn(
      '⏩ Skipping MongoDB E2E: set E2E_MONGODB_URI (see packages/drivers/mongodb/e2e/README.md)',
    );
    return true;
  }
  return false;
}

describe('MongoDB driver smoke', () => {
  before(function () {
    if (shouldSkip()) this.skip();
  });

  it('placeholder — extend when CI fixture exists', () => {
    expect(process.env.E2E_MONGODB_URI).toBeTruthy();
  });
});
