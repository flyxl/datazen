import { describe, expect, it, vi } from 'vitest';
import {
  applyConnectionClipboardFill,
  applyMatchedClipboard,
  matchConnectionClipboard,
  parseGenericConnectionUrl,
} from '../connectionClipboard';
import { DB_REGISTRY } from '../databaseTypes';
import type { ConnectionFormState } from '../../components/connection/useConnectionForm';

function stubForm(overrides: Partial<ConnectionFormState> = {}): ConnectionFormState {
  return {
    name: '',
    databaseType: 'postgresql',
    host: '127.0.0.1',
    port: '5432',
    database: 'postgres',
    username: 'postgres',
    password: '',
    options: {},
    setName: vi.fn(),
    setHost: vi.fn(),
    setPort: vi.fn(),
    setDatabase: vi.fn(),
    setUsername: vi.fn(),
    setPassword: vi.fn(),
    setSchema: vi.fn(),
    setSslMode: vi.fn(),
    setOptions: vi.fn(),
    setShowAdvanced: vi.fn(),
    handleDatabaseTypeChange: vi.fn(),
    ...overrides,
  } as ConnectionFormState;
}

describe('matchConnectionClipboard', () => {
  it('returns null for empty or quoted-empty clipboard', () => {
    expect(matchConnectionClipboard('')).toBeNull();
    expect(matchConnectionClipboard('   \n')).toBeNull();
    expect(matchConnectionClipboard('""')).toBeNull();
  });

  it('selects redis and fills from a redis URL', () => {
    const matched = matchConnectionClipboard('rediss://alice:s3cret@cache.internal:6380/2');
    expect(matched?.databaseType).toBe('redis');
    expect(matched?.fill.host).toBe('cache.internal');
    expect(matched?.fill.port).toBe('6380');
    expect(matched?.fill.sslMode).toBe('require');
    expect(matched?.fill.expandAdvanced).toBe(true);
    expect(matched?.fill.options).toEqual(expect.objectContaining({ tls: { enabled: true } }));
  });

  it('selects postgresql from postgres:// and expands SSL advanced', () => {
    const matched = matchConnectionClipboard(
      'postgres://app:secret@db.example.com:5432/orders?sslmode=require',
    );
    expect(matched?.databaseType).toBe('postgresql');
    expect(matched?.fill).toEqual(
      expect.objectContaining({
        host: 'db.example.com',
        port: '5432',
        database: 'orders',
        username: 'app',
        password: 'secret',
        sslMode: 'require',
        expandAdvanced: true,
      }),
    );
  });

  it('accepts jdbc URLs, env prefixes, quotes, and username-only auth', () => {
    expect(
      matchConnectionClipboard('jdbc:postgresql://db.internal:5432/app')?.databaseType,
    ).toBe('postgresql');
    expect(
      matchConnectionClipboard('DATABASE_URL=postgres://app@db.internal/orders')?.fill,
    ).toEqual(
      expect.objectContaining({
        host: 'db.internal',
        port: '5432',
        database: 'orders',
        username: 'app',
      }),
    );
    expect(matchConnectionClipboard('"mysql://root@127.0.0.1:3306/app"')?.databaseType).toBe(
      'mysql',
    );
  });

  it('selects mysql from mysql://', () => {
    expect(matchConnectionClipboard('mysql://root@127.0.0.1:3306/app')?.databaseType).toBe(
      'mysql',
    );
  });

  it('uses a unique default port when there is no URL scheme', () => {
    expect(matchConnectionClipboard('10.0.0.8:6379')?.databaseType).toBe('redis');
    expect(matchConnectionClipboard('10.0.0.8:8812')?.databaseType).toBe('questdb');
    expect(matchConnectionClipboard('[::1]:8812')?.databaseType).toBe('questdb');
  });

  it('does not claim an ambiguous shared port or a non-endpoint', () => {
    expect(matchConnectionClipboard('10.0.0.8:3306')).toBeNull();
    expect(matchConnectionClipboard('10.0.0.8:5432')).toBeNull();
    expect(matchConnectionClipboard('10.0.0.8:6379,10.0.0.9:6379')?.databaseType).toBe(
      'redis',
    );
    expect(matchConnectionClipboard('not-a-host')).toBeNull();
    expect(matchConnectionClipboard('host:99999')).toBeNull();
    expect(matchConnectionClipboard('[::1]')).toBeNull();
    expect(matchConnectionClipboard('[bad')).toBeNull();
  });

  it('ignores drivers that are not available in this build', () => {
    expect(
      matchConnectionClipboard('redis://cache:6379', ['postgresql', 'mysql']),
    ).toBeNull();
  });

  it('returns null when a claimed URL has no host', () => {
    expect(matchConnectionClipboard('postgres://')).toBeNull();
    expect(matchConnectionClipboard('postgres://user@')).toBeNull();
  });
});

