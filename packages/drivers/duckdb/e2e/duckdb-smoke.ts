/**
 * DuckDB driver E2E smoke (optional).
 *
 * Skips unless E2E_DUCKDB_PATH is set. See README.md in this directory.
 */
import { expect } from '@wdio/globals';

function shouldSkip(): boolean {
  if (process.env.E2E_SKIP_DUCKDB === '1') {
    console.warn('⏩ Skipping DuckDB E2E: E2E_SKIP_DUCKDB=1');
    return true;
  }
  if (!process.env.E2E_DUCKDB_PATH?.trim()) {
    console.warn(
      '⏩ Skipping DuckDB E2E: set E2E_DUCKDB_PATH (see packages/drivers/duckdb/e2e/README.md)',
    );
    return true;
  }
  return false;
}

describe('DuckDB driver smoke', () => {
  before(function () {
    if (shouldSkip()) this.skip();
  });

  it('placeholder — extend when CI fixture exists', () => {
    expect(process.env.E2E_DUCKDB_PATH).toBeTruthy();
  });
});
