import { describe, it, expect } from 'vitest';
import { DB_REGISTRY } from '../databaseTypes';

describe('DB_REGISTRY behavioral flags', () => {
  it('kiwi has multi-database and fixed page size when plugin is loaded', () => {
    if (!DB_REGISTRY.kiwi) return; // drivers not injected in this workspace
    expect(DB_REGISTRY.kiwi.hasMultiDatabase).toBe(true);
    expect(DB_REGISTRY.kiwi.databaseFieldType).toBe('domain');
    expect(DB_REGISTRY.kiwi.defaultPageSize).toBe(999);
    expect(DB_REGISTRY.kiwi.connectionForm).toBe('kiwi');
  });

  it('mysql and mariadb enable multi-database session capability', () => {
    expect(DB_REGISTRY.mysql.hasMultiDatabase).toBe(true);
    expect(DB_REGISTRY.mariadb.hasMultiDatabase).toBe(true);
  });

  it('postgresql enables multi-database session capability', () => {
    expect(DB_REGISTRY.postgresql.hasMultiDatabase).toBe(true);
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

  it('supportsExplain is opt-in via explicit true', () => {
    expect(DB_REGISTRY.postgresql.supportsExplain).toBe(true);
    expect(DB_REGISTRY.redis.supportsExplain).toBeUndefined();
  });

  it('native SQL engines advertise explain only when backend implements it', () => {
    for (const id of ['clickhouse', 'duckdb', 'rqlite', 'turso', 'sqlserver'] as const) {
      if (!DB_REGISTRY[id]) continue;
      expect(DB_REGISTRY[id].supportsExplain).toBe(true);
    }
  });

  it('ob_oracle reuses MySQL wire protocol quoting', () => {
    expect(DB_REGISTRY.ob_oracle.quoteChar).toBe('`');
    expect(DB_REGISTRY.ob_oracle.sqlDialect).toBe('mysql');
  });

  it('mongodb uses document connection view', () => {
    if (!DB_REGISTRY.mongodb) return;
    expect(DB_REGISTRY.mongodb.connectionView).toBe('document');
    expect(DB_REGISTRY.mongodb.category).toBe('document');
    expect(DB_REGISTRY.mongodb.supportsSQL).toBe(false);
    expect(DB_REGISTRY.mongodb.hasMultiDatabase).toBe(true);
  });
});
