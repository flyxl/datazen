import { describe, expect, it } from 'vitest';
import { getFixture } from '../fixtures';
import {
  bodyContainsAll,
  bodyContainsNone,
  describeMatrixTitle,
  F2_CORE_JOURNEYS,
  ALL_CONTRACT_JOURNEYS,
  journeysToRun,
  paginationRangeVisible,
  planJourneys,
} from '../journeys/plan';

describe('planJourneys (F2 core)', () => {
  it('runs DATA/FILTER/QUERY for postgres and mysql', () => {
    for (const id of ['postgres', 'mysql'] as const) {
      const plan = planJourneys(getFixture(id), F2_CORE_JOURNEYS);
      expect(plan.every((p) => p.status === 'run')).toBe(true);
      expect(journeysToRun(plan)).toEqual([...F2_CORE_JOURNEYS]);
    }
  });

  it('runs DATA/FILTER/QUERY for sqlite (OBJ not in F2 core)', () => {
    const plan = planJourneys(getFixture('sqlite'), F2_CORE_JOURNEYS);
    expect(journeysToRun(plan)).toEqual(['HC-DATA', 'HC-FILTER', 'HC-QUERY']);
  });

  it('skips all F2 core journeys for redis-like mode', () => {
    const base = getFixture('postgres');
    const redisLike = {
      ...base,
      id: 'postgres' as const,
      capabilities: { ...base.capabilities, connectionMode: 'redis' as const },
    };
    const plan = planJourneys(redisLike, F2_CORE_JOURNEYS);
    expect(plan.every((p) => p.status === 'skip')).toBe(true);
    expect(journeysToRun(plan)).toEqual([]);
    expect(plan[0].reason).toMatch(/redis/);
  });

  it('builds a stable matrix describe title', () => {
    expect(describeMatrixTitle(getFixture('mysql'))).toBe('Host contract @ mysql (E2E-MySQL)');
  });
});

describe('planJourneys (F3 full contract)', () => {
  it('runs all journeys on postgres/mysql', () => {
    for (const id of ['postgres', 'mysql'] as const) {
      const plan = planJourneys(getFixture(id), ALL_CONTRACT_JOURNEYS);
      expect(journeysToRun(plan)).toEqual([...ALL_CONTRACT_JOURNEYS]);
    }
  });

  it('skips HC-OBJ on sqlite but runs the rest', () => {
    const plan = planJourneys(getFixture('sqlite'), ALL_CONTRACT_JOURNEYS);
    expect(journeysToRun(plan)).not.toContain('HC-OBJ');
    expect(journeysToRun(plan)).toContain('HC-DATA');
    expect(journeysToRun(plan)).toContain('HC-EXPLAIN');
    const obj = plan.find((p) => p.id === 'HC-OBJ');
    expect(obj?.status).toBe('skip');
    expect(obj?.reason).toMatch(/hasObjects/);
  });

  it('defaults planJourneys to the full contract list', () => {
    const plan = planJourneys(getFixture('postgres'));
    expect(plan.map((p) => p.id)).toEqual([...ALL_CONTRACT_JOURNEYS]);
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
