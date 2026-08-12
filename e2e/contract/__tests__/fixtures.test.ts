import { describe, expect, it } from 'vitest';
import {
  dataSeedSql,
  DEFAULT_MATRIX_DRIVERS,
  filterSeedSql,
  getFixture,
  journeyAllowed,
  listMatrixFixtures,
  seedTableName,
  skipReason,
  type HostContractJourneyId,
} from '../fixtures';

describe('DriverFixture definitions', () => {
  it('exposes postgres, mysql, and sqlite in the default matrix', () => {
    expect([...DEFAULT_MATRIX_DRIVERS]).toEqual(['postgres', 'mysql', 'sqlite']);
    const fixtures = listMatrixFixtures();
    expect(fixtures.map((f) => f.id)).toEqual(['postgres', 'mysql', 'sqlite']);
  });

  it('uses distinct display names and table prefixes', () => {
    const fixtures = listMatrixFixtures();
    const names = new Set(fixtures.map((f) => f.displayName));
    const prefixes = new Set(fixtures.map((f) => f.dialect.tablePrefix));
    expect(names.size).toBe(3);
    expect(prefixes.size).toBe(3);
  });

  it('quoteIdent escapes dialect-specific characters', () => {
    expect(getFixture('postgres').dialect.quoteIdent('a"b')).toBe('"a""b"');
    expect(getFixture('mysql').dialect.quoteIdent('a`b')).toBe('`a``b`');
    expect(getFixture('sqlite').dialect.quoteIdent('a"b')).toBe('"a""b"');
  });
});

describe('journeyAllowed / skipReason', () => {
  const allJourneys: HostContractJourneyId[] = [
    'HC-CONN',
    'HC-QUERY',
    'HC-DATA',
    'HC-FILTER',
    'HC-EDIT',
    'HC-STRUCT',
    'HC-INDEX',
    'HC-EXPORT',
    'HC-OBJ',
    'HC-EXPLAIN',
  ];

  it('allows full SQL contract on postgres and mysql', () => {
    for (const id of ['postgres', 'mysql'] as const) {
      const f = getFixture(id);
      for (const j of allJourneys) {
        expect(journeyAllowed(f, j), `${id} ${j}`).toBe(true);
        expect(skipReason(f, j)).toBeNull();
      }
    }
  });

  it('skips objects and privileges-related journeys on sqlite', () => {
    const f = getFixture('sqlite');
    expect(journeyAllowed(f, 'HC-OBJ')).toBe(false);
    expect(skipReason(f, 'HC-OBJ')).toMatch(/hasObjects/);
    expect(journeyAllowed(f, 'HC-DATA')).toBe(true);
    expect(journeyAllowed(f, 'HC-FILTER')).toBe(true);
    expect(journeyAllowed(f, 'HC-EXPLAIN')).toBe(true);
  });

  it('rejects non-sql connection modes for all journeys', () => {
    const fake = {
      ...getFixture('postgres'),
      capabilities: {
        ...getFixture('postgres').capabilities,
        connectionMode: 'redis' as const,
      },
    };
    expect(journeyAllowed(fake, 'HC-DATA')).toBe(false);
    expect(skipReason(fake, 'HC-DATA')).toMatch(/redis/);
  });
});

describe('seed helpers', () => {
  it('builds safe seed table names', () => {
    const f = getFixture('postgres');
    expect(seedTableName(f, 'filter')).toBe('_e2e_hc_pg_filter');
    expect(seedTableName(f, 'bad-name!')).toBe('_e2e_hc_pg_bad_name_');
  });

  it('emits dialect-specific filter seed SQL', () => {
    const pg = filterSeedSql(getFixture('postgres'), 't_pg');
    expect(pg[1]).toMatch(/SERIAL/);
    expect(pg[2]).toContain('alpha');

    const my = filterSeedSql(getFixture('mysql'), 't_my');
    expect(my[1]).toMatch(/AUTO_INCREMENT/);
    expect(my[2]).toContain('beta');

    const lt = filterSeedSql(getFixture('sqlite'), 't_lt');
    expect(lt[1]).toMatch(/AUTOINCREMENT/);
    expect(lt[2]).toContain('gamma');
  });

  it('emits data seed inserts for requested row counts', () => {
    const pg = dataSeedSql(getFixture('postgres'), 't', 3);
    expect(pg[2]).toContain('generate_series(1, 3)');

    const my = dataSeedSql(getFixture('mysql'), 't', 2);
    expect(my[2]).toContain('user_1');
    expect(my[2]).toContain('user_2');
    expect(my[2]).not.toContain('user_3');

    const empty = dataSeedSql(getFixture('sqlite'), 't', 0);
    expect(empty[2]).toMatch(/SELECT 1/);
  });

  it('floors fractional and clamps negative row counts', () => {
    const my = dataSeedSql(getFixture('mysql'), 't', 2.9);
    expect(my[2]).toContain('user_2');
    expect(my[2]).not.toContain('user_3');

    const neg = dataSeedSql(getFixture('mysql'), 't', -5);
    expect(neg[2]).toMatch(/SELECT 1/);
  });
});
