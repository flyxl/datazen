import { describe, expect, it } from 'vitest';
import { getFixture } from '../fixtures';
import {
  bodyContainsAll,
  bodyContainsNone,
  describeMatrixTitle,
  F2_CORE_JOURNEYS,
  journeysToRun,
  paginationRangeVisible,
  planJourneys,
} from '../journeys/plan';

describe('planJourneys (F2 core)', () => {
  it('runs DATA/FILTER/QUERY for postgres and mysql', () => {
    for (const id of ['postgres', 'mysql'] as const) {
      const plan = planJourneys(getFixture(id));
      expect(plan.every((p) => p.status === 'run')).toBe(true);
      expect(journeysToRun(plan)).toEqual([...F2_CORE_JOURNEYS]);
    }
  });

  it('runs DATA/FILTER/QUERY for sqlite (OBJ not in F2 core)', () => {
    const plan = planJourneys(getFixture('sqlite'));
    expect(journeysToRun(plan)).toEqual(['HC-DATA', 'HC-FILTER', 'HC-QUERY']);
  });

  it('skips all F2 core journeys for redis-like mode', () => {
    const base = getFixture('postgres');
    const redisLike = {
      ...base,
      id: 'postgres' as const,
      capabilities: { ...base.capabilities, connectionMode: 'redis' as const },
    };
    const plan = planJourneys(redisLike);
    expect(plan.every((p) => p.status === 'skip')).toBe(true);
    expect(journeysToRun(plan)).toEqual([]);
    expect(plan[0].reason).toMatch(/redis/);
  });

  it('builds a stable matrix describe title', () => {
    expect(describeMatrixTitle(getFixture('mysql'))).toBe(
      'Host contract @ mysql (E2E-MySQL)',
    );
  });
});

describe('body assertion helpers', () => {
  it('bodyContainsAll / None', () => {
    const body = 'alpha beta gamma';
    expect(bodyContainsAll(body, ['alpha', 'beta'])).toBe(true);
    expect(bodyContainsAll(body, ['alpha', 'delta'])).toBe(false);
    expect(bodyContainsNone(body, ['delta', 'epsilon'])).toBe(true);
    expect(bodyContainsNone(body, ['alpha'])).toBe(false);
  });

  it('paginationRangeVisible matches Host chrome', () => {
    expect(paginationRangeVisible('Rows 1-25 / 60')).toBe(true);
    expect(paginationRangeVisible('1-25 / 60')).toBe(true);
    expect(paginationRangeVisible('no range here')).toBe(false);
  });
});
