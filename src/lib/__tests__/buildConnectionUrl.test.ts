import { describe, it, expect } from 'vitest';
import { buildConnectionUrl } from '../buildConnectionUrl';
import type { ConnectionConfig } from '../../types';

function makeConfig(overrides: Partial<ConnectionConfig>): ConnectionConfig {
  return {
    id: 'test-id',
    name: 'Test',
    databaseType: 'postgresql' as any,
    sslMode: 'disable',
    ...overrides,
  };
}

describe('buildConnectionUrl', () => {
  it('builds PostgreSQL URL with all fields', () => {
    const url = buildConnectionUrl(
      makeConfig({
        databaseType: 'postgresql' as any,
        host: 'db.example.com',
        port: 5432,
        database: 'mydb',
        username: 'admin',
        password: 's3cret',
      }),
    );
    expect(url).toBe('postgres://admin:s3cret@db.example.com/mydb');
  });

  it('includes non-default port', () => {
    const url = buildConnectionUrl(
      makeConfig({
        databaseType: 'postgresql' as any,
        host: 'localhost',
        port: 5433,
        database: 'mydb',
        username: 'admin',
      }),
    );
    expect(url).toBe('postgres://admin@localhost:5433/mydb');
  });

  it('omits default port', () => {
    const url = buildConnectionUrl(
      makeConfig({
        databaseType: 'postgresql' as any,
        host: 'localhost',
        port: 5432,
        database: 'mydb',
      }),
    );
    expect(url).toBe('postgres://localhost/mydb');
  });

  it('builds MySQL URL', () => {
    const url = buildConnectionUrl(
      makeConfig({
        databaseType: 'mysql' as any,
        host: '127.0.0.1',
        port: 3306,
        database: 'shop',
        username: 'root',
        password: 'pass',
      }),
    );
    expect(url).toBe('mysql://root:pass@127.0.0.1/shop');
  });

  it('builds Redis URL', () => {
    const url = buildConnectionUrl(
      makeConfig({
        databaseType: 'redis' as any,
        host: 'redis.local',
        port: 6379,
        database: '0',
        password: 'pw',
      }),
    );
    expect(url).toBe('redis://:pw@redis.local/0');
  });

  it('builds SQLite file URL', () => {
    const url = buildConnectionUrl(
      makeConfig({
        databaseType: 'sqlite' as any,
        database: '/tmp/test.db',
      }),
    );
    expect(url).toBe('sqlite:///tmp/test.db');
  });

  it('appends sslmode when not disable', () => {
    const url = buildConnectionUrl(
      makeConfig({
        host: 'secure.host',
        database: 'db',
        sslMode: 'require',
      }),
    );
    expect(url).toContain('?sslmode=require');
  });

  it('encodes special characters in username and password', () => {
    const url = buildConnectionUrl(
      makeConfig({
        host: 'localhost',
        database: 'db',
        username: 'user@org',
        password: 'p@ss:w/rd',
      }),
    );
    expect(url).toContain('user%40org:p%40ss%3Aw%2Frd@');
  });

  it('returns null for unknown database type', () => {
    const url = buildConnectionUrl(makeConfig({ databaseType: 'unknown-type' as any }));
    expect(url).toBeNull();
  });

  it('handles missing database gracefully', () => {
    const url = buildConnectionUrl(makeConfig({ host: 'localhost', username: 'admin' }));
    expect(url).toBe('postgres://admin@localhost');
  });

  it('returns null for file driver without database path', () => {
    const url = buildConnectionUrl(
      makeConfig({ databaseType: 'sqlite' as any, database: undefined }),
    );
    expect(url).toBeNull();
  });
});
