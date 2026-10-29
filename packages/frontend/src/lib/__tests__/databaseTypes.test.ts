import { describe, it, expect } from 'vitest';
import {
  DB_REGISTRY,
  escapeIdent,
  formatConnectionAddr,
  getDbIcon,
  getDbLabel,
  getDriverIconMap,
  getDriverIconParents,
  type DatabaseTypeMeta,
} from '../databaseTypes';
import type { DatabaseType } from '../../types';

/**
 * `DatabaseType` and `DB_REGISTRY` are codegen'd from the drivers resolved into
 * *this* SKU, so a driver documented below may legitimately be absent from the
 * current build. The cases keep their `if (!entry) return;` guards and look the
 * registry up by plain id instead of pinning the generated union.
 */
const REGISTRY_BY_ID = DB_REGISTRY as unknown as Record<string, DatabaseTypeMeta | undefined>;

function dbTypeOf(id: string): DatabaseType {
  return id as DatabaseType;
}

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
    for (const id of ['clickhouse', 'duckdb', 'rqlite', 'turso', 'sqlserver']) {
      const entry = REGISTRY_BY_ID[id];
      if (!entry) continue;
      expect(entry.supportsExplain).toBe(true);
    }
  });

  it('ob_oracle reuses MySQL wire protocol quoting', () => {
    expect(DB_REGISTRY.ob_oracle.quoteChar).toBe('`');
    expect(DB_REGISTRY.ob_oracle.sqlDialect).toBe('mysql');
  });

  it('mongodb uses document connection view', () => {
    const mongodb = REGISTRY_BY_ID.mongodb;
    if (!mongodb) return;
    expect(mongodb.connectionView).toBe('document');
    expect(mongodb.category).toBe('document');
    expect(mongodb.supportsSQL).toBe(false);
    expect(mongodb.hasMultiDatabase).toBe(true);
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
      databaseType: dbTypeOf('mongodb'),
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
