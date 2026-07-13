import { describe, it, expect } from 'vitest';
import { DB_REGISTRY } from '../databaseTypes';

describe('DB_REGISTRY behavioral flags', () => {
  it('kiwi has multi-database and fixed page size', () => {
    expect(DB_REGISTRY.kiwi.hasMultiDatabase).toBe(true);
    expect(DB_REGISTRY.kiwi.defaultPageSize).toBe(1000);
    expect(DB_REGISTRY.kiwi.connectionForm).toBe('kiwi');
  });

  it('redis uses index form and keyvalue view', () => {
    expect(DB_REGISTRY.redis.connectionForm).toBe('index');
    expect(DB_REGISTRY.redis.connectionView).toBe('keyvalue');
  });

  it('sqlite uses file form', () => {
    expect(DB_REGISTRY.sqlite.connectionForm).toBe('file');
  });

  it('standard sql dbs use standard form', () => {
    expect(DB_REGISTRY.postgresql.connectionForm).toBe('standard');
    expect(DB_REGISTRY.mysql.connectionForm).toBe('standard');
  });

  it('presto and trino use catalog connection form', () => {
    expect(DB_REGISTRY.presto.connectionForm).toBe('catalog');
    expect(DB_REGISTRY.trino.connectionForm).toBe('catalog');
    expect(DB_REGISTRY.presto.hasMultiDatabase).toBe(true);
    expect(DB_REGISTRY.trino.sqlDialect).toBe('trino');
  });
});
