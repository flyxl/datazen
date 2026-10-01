/** @vitest-environment node */
import { describe, expect, it } from 'vitest';
import { TEST_COUNT_CLAIMS, extractTestCountClaims, findTestCountProblems } from '../check-test-count.mjs';

/**
 * Unit tests for the pure half of scripts/check-test-count.mjs.
 *
 * `collectTestCount` is deliberately not exercised here: it shells out to the
 * real collector and is what `pnpm test:unit-count` runs in CI. Asserting it
 * from inside vitest would nest a second full collection inside the suite,
 * which is the cost the script's own header documents avoiding.
 */

/**
 * Build one source per claim, each stating `count` in the words its anchor
 * requires. Hand-written per file rather than generated from the pattern, so
 * a re-anchored pattern fails these fixtures instead of silently passing.
 */
function sourcesWith(counts: Record<string, string>): Record<string, string> {
  const byFile: Record<string, string> = {
    'vitest.config.ts': `// Measured per-test wall time over all ${counts['vitest.config.ts']} Host tests\n`,
    '.github/workflows/ci.yml': `        # Measured per-test wall time, all ${
          counts['.github/workflows/ci.yml']
        }\n        # tests, --reporter=json)\n`,
    'docs/development/ci-test-matrix.md': `对全部 ${
      counts['docs/development/ci-test-matrix.md']
    } 个 Host 用例逐条统计。\n`,
  };
  for (const { file } of TEST_COUNT_CLAIMS) {
    if (typeof byFile[file] !== 'string') throw new Error(`fixture does not cover ${file}`);
  }
  return byFile;
}

const ALL_AGREEING = { 'vitest.config.ts': '5695', '.github/workflows/ci.yml': '5695', 'docs/development/ci-test-matrix.md': '5695' };

describe('[tester] check-test-count claim extraction', () => {
  it('test_tester_extracts_one_count_per_file', () => {
    const claims = extractTestCountClaims(sourcesWith(ALL_AGREEING));
    expect(claims.map((c) => c.file)).toEqual(TEST_COUNT_CLAIMS.map((c) => c.file));
    expect(claims.map((c) => c.count)).toEqual([5695, 5695, 5695]);
  });

  it('test_tester_throws_when_a_count_is_missing', () => {
    // The failure that matters: a reworded file must fail loudly, because a
    // claim the guard cannot see is a claim it is silently not checking.
    const sources = sourcesWith(ALL_AGREEING);
    sources['docs/development/ci-test-matrix.md'] = '本文不再给出任何测试数量。\n';
    expect(() => extractTestCountClaims(sources)).toThrow(/ci-test-matrix\.md: no Host test count found/);
  });

  it('test_tester_throws_when_a_count_is_ambiguous', () => {
    const sources = sourcesWith(ALL_AGREEING);
    sources['vitest.config.ts'] += sources['vitest.config.ts'];
    expect(() => extractTestCountClaims(sources)).toThrow(/2 Host test counts match/);
  });

  it('test_tester_throws_when_a_count_is_not_a_positive_integer', () => {
    const sources = sourcesWith({ ...ALL_AGREEING, 'vitest.config.ts': '0' });
    expect(() => extractTestCountClaims(sources)).toThrow(/not a positive integer/);
  });
});

describe('[tester] check-test-count problem reporting', () => {
  const claims = (counts: Record<string, string>) => extractTestCountClaims(sourcesWith(counts));

  it('test_tester_passes_when_the_quotes_match_the_collector', () => {
    expect(findTestCountProblems(claims(ALL_AGREEING), 5695)).toEqual([]);
  });

  it('test_tester_catches_three_consistently_wrong_numbers', () => {
    // The case the three hand-written numbers cannot catch on their own, and
    // the whole reason this script exists.
    const wrong = { 'vitest.config.ts': '5700', '.github/workflows/ci.yml': '5700', 'docs/development/ci-test-matrix.md': '5700' };
    expect(new Set(claims(wrong).map((c) => c.count)).size).toBe(1);
    expect(findTestCountProblems(claims(wrong), 5695)).toEqual([
      'vitest.config.ts quotes 5700 Host tests, collector measured 5695',
      '.github/workflows/ci.yml quotes 5700 Host tests, collector measured 5695',
      'docs/development/ci-test-matrix.md quotes 5700 Host tests, collector measured 5695',
    ]);
  });

  it('test_tester_catches_contradictory_numbers', () => {
    // Three mismatches plus the mutual disagreement. All three are also wrong,
    // so the disagreement line is not the only thing carrying the failure.
    const split = { 'vitest.config.ts': '5700', '.github/workflows/ci.yml': '5701', 'docs/development/ci-test-matrix.md': '5702' };
    const problems = findTestCountProblems(claims(split), 5695);
    expect(problems).toHaveLength(4);
    expect(problems.at(-1)).toContain('the quoted counts disagree with each other');
  });

  it('test_tester_catches_a_claim_set_that_lost_a_file', () => {
    expect(findTestCountProblems([{ id: 'ci.yml', file: '.github/workflows/ci.yml', count: 5695 }], 5695)).toEqual([
      'expected 3 quoted counts, got 1',
    ]);
  });
});
