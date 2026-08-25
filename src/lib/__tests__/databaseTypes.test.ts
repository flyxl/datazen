import { describe, it, expect } from 'vitest';
import {
  DB_REGISTRY,
  escapeIdent,
  formatConnectionAddr,
  getDbIcon,
  getDbLabel,
  getDriverIconMap,
  getDriverIconParents,
} from '../databaseTypes';

describe('DB_REGISTRY behavioral flags', () => {
  it('mysql and mariadb enable multi-database session capability', () => {
    expect(DB_REGISTRY.mysql.hasMultiDatabase).toBe(true);
    expect(DB_REGISTRY.mariadb.hasMultiDatabase).toBe(true);
  });

  it('postgresql enables multi-database session capability', () => {
    expect(DB_REGISTRY.postgresql.hasMultiDatabase).toBe(true);
  });

  it('redis uses redis form and keyvalue view', () => {
    if (!DB_REGISTRY.redis) return;
    expect(DB_REGISTRY.redis.connectionForm).toBe('redis');
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
    if (DB_REGISTRY.redis) expect(DB_REGISTRY.redis.supportsExplain).toBeUndefined();
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

describe('escapeIdent', () => {
  it('quotes postgres identifiers with double quotes', () => {
    expect(escapeIdent('user"name', 'postgresql')).toBe('"user""name"');
  });

  it('quotes mysql identifiers with backticks', () => {
    expect(escapeIdent('col`name', 'mysql')).toBe('`col``name`');
  });

  it('returns bare name for redis (no quoting)', () => {
    if (!DB_REGISTRY.redis) return;
    expect(escapeIdent('mykey', 'redis')).toBe('mykey');
  });
});

describe('getDbLabel and icons', () => {
  it('returns registry label or fallback id', () => {
    expect(getDbLabel('postgresql')).toBeTruthy();
    expect(getDbLabel('unknown' as 'postgresql')).toBe('unknown');
  });

  it('getDbIcon returns meta or default', () => {
    expect(getDbIcon('postgresql').label.length).toBeGreaterThan(0);
    expect(getDbIcon('unknown' as 'postgresql')).toEqual({ label: 'DB', bg: 'bg-gray-500' });
  });

  it('getDriverIconMap and parents are objects', () => {
    expect(typeof getDriverIconMap()).toBe('object');
    expect(typeof getDriverIconParents()).toBe('object');
  });
});

describe('formatConnectionAddr', () => {
  it('formats file mode from database path', () => {
    const addr = formatConnectionAddr({
      databaseType: 'sqlite',
      database: '/tmp/app.db',
    });
    expect(addr).toContain('/tmp/app.db');
  });

  it('formats url mode from host', () => {
    const addr = formatConnectionAddr({
      databaseType: 'mongodb',
      host: 'mongodb://localhost',
    });
    expect(addr).toContain('mongodb://localhost');
  });

  it('formats SSH tunnel prefix', () => {
    const addr = formatConnectionAddr({
      databaseType: 'postgresql',
      host: 'db.internal',
      database: 'app',
      sshTunnel: { enabled: true, host: 'bastion' },
    });
    expect(addr).toContain('bastion');
    expect(addr).toContain('db.internal');
  });

  it('formats standard host : database', () => {
    const addr = formatConnectionAddr({
      databaseType: 'postgresql',
      host: 'localhost',
      database: 'app',
    });
    expect(addr).toBe('localhost : app');
  });
});
