import { describe, it, expect } from 'vitest';
import type { EditorMetadataSnapshot, EditorRelationMetadata } from '../../metadata/types';
import type {
  QualifiedRelationId,
  SqlSemanticModel,
  SqlScope,
  SqlToken,
} from '../../semantic/types';
import { SqlTokenKind } from '../../semantic/tokens';
import { getDialectAdapter } from '../../semantic/dialectAdapter';
import { buildSemanticModel } from '../../semantic/scopeModel';
import {
  produceSchemaCompletions,
  resolveAliasDotCompletions,
  extractSelectColumns,
} from '../schemaCompletion';

/* -------------------------------------------------------------------------- */
/*  Helpers                                                                   */
/* -------------------------------------------------------------------------- */

function makeIdentity(name: string, namespace: string[] = []): QualifiedRelationId {
  return {
    namespacePath: namespace.map((n) => ({ name: n, quoted: false })),
    name: { name, quoted: false },
  };
}

function makeRelation(
  name: string,
  columns: { name: string; dataType: string; nullable: boolean; comment?: string }[],
  namespace: string[] = [],
  adapter: ReturnType<typeof getDialectAdapter> = getDialectAdapter('standard'),
): EditorRelationMetadata {
  const identity = makeIdentity(name, namespace);
  // Key must match what relationKey() produces: folded segments joined by '.'
  const key = [...namespace, name].map((s) => adapter.foldUnquotedIdentifier(s)).join('.');
  return {
    key,
    identity,
    kind: 'table',
    columns: columns.map((c) => ({
      name: c.name,
      dataType: c.dataType,
      nullable: c.nullable,
      comment: c.comment,
      isPrimaryKey: false,
    })),
    primaryKey: [],
    indexes: [],
    foreignKeys: [],
    loadedAt: Date.now(),
  };
}

function makeSnapshot(relations: EditorRelationMetadata[]): EditorMetadataSnapshot {
  const map = new Map<string, EditorRelationMetadata>();
  for (const rel of relations) {
    map.set(rel.key, rel);
  }
  return {
    dbSessionId: 'test-session',
    epoch: 1,
    relations: map,
  };
}

function makeModel(
  scopes: SqlScope[],
  cursorIntent: SqlSemanticModel['cursorIntent'],
  tokens: readonly SqlToken[] = [],
): SqlSemanticModel {
  return {
    statement: {
      index: 0,
      from: 0,
      to: 100,
      contentFrom: 0,
      contentTo: 100,
      delimiterFrom: null,
      delimiterTo: null,
      firstExecutableLine: 0,
      confidence: 'exact',
    },
    tokens,
    scopes,
    references: [],
    cursorIntent,
    diagnostics: [],
  };
}

/** Build a simple SqlToken for testing. */
function tok(
  kind: SqlTokenKind,
  text: string,
  from: number,
  to?: number,
  parenDepth = 0,
): SqlToken {
  return { kind, text, from, to: to ?? from + text.length, parenDepth };
}

/* -------------------------------------------------------------------------- */
/*  Tests                                                                     */
/* -------------------------------------------------------------------------- */

