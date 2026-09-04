import { describe, expect, it, vi, beforeEach, afterEach } from 'vitest';
import { renderHook } from '@testing-library/react';
import type { ConnectionConfig } from '../../types';

const urlParamMock = vi.fn<(name: string) => string | null>();

vi.mock('../windowKind', () => ({
  getUrlParam: (name: string) => urlParamMock(name),
}));

import {
  buildMigrationWindowUrlParams,
  pickPrefillDatabase,
  pickPrefillSchema,
  readMigrationPrefillFromUrl,
  useMigrationEndpointPrefill,
} from '../migrationWindowPrefill';

const pgSrc: ConnectionConfig = {
  id: 'pg-src',
  name: 'PG Src',
  databaseType: 'postgresql',
  host: '127.0.0.1',
  port: 5432,
  database: 'src',
  username: 'postgres',
  password: '',
  sslMode: 'disable',
};

const pgTgt: ConnectionConfig = {
  id: 'pg-tgt',
  name: 'PG Tgt',
  databaseType: 'postgresql',
  host: '127.0.0.1',
  port: 5432,
  database: 'tgt',
  username: 'postgres',
  password: '',
  sslMode: 'disable',
};

describe('migrationWindowPrefill', () => {
  beforeEach(() => {
    urlParamMock.mockReset();
  });

  afterEach(() => {
    vi.clearAllMocks();
  });

  it('readMigrationPrefillFromUrl collects endpoint params', () => {
    urlParamMock.mockImplementation((name) => {
      const values: Record<string, string> = {
        sourceId: 'pg-src',
        targetId: 'pg-tgt',
        sourceDatabase: 'src',
        targetDatabase: 'tgt',
        sourceSchema: 'public',
        targetSchema: 'public',
      };
      return values[name] ?? null;
    });
    expect(readMigrationPrefillFromUrl()).toEqual({
      sourceId: 'pg-src',
      targetId: 'pg-tgt',
      sourceDatabase: 'src',
      targetDatabase: 'tgt',
      sourceSchema: 'public',
      targetSchema: 'public',
    });
  });

  it('buildMigrationWindowUrlParams serializes window kind and prefill', () => {
    expect(
      buildMigrationWindowUrlParams('data-sync', {
        sourceId: 'pg-src',
        targetDatabase: 'tgt',
      }),
    ).toEqual({
      window: 'data-sync',
      sourceId: 'pg-src',
      targetDatabase: 'tgt',
    });
  });

  it('pickPrefillDatabase consumes prefill once when present in list', () => {
    const prefillRef = {
      current: { sourceDatabase: 'analytics' },
    };
    const picked = pickPrefillDatabase(
      prefillRef,
      'source',
      ['default', 'analytics'],
      () => 'default',
      '',
    );
    expect(picked).toBe('analytics');
    expect(prefillRef.current.sourceDatabase).toBeUndefined();
  });

  it('pickPrefillSchema falls back when prefill schema is missing', () => {
    const prefillRef = { current: {} };
    const picked = pickPrefillSchema(
      prefillRef,
      'target',
      ['public', 'app'],
      (prev) => prev || 'public',
      '',
    );
    expect(picked).toBe('public');
  });

  it('useMigrationEndpointPrefill applies connection ids once connections load', () => {
    urlParamMock.mockImplementation((name) => {
      if (name === 'sourceId') return 'pg-src';
      if (name === 'targetId') return 'pg-tgt';
      return null;
    });
    const setSourceId = vi.fn();
    const setTargetId = vi.fn();
    const { rerender } = renderHook(
      ({ connections }: { connections: ConnectionConfig[] }) =>
        useMigrationEndpointPrefill(connections, setSourceId, setTargetId),
      { initialProps: { connections: [] as ConnectionConfig[] } },
    );
    expect(setSourceId).not.toHaveBeenCalled();
    rerender({ connections: [pgSrc, pgTgt] });
    expect(setSourceId).toHaveBeenCalledWith('pg-src');
    expect(setTargetId).toHaveBeenCalledWith('pg-tgt');
    rerender({ connections: [pgSrc, pgTgt, { ...pgTgt, id: 'other' }] });
    expect(setSourceId).toHaveBeenCalledTimes(1);
    expect(setTargetId).toHaveBeenCalledTimes(1);
  });
});
