import { describe, expect, it, vi } from 'vitest';
import type { QualifiedRelationId } from '../../semantic/types';
import type { SqlDialectStrategy } from '../../../lib/sqlDialects';
import type { DdlCacheIdentity } from '../../../lib/schemaCache';
import { copyRelationDdl, resolveDdlQuery } from '../copyDdl';

function ident(namespace: string[], name: string): QualifiedRelationId {
  return {
    namespacePath: namespace.map((n) => ({ name: n, quoted: false })),
    name: { name, quoted: false },
  };
}

const tableDialect: SqlDialectStrategy = {
  family: 'postgresql',
  ddl: {
    getTableDdlQuery: (name, schema) => ({
      sql: `DDL ${schema ?? ''}${name}`,
      extractColumnIndex: 0,
    }),
    getViewDdlQuery: (name, schema) => ({
      sql: `VIEW ${schema ?? ''}${name}`,
      extractColumnIndex: 1,
    }),
  },
  index: {
    supportedIndexMethods: ['btree'],
    getDropIndexSql: () => '',
    getCreateIndexSql: () => '',
  },
  backupOptions: [],
};

describe('metadata copyDdl', () => {
  it('resolves a table DDL query with schema ref', () => {
    const r = resolveDdlQuery(tableDialect, 'users', 'table', 'public');
    expect(r).toEqual({ sql: 'DDL publicusers', extractColumnIndex: 0, supported: true });
  });

  it('resolves a view DDL query when the dialect supports it', () => {
    const r = resolveDdlQuery(tableDialect, 'orders', 'view', 'public');
    expect(r).toEqual({ sql: 'VIEW publicorders', extractColumnIndex: 1, supported: true });
  });

  it('reports unsupported when there is no ddl dialect', () => {
    expect(resolveDdlQuery(null, 'users', 'table')).toEqual({
      sql: '',
      extractColumnIndex: 0,
      supported: false,
    });
  });

  it('calls the upgraded getDdl with full identity and extractor', async () => {
    const getDdl = vi.fn(
      async (
        _s: string,
        _t: string,
        _sql: string,
        extract: (r: unknown[][]) => string,
        ident: DdlCacheIdentity,
      ) => {
        expect(ident).toEqual({ objectKind: 'table', namespacePath: ['public'] });
        return extract([['CREATE TABLE users (...)']]);
      },
    );
    const ddl = await copyRelationDdl('s1', ident(['public'], 'users'), 'table', {
      databaseType: 'postgresql',
      getDdl: getDdl as never,
      getDialect: () => tableDialect,
    });
    expect(ddl).toBe('CREATE TABLE users (...)');
    expect(getDdl).toHaveBeenCalledWith('s1', 'users', 'DDL publicusers', expect.any(Function), {
      objectKind: 'table',
      namespacePath: ['public'],
    });
  });

  it('passes objectKind view and the raw namespace when allowQualified=false', async () => {
    const getDdl = vi.fn(
      async (
        _s: string,
        _t: string,
        _sql: string,
        _extract: (r: unknown[][]) => string,
        ident: DdlCacheIdentity,
      ) => `ddl-${ident.objectKind}`,
    );
    const ddl = await copyRelationDdl('s1', ident(['public'], 'orders'), 'view', {
      databaseType: 'postgresql',
      allowQualified: false,
      getDdl: getDdl as never,
      getDialect: () => tableDialect,
    });
    expect(ddl).toBe('ddl-view');
    // allowQualified=false → schemaRef undefined, so the identity uses the raw namespace path.
    const call = getDdl.mock.calls[0]![3];
    const extracted = call([['ignored', 'CREATE VIEW orders AS SELECT 1']]);
    expect(extracted).toBe('CREATE VIEW orders AS SELECT 1');
  });

  it('throws when the dialect cannot generate DDL', async () => {
    await expect(
      copyRelationDdl('s1', ident([], 'users'), 'table', {
        databaseType: 'generic',
        getDialect: () => null,
      }),
    ).rejects.toThrow('unable-generate-ddl');
  });

  it('falls back to the table query for views when getViewDdlQuery is absent', () => {
    const dialectSql: SqlDialectStrategy = {
      family: 'x',
      ddl: { getTableDdlQuery: (n) => ({ sql: `T ${n}`, extractColumnIndex: 0 }) },
      index: {
        supportedIndexMethods: ['btree'],
        getDropIndexSql: () => '',
        getCreateIndexSql: () => '',
      },
      backupOptions: [],
    };
    expect(resolveDdlQuery(dialectSql, 'v', 'view', undefined).sql).toBe('T v');
  });
});