describe('schemaCompletion', () => {
  const adapter = getDialectAdapter('standard');

  describe('resolveAliasDotCompletions', () => {
    it('returns columns for a uniquely resolved alias', () => {
      const users = makeRelation('users', [
        { name: 'id', dataType: 'integer', nullable: false },
        { name: 'name', dataType: 'varchar', nullable: true },
        { name: 'email', dataType: 'varchar', nullable: false, comment: 'User email' },
      ]);
      const snapshot = makeSnapshot([users]);

      const model = makeModel(
        [
          {
            id: 's1',
            range: { from: 0, to: 100 },
            kind: 'select',
            ctes: [],
            relations: [
              {
                relation: makeIdentity('users'),
                alias: 'u',
                sourceRange: { from: 0, to: 100 },
                sourceKind: 'table',
              },
            ],
            projectionAliases: [],
          },
        ],
        {
          kind: 'qualified_column',
          prefix: 'u.',
          replacementRange: { from: 3, to: 3 },
          qualifierParts: ['u'],
        },
      );

      const completions = resolveAliasDotCompletions(['u'], model, snapshot, adapter);
      expect(completions).not.toBeNull();
      // Default quotePolicy is 'unquoted': safe identifiers are unquoted
      expect(completions!.length).toBe(3);
      const labels = completions!.map((c) => c.label);
      expect(labels).toEqual(['id', 'name', 'email']);

      // When quotePolicy is 'both', both unquoted and quoted variants are provided
      const bothCompletions = resolveAliasDotCompletions(
        ['u'],
        model,
        snapshot,
        adapter,
        undefined,
        'both',
      );
      expect(bothCompletions!.length).toBe(6);
      const bothLabels = bothCompletions!.map((c) => c.label);
      expect(bothLabels).toContain('"id"');
      expect(bothLabels).toContain('id');
      expect(bothLabels).toContain('"name"');
      expect(bothLabels).toContain('name');
      expect(bothLabels).toContain('"email"');
      expect(bothLabels).toContain('email');
    });

    it('quotes reserved keywords even with default unquoted policy', () => {
      const orders = makeRelation('orders', [
        { name: 'id', dataType: 'integer', nullable: false },
        { name: 'order', dataType: 'varchar', nullable: true },
        { name: 'user', dataType: 'varchar', nullable: true },
      ]);
      const snapshot = makeSnapshot([orders]);
      const model = makeModel(
        [
          {
            id: 's1',
            range: { from: 0, to: 100 },
            kind: 'select',
            ctes: [],
            relations: [
              {
                relation: makeIdentity('orders'),
                alias: 'o',
                sourceRange: { from: 0, to: 100 },
                sourceKind: 'table',
              },
            ],
            projectionAliases: [],
          },
        ],
        {
          kind: 'qualified_column',
          prefix: 'o.',
          replacementRange: { from: 3, to: 3 },
          qualifierParts: ['o'],
        },
      );
      const completions = resolveAliasDotCompletions(['o'], model, snapshot, adapter);
      expect(completions).not.toBeNull();
      const labels = completions!.map((c) => c.label);
      expect(labels).toContain('id'); // safe -> unquoted
      expect(labels).toContain('"order"'); // reserved keyword -> quoted!
      expect(labels).toContain('"user"'); // reserved keyword -> quoted!
    });

    it('includes type and nullable info in detail', () => {
      const users = makeRelation('users', [
        { name: 'id', dataType: 'integer', nullable: false },
        { name: 'name', dataType: 'varchar', nullable: true },
      ]);
      const snapshot = makeSnapshot([users]);

      const model = makeModel(
        [
          {
            id: 's1',
            range: { from: 0, to: 100 },
            kind: 'select',
            ctes: [],
            relations: [
              {
                relation: makeIdentity('users'),
                alias: 'u',
                sourceRange: { from: 0, to: 100 },
                sourceKind: 'table',
              },
            ],
            projectionAliases: [],
          },
        ],
        {
          kind: 'qualified_column',
          prefix: 'u.',
          replacementRange: { from: 3, to: 3 },
          qualifierParts: ['u'],
        },
      );

      const completions = resolveAliasDotCompletions(['u'], model, snapshot, adapter);
      expect(completions).not.toBeNull();
      const nameCol = completions!.find((c) => c.label === 'name');
      expect(nameCol!.detail).toContain('varchar');
      expect(nameCol!.detail).toContain('nullable');
    });

    it('includes comment in detail when present', () => {
      const users = makeRelation('users', [
        { name: 'email', dataType: 'varchar', nullable: false, comment: 'User email' },
      ]);
      const snapshot = makeSnapshot([users]);

      const model = makeModel(
        [
          {
            id: 's1',
            range: { from: 0, to: 100 },
            kind: 'select',
            ctes: [],
            relations: [
              {
                relation: makeIdentity('users'),
                alias: 'u',
                sourceRange: { from: 0, to: 100 },
                sourceKind: 'table',
              },
            ],
            projectionAliases: [],
          },
        ],
        {
          kind: 'qualified_column',
          prefix: 'u.',
          replacementRange: { from: 3, to: 3 },
          qualifierParts: ['u'],
        },
      );

      const completions = resolveAliasDotCompletions(['u'], model, snapshot, adapter);
      expect(completions).not.toBeNull();
      const emailCol = completions!.find((c) => c.label === 'email');
      expect(emailCol!.detail).toContain('User email');
    });

    it('returns null for ambiguous alias', () => {
      const users1 = makeRelation('users', [{ name: 'id', dataType: 'integer', nullable: false }]);
      const users2 = makeRelation(
        'users',
        [{ name: 'id', dataType: 'bigint', nullable: false }],
        ['other_schema'],
      );
      const snapshot = makeSnapshot([users1, users2]);

      const model = makeModel(
        [
          {
            id: 's1',
            range: { from: 0, to: 100 },
            kind: 'select',
            ctes: [],
            relations: [
              {
                relation: makeIdentity('users'),
                sourceRange: { from: 0, to: 100 },
                sourceKind: 'table',
              },
              {
                relation: makeIdentity('users', ['other_schema']),
                sourceRange: { from: 0, to: 100 },
                sourceKind: 'table',
              },
            ],
            projectionAliases: [],
          },
        ],
        {
          kind: 'qualified_column',
          prefix: 'u.',
          replacementRange: { from: 3, to: 3 },
          qualifierParts: ['u'],
        },
      );

      // Both have the same bare name 'users' so the alias resolution finds
      // two matches in the same scope.
      const completions = resolveAliasDotCompletions(['users'], model, snapshot, adapter);
      // Should return null because the alias is ambiguous (two bindings match).
      expect(completions).toBeNull();
    });

    it('returns null for CTE alias (no metadata)', () => {
      const snapshot = makeSnapshot([]);

      const model = makeModel(
        [
          {
            id: 's1',
            range: { from: 0, to: 100 },
            kind: 'select',
            ctes: [{ name: 'cte', nameRange: { from: 5, to: 8 }, queryScopeId: 's2' }],
            relations: [],
            projectionAliases: [],
          },
        ],
        {
          kind: 'qualified_column',
          prefix: 'cte.',
          replacementRange: { from: 9, to: 9 },
          qualifierParts: ['cte'],
        },
      );

      // CTE alias resolves, but no metadata in snapshot → null.
      const completions = resolveAliasDotCompletions(['cte'], model, snapshot, adapter);
      expect(completions).toBeNull();
    });

    it('returns null when metadata missing for the resolved relation', () => {
      const snapshot = makeSnapshot([]);

      const model = makeModel(
        [
          {
            id: 's1',
            range: { from: 0, to: 100 },
            kind: 'select',
            ctes: [],
            relations: [
              {
                relation: makeIdentity('users'),
                alias: 'u',
                sourceRange: { from: 0, to: 100 },
                sourceKind: 'table',
              },
            ],
            projectionAliases: [],
          },
        ],
        {
          kind: 'qualified_column',
          prefix: 'u.',
          replacementRange: { from: 3, to: 3 },
          qualifierParts: ['u'],
        },
      );

      const completions = resolveAliasDotCompletions(['u'], model, snapshot, adapter);
      expect(completions).toBeNull();
    });
  });

  describe('produceSchemaCompletions', () => {
    it('produces relation completions for join_target intent', () => {
      const orders = makeRelation('orders', []);
      const customers = makeRelation('customers', []);
      const snapshot = makeSnapshot([orders, customers]);

      const model = makeModel(
        [
          {
            id: 's1',
            range: { from: 0, to: 100 },
            kind: 'select',
            ctes: [],
            relations: [],
            projectionAliases: [],
          },
        ],
        {
          kind: 'join_target',
          prefix: '',
          replacementRange: { from: 50, to: 50 },
          qualifierParts: [],
        },
      );

      const completions = produceSchemaCompletions({ model, snapshot, adapter });
      expect(completions.length).toBe(2);
      const labels = completions.map((c) => c.label);
      expect(labels).toContain('"orders"');
      expect(labels).toContain('"customers"');
    });

    it('returns empty for ambiguous qualifier', () => {
      const users1 = makeRelation('users', [{ name: 'id', dataType: 'integer', nullable: false }]);
      const users2 = makeRelation(
        'users',
        [{ name: 'id', dataType: 'bigint', nullable: false }],
        ['other_schema'],
      );
      const snapshot = makeSnapshot([users1, users2]);

      const model = makeModel(
        [
          {
            id: 's1',
            range: { from: 0, to: 100 },
            kind: 'select',
            ctes: [],
            relations: [
              {
                relation: makeIdentity('users'),
                sourceRange: { from: 0, to: 100 },
                sourceKind: 'table',
              },
              {
                relation: makeIdentity('users', ['other_schema']),
                sourceRange: { from: 0, to: 100 },
                sourceKind: 'table',
              },
            ],
            projectionAliases: [],
          },
        ],
        {
          kind: 'qualified_column',
          prefix: 'users.',
          replacementRange: { from: 6, to: 6 },
          qualifierParts: ['users'],
        },
      );

      const completions = produceSchemaCompletions({ model, snapshot, adapter });
      expect(completions).toHaveLength(0);
    });
  });

  describe('extractSelectColumns', () => {
    it('returns empty for empty tokens', () => {
      const model = makeModel([], {
        kind: 'projection',
        prefix: '',
        replacementRange: { from: 0, to: 0 },
        qualifierParts: [],
      });
      expect(extractSelectColumns(model)).toEqual([]);
    });

    it('extracts bare column names from SELECT', () => {
      const tokens: SqlToken[] = [
        tok(SqlTokenKind.Other, 'SELECT', 0),
        tok(SqlTokenKind.Whitespace, ' ', 6),
        tok(SqlTokenKind.Other, 'id', 7),
        tok(SqlTokenKind.Other, ',', 9),
        tok(SqlTokenKind.Whitespace, ' ', 10),
        tok(SqlTokenKind.Other, 'name', 11),
        tok(SqlTokenKind.Whitespace, ' ', 15),
        tok(SqlTokenKind.Other, 'FROM', 16),
      ];
      const model = makeModel(
        [],
        {
          kind: 'relation',
          prefix: '',
          replacementRange: { from: 21, to: 21 },
          qualifierParts: [],
        },
        tokens,
      );
      expect(extractSelectColumns(model)).toEqual(['id', 'name']);
    });

    it('skips AS keyword and aliases', () => {
      const tokens: SqlToken[] = [
        tok(SqlTokenKind.Other, 'SELECT', 0),
        tok(SqlTokenKind.Whitespace, ' ', 6),
        tok(SqlTokenKind.Other, 'id', 7),
        tok(SqlTokenKind.Whitespace, ' ', 9),
        tok(SqlTokenKind.Other, 'AS', 10),
        tok(SqlTokenKind.Whitespace, ' ', 12),
        tok(SqlTokenKind.Other, 'user_id', 13),
        tok(SqlTokenKind.Other, ',', 20),
        tok(SqlTokenKind.Whitespace, ' ', 21),
        tok(SqlTokenKind.Other, 'name', 22),
        tok(SqlTokenKind.Whitespace, ' ', 26),
        tok(SqlTokenKind.Other, 'FROM', 27),
      ];
      const model = makeModel(
        [],
        {
          kind: 'relation',
          prefix: '',
          replacementRange: { from: 32, to: 32 },
          qualifierParts: [],
        },
        tokens,
      );
      expect(extractSelectColumns(model)).toEqual(['id', 'name']);
    });

    it('skips function calls like count(*)', () => {
      const tokens: SqlToken[] = [
        tok(SqlTokenKind.Other, 'SELECT', 0),
        tok(SqlTokenKind.Whitespace, ' ', 6),
        tok(SqlTokenKind.Other, 'count', 7),
        tok(SqlTokenKind.OpenParen, '(', 12),
        tok(SqlTokenKind.Other, '*', 13),
        tok(SqlTokenKind.CloseParen, ')', 14),
        tok(SqlTokenKind.Whitespace, ' ', 15),
        tok(SqlTokenKind.Other, 'FROM', 16),
      ];
      const model = makeModel(
        [],
        {
          kind: 'relation',
          prefix: '',
          replacementRange: { from: 21, to: 21 },
          qualifierParts: [],
        },
        tokens,
      );
      expect(extractSelectColumns(model)).toEqual([]);
    });

    it('extracts double-quoted column names', () => {
      const tokens: SqlToken[] = [
        tok(SqlTokenKind.Other, 'SELECT', 0),
        tok(SqlTokenKind.Whitespace, ' ', 6),
        tok(SqlTokenKind.DoubleQuoted, '"userId"', 7),
        tok(SqlTokenKind.Whitespace, ' ', 15),
        tok(SqlTokenKind.Other, 'FROM', 16),
      ];
      const model = makeModel(
        [],
        {
          kind: 'relation',
          prefix: '',
          replacementRange: { from: 21, to: 21 },
          qualifierParts: [],
        },
        tokens,
      );
      expect(extractSelectColumns(model)).toEqual(['userid']);
    });

    it('stops at WHERE clause', () => {
      const tokens: SqlToken[] = [
        tok(SqlTokenKind.Other, 'SELECT', 0),
        tok(SqlTokenKind.Whitespace, ' ', 6),
        tok(SqlTokenKind.Other, 'id', 7),
        tok(SqlTokenKind.Whitespace, ' ', 9),
        tok(SqlTokenKind.Other, 'FROM', 10),
        tok(SqlTokenKind.Whitespace, ' ', 14),
        tok(SqlTokenKind.Other, 'users', 15),
        tok(SqlTokenKind.Whitespace, ' ', 20),
        tok(SqlTokenKind.Other, 'WHERE', 21),
      ];
      const model = makeModel(
        [],
        {
          kind: 'relation',
          prefix: '',
          replacementRange: { from: 27, to: 27 },
          qualifierParts: [],
        },
        tokens,
      );
      expect(extractSelectColumns(model)).toEqual(['id']);
    });

    it('returns empty when no SELECT keyword', () => {
      const tokens: SqlToken[] = [
        tok(SqlTokenKind.Other, 'INSERT', 0),
        tok(SqlTokenKind.Whitespace, ' ', 6),
        tok(SqlTokenKind.Other, 'id', 7),
      ];
      const model = makeModel(
        [],
        { kind: 'unknown', prefix: '', replacementRange: { from: 0, to: 0 }, qualifierParts: [] },
        tokens,
      );
      expect(extractSelectColumns(model)).toEqual([]);
    });

    it('handles DISTINCT keyword', () => {
      const tokens: SqlToken[] = [
        tok(SqlTokenKind.Other, 'SELECT', 0),
        tok(SqlTokenKind.Whitespace, ' ', 6),
        tok(SqlTokenKind.Other, 'DISTINCT', 7),
        tok(SqlTokenKind.Whitespace, ' ', 15),
        tok(SqlTokenKind.Other, 'col1', 16),
        tok(SqlTokenKind.Whitespace, ' ', 20),
        tok(SqlTokenKind.Other, 'FROM', 21),
      ];
      const model = makeModel(
        [],
        {
          kind: 'relation',
          prefix: '',
          replacementRange: { from: 26, to: 26 },
          qualifierParts: [],
        },
        tokens,
      );
      expect(extractSelectColumns(model)).toEqual(['col1']);
    });
  });

  describe('SELECT col1 projection completion', () => {
    it('detects projection intent and returns columns (not tables) for SELECT col1', () => {
      const users = makeRelation('users', [
        { name: 'col1', dataType: 'integer', nullable: false },
        { name: 'col2', dataType: 'varchar', nullable: true },
      ]);
      const orders = makeRelation('orders', [
        { name: 'col1', dataType: 'integer', nullable: false },
        { name: 'total', dataType: 'decimal', nullable: false },
      ]);
      const snapshot = makeSnapshot([users, orders]);

      const sql = 'SELECT col1';
      const model = buildSemanticModel(sql, sql.length, { dialectId: 'postgresql' });
      expect(model.cursorIntent.kind).toBe('projection');

      const completions = produceSchemaCompletions({ model, snapshot, adapter });
      expect(completions.length).toBeGreaterThan(0);
      expect(completions.every((c) => c.type === 'property')).toBe(true);
      expect(completions.some((c) => c.label === 'col1')).toBe(true);
      expect(completions.some((c) => c.type === 'type')).toBe(false);
    });

    it('column completions include filterText for unquoted prefix matching', () => {
      const users = makeRelation('users', [{ name: 'col1', dataType: 'integer', nullable: false }]);
      const snapshot = makeSnapshot([users]);

      const sql = 'SELECT col1';
      const model = buildSemanticModel(sql, sql.length, { dialectId: 'postgresql' });
      const completions = produceSchemaCompletions({ model, snapshot, adapter });

      const col1 = completions.find((c) => c.label === 'col1');
      expect(col1).toBeDefined();
      expect(col1!.filterText).toBe('col1');
    });

    it('works with lowercase select keyword', () => {
      const users = makeRelation('users', [{ name: 'col1', dataType: 'integer', nullable: false }]);
      const snapshot = makeSnapshot([users]);

      const sql = 'select col1';
      const model = buildSemanticModel(sql, sql.length, { dialectId: 'postgresql' });
      expect(model.cursorIntent.kind).toBe('projection');

      const completions = produceSchemaCompletions({ model, snapshot, adapter });
      expect(completions.every((c) => c.type === 'property')).toBe(true);
    });
  });

  describe('allColumnsFromSnapshot (projection without FROM)', () => {
    it('returns ALL columns from ALL tables when no visible relations', () => {
      const users = makeRelation('users', [
        { name: 'id', dataType: 'integer', nullable: false },
        { name: 'name', dataType: 'varchar', nullable: true },
      ]);
      const orders = makeRelation('orders', [
        { name: 'id', dataType: 'integer', nullable: false },
        { name: 'total', dataType: 'decimal', nullable: false },
      ]);
      const snapshot = makeSnapshot([users, orders]);

      // Empty scopes → no visible relations → fallback to allColumnsFromSnapshot
      const model = makeModel([], {
        kind: 'projection',
        prefix: '',
        replacementRange: { from: 7, to: 7 },
        qualifierParts: [],
      });

      // Default 'unquoted'
      const completions = produceSchemaCompletions({ model, snapshot, adapter });
      expect(completions.length).toBe(4);

      const labels = completions.map((c) => c.label);
      expect(labels).toContain('id');
      expect(labels).toContain('name');
      expect(labels).toContain('total');

      // Each completion should have the table-qualified apply text
      const idCompletions = completions.filter((c) => c.label === 'id');
      expect(idCompletions.length).toBe(2); // one from users, one from orders
      expect(idCompletions[0]!.apply).toContain('"users"');
      expect(idCompletions[1]!.apply).toContain('"orders"');

      // When quotePolicy is 'both'
      const bothCompletions = produceSchemaCompletions({
        model,
        snapshot,
        adapter,
        quotePolicy: 'both',
      });
      expect(bothCompletions.length).toBe(8);
      expect(bothCompletions.map((c) => c.label)).toContain('"id"');
      expect(bothCompletions.map((c) => c.label)).toContain('id');
    });

    it('returns empty when snapshot has no relations', () => {
      const snapshot = makeSnapshot([]);
      const model = makeModel([], {
        kind: 'projection',
        prefix: '',
        replacementRange: { from: 7, to: 7 },
        qualifierParts: [],
      });

      const completions = produceSchemaCompletions({ model, snapshot, adapter });
      expect(completions).toHaveLength(0);
    });

    it('skips relations with no columns', () => {
      const empty = makeRelation('empty_table', []);
      const users = makeRelation('users', [{ name: 'id', dataType: 'integer', nullable: false }]);
      const snapshot = makeSnapshot([empty, users]);

      const model = makeModel([], {
        kind: 'projection',
        prefix: '',
        replacementRange: { from: 7, to: 7 },
        qualifierParts: [],
      });

      const completions = produceSchemaCompletions({ model, snapshot, adapter });
      expect(completions.length).toBe(1);
      expect(completions.map((c) => c.label)).toContain('id');
    });

    it('falls back to editor schema when snapshot is empty and schema provided', () => {
      const snapshot = makeSnapshot([]);
      const model = makeModel([], {
        kind: 'projection',
        prefix: '',
        replacementRange: { from: 7, to: 7 },
        qualifierParts: [],
      });

      // Flat schema: table names are keys, column names are array values
      const schema: Record<string, string[]> = {
        users: ['id', 'name', 'email'],
        orders: ['id', 'total', 'user_id'],
      };

      const completions = produceSchemaCompletions({ model, snapshot, adapter, schema });
      expect(completions.length).toBe(6);

      const labels = completions.map((c) => c.label);
      expect(labels).toContain('id');
      expect(labels).toContain('name');
      expect(labels).toContain('email');
      expect(labels).toContain('total');
      expect(labels).toContain('user_id');

      // Each column should have table-qualified apply text
      const idCompletions = completions.filter((c) => c.label === 'id');
      expect(idCompletions.length).toBe(2);
      expect(idCompletions[0]!.apply).toBe('"users".id');
      expect(idCompletions[1]!.apply).toBe('"orders".id');

      // Detail should show the table name
      expect(idCompletions[0]!.detail).toBe('users');
      expect(idCompletions[1]!.detail).toBe('orders');
    });

    it('handles nested schema (with schema namespace)', () => {
      const snapshot = makeSnapshot([]);
      const model = makeModel([], {
        kind: 'projection',
        prefix: '',
        replacementRange: { from: 7, to: 7 },
        qualifierParts: [],
      });

      // Nested schema: schema → table → columns
      const schema: Record<string, Record<string, string[]>> = {
        public: {
          users: ['id', 'name'],
          orders: ['id', 'total'],
        },
        analytics: {
          events: ['event_id', 'payload'],
        },
      };

      const completions = produceSchemaCompletions({ model, snapshot, adapter, schema });
      expect(completions.length).toBe(6);

      const labels = completions.map((c) => c.label);
      expect(labels).toContain('id');
      expect(labels).toContain('name');
      expect(labels).toContain('total');
      expect(labels).toContain('event_id');
      expect(labels).toContain('payload');

      // Detail should show the table name (not the schema)
      const nameCol = completions.find((c) => c.label === 'name');
      expect(nameCol!.detail).toBe('users');
    });

    it('does not use schema fallback when snapshot has data', () => {
      const users = makeRelation('users', [{ name: 'id', dataType: 'integer', nullable: false }]);
      const snapshot = makeSnapshot([users]);
      const model = makeModel([], {
        kind: 'projection',
        prefix: '',
        replacementRange: { from: 7, to: 7 },
        qualifierParts: [],
      });

      const schema: Record<string, string[]> = {
        users: ['id', 'name', 'email'],
        orders: ['id', 'total'],
      };

      // Snapshot takes precedence — only 1 unquoted entry for 1 column from snapshot, not from schema
      const completions = produceSchemaCompletions({ model, snapshot, adapter, schema });
      expect(completions.length).toBe(1);
      expect(completions.map((c) => c.label)).toContain('id');
    });
  });

  describe('FROM hint (relation with SELECT columns)', () => {
    it('suggests matching tables first when SELECT columns match', () => {
      const users = makeRelation('users', [
        { name: 'id', dataType: 'integer', nullable: false },
        { name: 'name', dataType: 'varchar', nullable: true },
      ]);
      const orders = makeRelation('orders', [
        { name: 'id', dataType: 'integer', nullable: false },
        { name: 'total', dataType: 'decimal', nullable: false },
      ]);
      const snapshot = makeSnapshot([users, orders]);

      // Tokens: SELECT id, name  (cursor is at FROM position)
      const tokens: SqlToken[] = [
        tok(SqlTokenKind.Other, 'SELECT', 0),
        tok(SqlTokenKind.Whitespace, ' ', 6),
        tok(SqlTokenKind.Other, 'id', 7),
        tok(SqlTokenKind.Other, ',', 9),
        tok(SqlTokenKind.Whitespace, ' ', 10),
        tok(SqlTokenKind.Other, 'name', 11),
        tok(SqlTokenKind.Whitespace, ' ', 15),
      ];

      const model = makeModel(
        [],
        {
          kind: 'relation',
          prefix: '',
          replacementRange: { from: 16, to: 16 },
          qualifierParts: [],
        },
        tokens,
      );

      const completions = produceSchemaCompletions({ model, snapshot, adapter });
      // users should be first (has both id AND name), orders should be second
      expect(completions.length).toBe(2);
      expect(completions[0]!.label).toBe('"users"');
      expect(completions[0]!.detail).toContain('matched columns');
      expect(completions[1]!.label).toBe('"orders"');
    });

    it('falls through to full table list when no columns match', () => {
      const users = makeRelation('users', [{ name: 'id', dataType: 'integer', nullable: false }]);
      const orders = makeRelation('orders', [
        { name: 'total', dataType: 'decimal', nullable: false },
      ]);
      const snapshot = makeSnapshot([users, orders]);

      // Tokens: SELECT nonexistent  (no table has "nonexistent")
      const tokens: SqlToken[] = [
        tok(SqlTokenKind.Other, 'SELECT', 0),
        tok(SqlTokenKind.Whitespace, ' ', 6),
        tok(SqlTokenKind.Other, 'nonexistent', 7),
        tok(SqlTokenKind.Whitespace, ' ', 18),
      ];

      const model = makeModel(
        [],
        {
          kind: 'relation',
          prefix: '',
          replacementRange: { from: 19, to: 19 },
          qualifierParts: [],
        },
        tokens,
      );

      const completions = produceSchemaCompletions({ model, snapshot, adapter });
      // Should show all tables since no table matches all columns
      expect(completions.length).toBe(2);
      const labels = completions.map((c) => c.label);
      expect(labels).toContain('"users"');
      expect(labels).toContain('"orders"');
    });

    it('shows all tables when no SELECT columns found', () => {
      const users = makeRelation('users', []);
      const orders = makeRelation('orders', []);
      const snapshot = makeSnapshot([users, orders]);

      // No tokens → extractSelectColumns returns []
      const model = makeModel([], {
        kind: 'relation',
        prefix: '',
        replacementRange: { from: 0, to: 0 },
        qualifierParts: [],
      });

      const completions = produceSchemaCompletions({ model, snapshot, adapter });
      expect(completions.length).toBe(2);
    });

    it('suggests table aliases in projection context (e.g. after ON)', () => {
      const orders = makeRelation('er_orders', [
        { name: 'id', dataType: 'integer', nullable: false },
        { name: 'customer_id', dataType: 'integer', nullable: false },
      ]);
      const customers = makeRelation('er_customers', [
        { name: 'id', dataType: 'integer', nullable: false },
        { name: 'name', dataType: 'varchar', nullable: true },
      ]);
      const snapshot = makeSnapshot([orders, customers]);

      const model = makeModel(
        [
          {
            id: 's1',
            range: { from: 0, to: 100 },
            kind: 'select',
            ctes: [],
            relations: [
              {
                relation: makeIdentity('er_orders'),
                alias: 'eo',
                sourceRange: { from: 20, to: 34 },
                sourceKind: 'table',
              },
              {
                relation: makeIdentity('er_customers'),
                alias: 'ec',
                sourceRange: { from: 45, to: 60 },
                sourceKind: 'table',
              },
            ],
            projectionAliases: [],
          },
        ],
        {
          kind: 'projection',
          prefix: 'e',
          replacementRange: { from: 65, to: 66 },
          qualifierParts: [],
        },
      );

      const completions = produceSchemaCompletions({ model, snapshot, adapter });
      const labels = completions.map((c) => c.label);
      expect(labels).toContain('eo');
      expect(labels).toContain('ec');
      const eoItem = completions.find((c) => c.label === 'eo');
      expect(eoItem?.type).toBe('variable');
      expect(eoItem?.boost).toBe(8);
    });

    it('assigns high boost (10) to relations in relation context and falls back to options.schema', () => {
      // Empty snapshot, but schema tree has er_customers and er_orders
      const snapshot = makeSnapshot([]);
      const schema = {
        public: {
          er_customers: ['id', 'name'],
          er_orders: ['id', 'order_no'],
        },
      };

      const model = makeModel([], {
        kind: 'relation',
        prefix: 'ec',
        replacementRange: { from: 14, to: 16 },
        qualifierParts: [],
      });

      const completions = produceSchemaCompletions({ model, snapshot, adapter, schema });
      const labels = completions.map((c) => c.label);
      expect(labels).toContain('"er_customers"');
      expect(labels).toContain('"er_orders"');
      expect(labels).toContain('"public"');

      const customerItem = completions.find((c) => c.label === '"er_customers"');
      expect(customerItem?.boost).toBe(10);
      expect(customerItem?.type).toBe('type');
    });

    it('suggests columns when typing "c." at end of JOIN ON condition', () => {
      const sql = 'SELECT * FROM er_orders o LEFT JOIN er_customers c ON o."customer_id"=c.';
      const model = buildSemanticModel(sql, sql.length, { dialectId: 'postgresql' });

      const erOrders = makeRelation('er_orders', [
        { name: 'id', dataType: 'integer', nullable: false },
        { name: 'customer_id', dataType: 'integer', nullable: false },
      ]);
      const erCustomers = makeRelation('er_customers', [
        { name: 'id', dataType: 'integer', nullable: false },
        { name: 'name', dataType: 'varchar', nullable: false },
        { name: 'city', dataType: 'varchar', nullable: true },
      ]);
      const snapshot = makeSnapshot([erOrders, erCustomers]);

      const completions = produceSchemaCompletions({ model, snapshot, adapter });
      expect(completions.length).toBe(3);
      expect(completions.map((c) => c.label)).toContain('id');
      expect(completions.map((c) => c.label)).toContain('name');
      expect(completions.map((c) => c.label)).toContain('city');

      const bothCompletions = produceSchemaCompletions({
        model,
        snapshot,
        adapter,
        quotePolicy: 'both',
      });
      expect(bothCompletions.length).toBe(6);
      expect(bothCompletions.map((c) => c.label)).toContain('"id"');
      expect(bothCompletions.map((c) => c.label)).toContain('id');
    });
  });
});