describe('parseGenericConnectionUrl', () => {
  it('parses sqlite file URLs into the database path', () => {
    const fill = parseGenericConnectionUrl('sqlite:////tmp/app.db', DB_REGISTRY.sqlite);
    expect(fill?.database).toBe('/tmp/app.db');
    expect(fill?.name).toBe('app.db');
  });

  it('returns null for an empty sqlite URL', () => {
    expect(parseGenericConnectionUrl('sqlite://', DB_REGISTRY.sqlite)).toBeNull();
    expect(parseGenericConnectionUrl('not a url', DB_REGISTRY.sqlite)).toBeNull();
  });

  it('maps ssl query values and keeps invalid percent-encoding', () => {
    expect(
      parseGenericConnectionUrl(
        'postgres://u:p@h:5432/db?sslmode=disable',
        DB_REGISTRY.postgresql,
      )?.sslMode,
    ).toBe('disable');
    expect(
      parseGenericConnectionUrl(
        'postgres://u:p@h:5432/db?sslmode=prefer',
        DB_REGISTRY.postgresql,
      )?.expandAdvanced,
    ).toBe(false);
    expect(
      parseGenericConnectionUrl(
        'postgres://u:p@h:5432/db?ssl=verify-full',
        DB_REGISTRY.postgresql,
      )?.sslMode,
    ).toBe('require');
    expect(
      parseGenericConnectionUrl('postgres://u:p@h:5432/db?sslmode=weird', DB_REGISTRY.postgresql)
        ?.sslMode,
    ).toBeUndefined();
    expect(
      parseGenericConnectionUrl('postgres://u:%@h:5432/db', DB_REGISTRY.postgresql)?.password,
    ).toBe('%');
  });

  it('parses IPv6 hosts and default ports', () => {
    const fill = parseGenericConnectionUrl(
      'postgres://[2001:db8::1]/orders',
      DB_REGISTRY.postgresql,
    );
    expect(fill?.host).toBe('[2001:db8::1]');
    expect(fill?.port).toBe('5432');
    expect(fill?.database).toBe('orders');
  });
});

describe('applyMatchedClipboard', () => {
  it('switches driver, applies fields, and expands advanced', () => {
    const form = stubForm();
    applyMatchedClipboard(form, {
      databaseType: 'redis',
      fill: {
        host: 'cache.internal',
        port: '6380',
        sslMode: 'require',
        options: { topology: 'standalone', tls: { enabled: true } },
        expandAdvanced: true,
        name: 'cache.internal:6380',
      },
    });
    expect(form.handleDatabaseTypeChange).toHaveBeenCalledWith('redis');
    expect(form.setHost).toHaveBeenCalledWith('cache.internal');
    expect(form.setPort).toHaveBeenCalledWith('6380');
    expect(form.setSslMode).toHaveBeenCalledWith('require');
    expect(form.setShowAdvanced).toHaveBeenCalledWith(true);
    expect(form.setName).toHaveBeenCalledWith('cache.internal:6380');
  });

  it('does not switch when the form is already on that driver', () => {
    const form = stubForm({ databaseType: 'postgresql' });
    applyMatchedClipboard(form, {
      databaseType: 'postgresql',
      fill: { host: 'db.internal', port: '5432' },
    });
    expect(form.handleDatabaseTypeChange).not.toHaveBeenCalled();
    expect(form.setHost).toHaveBeenCalledWith('db.internal');
  });

  it('applies optional fields and expands advanced for disable SSL', () => {
    const form = stubForm({ options: undefined, name: 'keep-me' });
    applyConnectionClipboardFill(form, {
      database: 'orders',
      username: 'app',
      password: 'secret',
      schema: 'public',
      sslMode: 'disable',
      options: { extra: true },
      name: 'ignored',
    });
    expect(form.setDatabase).toHaveBeenCalledWith('orders');
    expect(form.setUsername).toHaveBeenCalledWith('app');
    expect(form.setPassword).toHaveBeenCalledWith('secret');
    expect(form.setSchema).toHaveBeenCalledWith('public');
    expect(form.setSslMode).toHaveBeenCalledWith('disable');
    expect(form.setOptions).toHaveBeenCalledWith({ extra: true });
    expect(form.setShowAdvanced).toHaveBeenCalledWith(true);
    expect(form.setName).not.toHaveBeenCalled();
  });
});
