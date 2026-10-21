/**
 * The host side of the shared-layer completeness contract (08 §3.3.1).
 *
 * The claim under test: `isCompleteFilter` (host) and `filter_is_complete`
 * (shared layer, `packages/feature-core/.../filter_sql`) judge the **same**
 * inputs the **same** way. A divergence means the host lets a condition
 * through that the back end then refuses (`grid.filter.incomplete`), or the
 * other way round: the host blocks a condition the back end would have
 * rendered.
 *
 * Both sides read the same canonical table —
 * `packages/feature-core/tests/fixtures/filter_completeness_cases.json` — so
 * the Rust integration test
 * (`packages/feature-core/tests/filter_empty_bounds_recheck.rs`) and this file
 * judge identical inputs against identical expectations. A disagreement on any
 * row turns one of the two sides red.
 */
import { describe, expect, it } from 'vitest';
import { existsSync, readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { isCompleteFilter } from '../filterUtils';
import type { FilterCondition, FilterOperator, Value } from '../../../types';

type Case = {
  id: string;
  operator: FilterOperator;
  value_kind: 'json_array' | 'string';
  value: unknown;
  expected_complete: boolean;
};

const TABLE_RELATIVE = 'packages/feature-core/tests/fixtures/filter_completeness_cases.json';

/** Walk up from the test cwd until the workspace root (holds the table) is found. */
function findRepoRoot(): string {
  let dir = process.cwd();
  for (;;) {
    if (existsSync(resolve(dir, TABLE_RELATIVE))) return dir;
    const parent = resolve(dir, '..');
    if (parent === dir) throw new Error('repo root not found from ' + process.cwd());
    dir = parent;
  }
}

const CASES: Case[] = (
  JSON.parse(readFileSync(resolve(findRepoRoot(), TABLE_RELATIVE), 'utf8')) as { cases: Case[] }
).cases;

function condition(case_: Case): FilterCondition {
  const value: Value = case_.value_kind === 'json_array' ? (case_.value as Value) : (case_.value as string);
  return { column: 'score', operator: case_.operator, value };
}

describe('the host and the shared layer judge a condition the same way', () => {
  it('reads the canonical table', () => {
    expect(CASES.length).toBeGreaterThanOrEqual(30);
    // Every shape the rule talks about must be in the table.
    const ids = CASES.map((case_) => case_.id);
    for (const required of [
      'between-array-empty-upper',
      'between-array-empty-lower',
      'between-string-empty-upper',
      'between-array-blank-upper',
      'between-array-null-lower',
      'between-array-null-upper',
      'notbetween-array-empty-upper',
      'between-array-zero-lower',
      'between-array-false-lower',
    ]) {
      expect(ids).toContain(required);
    }
  });

  for (const case_ of CASES) {
    it(`${case_.id}: complete = ${case_.expected_complete}`, () => {
      expect(isCompleteFilter(condition(case_))).toBe(case_.expected_complete);
    });
  }
});
