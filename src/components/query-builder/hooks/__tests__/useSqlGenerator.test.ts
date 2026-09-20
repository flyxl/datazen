import { describe, it, expect } from 'vitest';
import { generateSql } from '../useSqlGenerator';
import type { GenerateSqlInput } from '../useSqlGenerator';
import type { QbConditionGroup } from '../../types';

// ── Helpers ───────────────────────────────────────────────────

function emptyGroup(id = 'root'): QbConditionGroup {
  return { id, logic: 'AND', conditions: [], groups: [] };
}

function baseInput(overrides: Partial<GenerateSqlInput> = {}): GenerateSqlInput {
  return {
    selectedTables: [],
    selectedColumns: [],
    joins: [],
    tableAliases: {},
    where: emptyGroup(),
    orderBy: [],
    groupBy: [],
    distinct: false,
    limit: null,
    offset: null,
    ...overrides,
  };
}

// ── Tests ─────────────────────────────────────────────────────

describe('generateSql', () => {
  // ── Empty state ────────────────────────────────────────────

  it('returns empty string when no tables selected', () => {
    const sql = generateSql(baseInput());
    expect(sql).toBe('');
  });

  it('returns empty string when tables selected but no columns', () => {
    const sql = generateSql(baseInput({ selectedTables: ['users'] }));
    expect(sql).toBe('');
  });

  // ── Single-table SELECT ────────────────────────────────────

  it('generates simple single-table SELECT', () => {
    const sql = generateSql(
      baseInput({
        selectedTables: ['users'],
        selectedColumns: [
          { table: 'users', column: 'id' },
          { table: 'users', column: 'name' },
        ],
      }),
    );
    expect(sql).toBe('SELECT "users"."id", "users"."name" FROM "users";');
  });

  // ── Multi-column with alias ────────────────────────────────

  it('generates SELECT with column alias', () => {
    const sql = generateSql(
      baseInput({
        selectedTables: ['users'],
        selectedColumns: [{ table: 'users', column: 'name', alias: 'user_name' }],
      }),
    );
    expect(sql).toBe('SELECT "users"."name" AS "user_name" FROM "users";');
  });

  // ── Aggregate functions ────────────────────────────────────

  it('generates SELECT with COUNT aggregate', () => {
    const sql = generateSql(
      baseInput({
        selectedTables: ['orders'],
        selectedColumns: [
          { table: 'orders', column: 'id', aggregate: 'COUNT', alias: 'order_count' },
        ],
      }),
    );
    expect(sql).toBe('SELECT COUNT("orders"."id") AS "order_count" FROM "orders";');
  });

  it('generates SELECT with SUM aggregate', () => {
    const sql = generateSql(
      baseInput({
        selectedTables: ['orders'],
        selectedColumns: [
          { table: 'orders', column: 'total', aggregate: 'SUM', alias: 'sum_total' },
        ],
      }),
    );
    expect(sql).toBe('SELECT SUM("orders"."total") AS "sum_total" FROM "orders";');
  });

  it('generates SELECT with AVG aggregate', () => {
    const sql = generateSql(
      baseInput({
        selectedTables: ['orders'],
        selectedColumns: [{ table: 'orders', column: 'total', aggregate: 'AVG' }],
      }),
    );
    expect(sql).toBe('SELECT AVG("orders"."total") FROM "orders";');
  });

  // ── WHERE single conditions ────────────────────────────────

  it('generates WHERE with equals condition', () => {
    const where = emptyGroup();
    where.conditions = [
      {
        id: '1',
        table: 'users',
        column: 'name',
        operator: '=',
        value: 'Alice',
        conjunction: 'AND',
      },
    ];
    const sql = generateSql(
      baseInput({
        selectedTables: ['users'],
        selectedColumns: [{ table: 'users', column: '*' }],
        where,
      }),
    );
    expect(sql).toBe('SELECT "users"."*" FROM "users" WHERE "users"."name" = \'Alice\';');
  });

  it('generates WHERE with not-equals condition', () => {
    const where = emptyGroup();
    where.conditions = [
      {
        id: '1',
        table: 'users',
        column: 'status',
        operator: '!=',
        value: 'inactive',
        conjunction: 'AND',
      },
    ];
    const sql = generateSql(
      baseInput({
        selectedTables: ['users'],
        selectedColumns: [{ table: 'users', column: 'id' }],
        where,
      }),
    );
    expect(sql).toBe('SELECT "users"."id" FROM "users" WHERE "users"."status" != \'inactive\';');
  });

  it('generates WHERE with greater-than condition', () => {
    const where = emptyGroup();
    where.conditions = [
      { id: '1', table: 'users', column: 'age', operator: '>', value: '18', conjunction: 'AND' },
    ];
    const sql = generateSql(
      baseInput({
        selectedTables: ['users'],
        selectedColumns: [{ table: 'users', column: 'name' }],
        where,
      }),
    );
    expect(sql).toBe('SELECT "users"."name" FROM "users" WHERE "users"."age" > 18;');
  });

  it('generates WHERE with less-than condition', () => {
    const where = emptyGroup();
    where.conditions = [
      { id: '1', table: 'users', column: 'age', operator: '<', value: '65', conjunction: 'AND' },
    ];
    const sql = generateSql(
      baseInput({
        selectedTables: ['users'],
        selectedColumns: [{ table: 'users', column: 'name' }],
        where,
      }),
    );
    expect(sql).toBe('SELECT "users"."name" FROM "users" WHERE "users"."age" < 65;');
  });

  it('generates WHERE with LIKE condition', () => {
    const where = emptyGroup();
    where.conditions = [
      {
        id: '1',
        table: 'users',
        column: 'name',
        operator: 'LIKE',
        value: '%test%',
        conjunction: 'AND',
      },
    ];
    const sql = generateSql(
      baseInput({
        selectedTables: ['users'],
        selectedColumns: [{ table: 'users', column: 'name' }],
        where,
      }),
    );
    expect(sql).toBe('SELECT "users"."name" FROM "users" WHERE "users"."name" LIKE \'%test%\';');
  });

  it('generates WHERE with IN condition', () => {
    const where = emptyGroup();
    where.conditions = [
      {
        id: '1',
        table: 'users',
        column: 'id',
        operator: 'IN',
        value: '1, 2, 3',
        conjunction: 'AND',
      },
    ];
    const sql = generateSql(
      baseInput({
        selectedTables: ['users'],
        selectedColumns: [{ table: 'users', column: 'name' }],
        where,
      }),
    );
    expect(sql).toBe('SELECT "users"."name" FROM "users" WHERE "users"."id" IN (1, 2, 3);');
  });

  it('generates WHERE with IS NULL condition', () => {
    const where = emptyGroup();
    where.conditions = [
      {
        id: '1',
        table: 'users',
        column: 'email',
        operator: 'IS NULL',
        value: null,
        conjunction: 'AND',
      },
    ];
    const sql = generateSql(
      baseInput({
        selectedTables: ['users'],
        selectedColumns: [{ table: 'users', column: 'name' }],
        where,
      }),
    );
    expect(sql).toBe('SELECT "users"."name" FROM "users" WHERE "users"."email" IS NULL;');
  });

  // ── WHERE AND/OR combinations ──────────────────────────────

  it('generates WHERE with AND combination', () => {
    const where = emptyGroup();
    where.conditions = [
      { id: '1', table: 'users', column: 'age', operator: '>', value: '18', conjunction: 'AND' },
      {
        id: '2',
        table: 'users',
        column: 'status',
        operator: '=',
        value: 'active',
        conjunction: 'AND',
      },
    ];
    const sql = generateSql(
      baseInput({
        selectedTables: ['users'],
        selectedColumns: [{ table: 'users', column: 'name' }],
        where,
      }),
    );
    expect(sql).toBe(
      'SELECT "users"."name" FROM "users" WHERE "users"."age" > 18 AND "users"."status" = \'active\';',
    );
  });

  it('generates WHERE with OR combination', () => {
    const where = emptyGroup('root');
    where.logic = 'OR';
    where.conditions = [
      { id: '1', table: 'users', column: 'role', operator: '=', value: 'admin', conjunction: 'OR' },
      {
        id: '2',
        table: 'users',
        column: 'role',
        operator: '=',
        value: 'superadmin',
        conjunction: 'OR',
      },
    ];
    const sql = generateSql(
      baseInput({
        selectedTables: ['users'],
        selectedColumns: [{ table: 'users', column: 'name' }],
        where,
      }),
    );
    expect(sql).toBe(
      'SELECT "users"."name" FROM "users" WHERE "users"."role" = \'admin\' OR "users"."role" = \'superadmin\';',
    );
  });

  // ── Nested condition groups ────────────────────────────────

  it('generates WHERE with nested condition groups', () => {
    const subGroup: QbConditionGroup = {
      id: 'sub',
      logic: 'OR',
      conditions: [
        {
          id: '3',
          table: 'users',
          column: 'role',
          operator: '=',
          value: 'admin',
          conjunction: 'OR',
        },
        {
          id: '4',
          table: 'users',
          column: 'role',
          operator: '=',
          value: 'editor',
          conjunction: 'OR',
        },
      ],
      groups: [],
    };
    const where = emptyGroup('root');
    where.conditions = [
      { id: '1', table: 'users', column: 'active', operator: '=', value: '1', conjunction: 'AND' },
    ];
    where.groups = [subGroup];

    const sql = generateSql(
      baseInput({
        selectedTables: ['users'],
        selectedColumns: [{ table: 'users', column: 'name' }],
        where,
      }),
    );
    expect(sql).toBe(
      'SELECT "users"."name" FROM "users" WHERE "users"."active" = 1 AND ("users"."role" = \'admin\' OR "users"."role" = \'editor\');',
    );
  });

  // ── ORDER BY ───────────────────────────────────────────────

  it('generates ORDER BY single column', () => {
    const sql = generateSql(
      baseInput({
        selectedTables: ['users'],
        selectedColumns: [{ table: 'users', column: 'name' }],
        orderBy: [{ table: 'users', column: 'name', direction: 'ASC' }],
      }),
    );
    expect(sql).toBe('SELECT "users"."name" FROM "users" ORDER BY "users"."name" ASC;');
  });

  it('generates ORDER BY multiple columns', () => {
    const sql = generateSql(
      baseInput({
        selectedTables: ['users'],
        selectedColumns: [{ table: 'users', column: 'name' }],
        orderBy: [
          { table: 'users', column: 'name', direction: 'ASC' },
          { table: 'users', column: 'age', direction: 'DESC' },
        ],
      }),
    );
    expect(sql).toBe(
      'SELECT "users"."name" FROM "users" ORDER BY "users"."name" ASC, "users"."age" DESC;',
    );
  });

  // ── GROUP BY ───────────────────────────────────────────────

  it('generates GROUP BY with aggregate', () => {
    const sql = generateSql(
      baseInput({
        selectedTables: ['orders'],
        selectedColumns: [
          { table: 'orders', column: 'user_id' },
          { table: 'orders', column: 'total', aggregate: 'SUM', alias: 'sum_total' },
        ],
        groupBy: [{ table: 'orders', column: 'user_id' }],
      }),
    );
    expect(sql).toBe(
      'SELECT "orders"."user_id", SUM("orders"."total") AS "sum_total" FROM "orders" GROUP BY "orders"."user_id";',
    );
  });

  // ── DISTINCT ───────────────────────────────────────────────

  it('generates SELECT DISTINCT', () => {
    const sql = generateSql(
      baseInput({
        selectedTables: ['users'],
        selectedColumns: [{ table: 'users', column: 'country' }],
        distinct: true,
      }),
    );
    expect(sql).toBe('SELECT DISTINCT "users"."country" FROM "users";');
  });

  // ── Dialect differences ────────────────────────────────────

  it('generates PostgreSQL with double-quote identifiers', () => {
    const sql = generateSql(
      baseInput({
        selectedTables: ['users'],
        selectedColumns: [{ table: 'users', column: 'name' }],
        databaseType: 'postgresql',
      }),
    );
    expect(sql).toBe('SELECT "users"."name" FROM "users";');
  });

  it('generates MySQL with backtick identifiers', () => {
    const sql = generateSql(
      baseInput({
        selectedTables: ['users'],
        selectedColumns: [{ table: 'users', column: 'name' }],
        databaseType: 'mysql',
      }),
    );
    expect(sql).toBe('SELECT `users`.`name` FROM `users`;');
  });

  it('generates SQLite with double-quote identifiers', () => {
    const sql = generateSql(
      baseInput({
        selectedTables: ['users'],
        selectedColumns: [{ table: 'users', column: 'name' }],
        databaseType: 'sqlite',
      }),
    );
    expect(sql).toBe('SELECT "users"."name" FROM "users";');
  });

  it('generates SQL Server with bracket identifiers', () => {
    const sql = generateSql(
      baseInput({
        selectedTables: ['users'],
        selectedColumns: [{ table: 'users', column: 'name' }],
        databaseType: 'sqlserver',
      }),
    );
    expect(sql).toBe('SELECT [users].[name] FROM [users];');
  });

  // ── Special character escaping ─────────────────────────────

  it('escapes single quotes in values', () => {
    const where = emptyGroup();
    where.conditions = [
      {
        id: '1',
        table: 'users',
        column: 'name',
        operator: '=',
        value: "O'Brien",
        conjunction: 'AND',
      },
    ];
    const sql = generateSql(
      baseInput({
        selectedTables: ['users'],
        selectedColumns: [{ table: 'users', column: 'name' }],
        where,
      }),
    );
    expect(sql).toBe('SELECT "users"."name" FROM "users" WHERE "users"."name" = \'O\'\'Brien\';');
  });

  // ── Full complex query ─────────────────────────────────────

  it('generates a complete complex query', () => {
    const subGroup: QbConditionGroup = {
      id: 'sub',
      logic: 'OR',
      conditions: [
        {
          id: '3',
          table: 'users',
          column: 'role',
          operator: '=',
          value: 'admin',
          conjunction: 'OR',
        },
      ],
      groups: [],
    };
    const where = emptyGroup('root');
    where.conditions = [
      { id: '1', table: 'users', column: 'active', operator: '=', value: '1', conjunction: 'AND' },
    ];
    where.groups = [subGroup];

    const sql = generateSql(
      baseInput({
        selectedTables: ['users', 'orders'],
        selectedColumns: [
          { table: 'users', column: 'name' },
          { table: 'orders', column: 'total', aggregate: 'SUM', alias: 'sum_total' },
        ],
        where,
        orderBy: [{ table: 'users', column: 'name', direction: 'ASC' }],
        groupBy: [{ table: 'users', column: 'name' }],
        distinct: false,
        databaseType: 'postgresql',
      }),
    );
    expect(sql).toBe(
      'SELECT "users"."name", SUM("orders"."total") AS "sum_total" FROM "users" WHERE "users"."active" = 1 AND ("users"."role" = \'admin\') GROUP BY "users"."name" ORDER BY "users"."name" ASC;',
    );
  });

  // ── >= and <= operators ────────────────────────────────────

  it('generates WHERE with >= and <= operators', () => {
    const where = emptyGroup();
    where.conditions = [
      { id: '1', table: 'users', column: 'age', operator: '>=', value: '18', conjunction: 'AND' },
      { id: '2', table: 'users', column: 'age', operator: '<=', value: '65', conjunction: 'AND' },
    ];
    const sql = generateSql(
      baseInput({
        selectedTables: ['users'],
        selectedColumns: [{ table: 'users', column: 'name' }],
        where,
      }),
    );
    expect(sql).toBe(
      'SELECT "users"."name" FROM "users" WHERE "users"."age" >= 18 AND "users"."age" <= 65;',
    );
  });

  // ── NOT LIKE operator ──────────────────────────────────────

  it('generates WHERE with NOT LIKE', () => {
    const where = emptyGroup();
    where.conditions = [
      {
        id: '1',
        table: 'users',
        column: 'name',
        operator: 'NOT LIKE',
        value: '%admin%',
        conjunction: 'AND',
      },
    ];
    const sql = generateSql(
      baseInput({
        selectedTables: ['users'],
        selectedColumns: [{ table: 'users', column: 'name' }],
        where,
      }),
    );
    expect(sql).toBe(
      'SELECT "users"."name" FROM "users" WHERE "users"."name" NOT LIKE \'%admin%\';',
    );
  });

  // ── NOT IN operator ────────────────────────────────────────

  it('generates WHERE with NOT IN', () => {
    const where = emptyGroup();
    where.conditions = [
      {
        id: '1',
        table: 'users',
        column: 'id',
        operator: 'NOT IN',
        value: '1, 2',
        conjunction: 'AND',
      },
    ];
    const sql = generateSql(
      baseInput({
        selectedTables: ['users'],
        selectedColumns: [{ table: 'users', column: 'name' }],
        where,
      }),
    );
    expect(sql).toBe('SELECT "users"."name" FROM "users" WHERE "users"."id" NOT IN (1, 2);');
  });

  // ── IS NOT NULL operator ───────────────────────────────────

  it('generates WHERE with IS NOT NULL', () => {
    const where = emptyGroup();
    where.conditions = [
      {
        id: '1',
        table: 'users',
        column: 'email',
        operator: 'IS NOT NULL',
        value: null,
        conjunction: 'AND',
      },
    ];
    const sql = generateSql(
      baseInput({
        selectedTables: ['users'],
        selectedColumns: [{ table: 'users', column: 'name' }],
        where,
      }),
    );
    expect(sql).toBe('SELECT "users"."name" FROM "users" WHERE "users"."email" IS NOT NULL;');
  });

  // ── Empty value → NULL ─────────────────────────────────────

  it('generates NULL for empty string value', () => {
    const where = emptyGroup();
    where.conditions = [
      { id: '1', table: 'users', column: 'email', operator: '=', value: '', conjunction: 'AND' },
    ];
    const sql = generateSql(
      baseInput({
        selectedTables: ['users'],
        selectedColumns: [{ table: 'users', column: 'name' }],
        where,
      }),
    );
    expect(sql).toBe('SELECT "users"."name" FROM "users" WHERE "users"."email" = NULL;');
  });

  // ── Unknown database type falls back to generic ────────────

  it('falls back to generic adapter for unknown database type', () => {
    const sql = generateSql(
      baseInput({
        selectedTables: ['users'],
        selectedColumns: [{ table: 'users', column: 'name' }],
        databaseType: 'unknown_db',
      }),
    );
    expect(sql).toBe('SELECT "users"."name" FROM "users";');
  });

  // ── JOIN generation ──────────────────────────────────────────

  it('generates INNER JOIN', () => {
    const sql = generateSql(
      baseInput({
        selectedTables: ['users', 'orders'],
        selectedColumns: [
          { table: 'users', column: 'name' },
          { table: 'orders', column: 'total' },
        ],
        joins: [
          {
            id: 'j1',
            type: 'INNER',
            leftTable: 'users',
            leftColumn: 'id',
            rightTable: 'orders',
            rightColumn: 'user_id',
            isManual: true,
          },
        ],
      }),
    );
    expect(sql).toBe(
      'SELECT "users"."name", "orders"."total" FROM "users"\nINNER JOIN "orders" ON "users"."id" = "orders"."user_id";',
    );
  });

  it('generates LEFT JOIN', () => {
    const sql = generateSql(
      baseInput({
        selectedTables: ['users', 'orders'],
        selectedColumns: [
          { table: 'users', column: 'name' },
          { table: 'orders', column: 'total' },
        ],
        joins: [
          {
            id: 'j1',
            type: 'LEFT',
            leftTable: 'users',
            leftColumn: 'id',
            rightTable: 'orders',
            rightColumn: 'user_id',
            isManual: false,
          },
        ],
      }),
    );
    expect(sql).toBe(
      'SELECT "users"."name", "orders"."total" FROM "users"\nLEFT JOIN "orders" ON "users"."id" = "orders"."user_id";',
    );
  });

  it('generates multiple JOINs', () => {
    const sql = generateSql(
      baseInput({
        selectedTables: ['users', 'orders', 'items'],
        selectedColumns: [
          { table: 'users', column: 'name' },
          { table: 'orders', column: 'total' },
          { table: 'items', column: 'product' },
        ],
        joins: [
          {
            id: 'j1',
            type: 'INNER',
            leftTable: 'users',
            leftColumn: 'id',
            rightTable: 'orders',
            rightColumn: 'user_id',
            isManual: true,
          },
          {
            id: 'j2',
            type: 'LEFT',
            leftTable: 'orders',
            leftColumn: 'id',
            rightTable: 'items',
            rightColumn: 'order_id',
            isManual: false,
          },
        ],
      }),
    );
    expect(sql).toContain('INNER JOIN "orders"');
    expect(sql).toContain('LEFT JOIN "items"');
  });

  it('generates JOIN with table alias', () => {
    const sql = generateSql(
      baseInput({
        selectedTables: ['users', 'orders'],
        selectedColumns: [
          { table: 'users', column: 'name', alias: 'user_name' },
          { table: 'orders', column: 'total' },
        ],
        tableAliases: { users: 'u' },
        joins: [
          {
            id: 'j1',
            type: 'INNER',
            leftTable: 'users',
            leftColumn: 'id',
            rightTable: 'orders',
            rightColumn: 'user_id',
            isManual: true,
          },
        ],
      }),
    );
    // The alias is declared in FROM …
    expect(sql).toContain('FROM "users" AS "u"');
    // … and every other clause must reference it, including SELECT (an alias
    // that only appears in FROM/ON makes the alias meaningless and, when the
    // plain table name is used elsewhere, the statement will not resolve).
    expect(sql).toContain('"u"."name" AS "user_name"');
    // `orders` has no alias in this fixture, so it stays unqualified — and is
    // therefore not re-declared with `AS`.
    expect(sql).toContain('"orders"."total"');
    expect(sql).toContain('INNER JOIN "orders" ON "u"."id" = "orders"."user_id"');
  });

  it('generates no JOIN clause when joins is empty', () => {
    const sql = generateSql(
      baseInput({
        selectedTables: ['users'],
        selectedColumns: [{ table: 'users', column: 'name' }],
        joins: [],
      }),
    );
    expect(sql).not.toContain('JOIN');
  });

  // ── LIMIT / OFFSET generation ───────────────────────────────

  it('generates LIMIT only', () => {
    const sql = generateSql(
      baseInput({
        selectedTables: ['users'],
        selectedColumns: [{ table: 'users', column: 'name' }],
        limit: 50,
      }),
    );
    expect(sql).toBe('SELECT "users"."name" FROM "users" LIMIT 50;');
  });

  it('generates LIMIT and OFFSET', () => {
    const sql = generateSql(
      baseInput({
        selectedTables: ['users'],
        selectedColumns: [{ table: 'users', column: 'name' }],
        limit: 10,
        offset: 20,
      }),
    );
    expect(sql).toBe('SELECT "users"."name" FROM "users" LIMIT 10 OFFSET 20;');
  });

  it('generates no LIMIT/OFFSET when both are null', () => {
    const sql = generateSql(
      baseInput({
        selectedTables: ['users'],
        selectedColumns: [{ table: 'users', column: 'name' }],
        limit: null,
        offset: null,
      }),
    );
    expect(sql).not.toContain('LIMIT');
    expect(sql).not.toContain('OFFSET');
  });

  it('generates MySQL LIMIT/OFFSET with reversed syntax', () => {
    const sql = generateSql(
      baseInput({
        selectedTables: ['users'],
        selectedColumns: [{ table: 'users', column: 'name' }],
        limit: 10,
        offset: 20,
        databaseType: 'mysql',
      }),
    );
    expect(sql).toBe('SELECT `users`.`name` FROM `users` LIMIT 20, 10;');
  });

  // ── Table alias in FROM ─────────────────────────────────────

  it('generates FROM with table alias', () => {
    const sql = generateSql(
      baseInput({
        selectedTables: ['users'],
        selectedColumns: [{ table: 'users', column: 'name' }],
        tableAliases: { users: 'u' },
      }),
    );
    expect(sql).toBe('SELECT "u"."name" FROM "users" AS "u";');
  });

  // ── Complete complex query with JOIN + LIMIT ────────────────

  it('generates a complete query with JOIN, WHERE, GROUP BY, ORDER BY, LIMIT', () => {
    const subGroup: QbConditionGroup = {
      id: 'sub',
      logic: 'OR',
      conditions: [
        {
          id: '3',
          table: 'users',
          column: 'role',
          operator: '=',
          value: 'admin',
          conjunction: 'OR',
        },
      ],
      groups: [],
    };
    const where = emptyGroup('root');
    where.conditions = [
      { id: '1', table: 'users', column: 'active', operator: '=', value: '1', conjunction: 'AND' },
    ];
    where.groups = [subGroup];

    const sql = generateSql(
      baseInput({
        selectedTables: ['users', 'orders'],
        selectedColumns: [
          { table: 'users', column: 'name' },
          { table: 'orders', column: 'total', aggregate: 'SUM', alias: 'sum_total' },
        ],
        joins: [
          {
            id: 'j1',
            type: 'INNER',
            leftTable: 'users',
            leftColumn: 'id',
            rightTable: 'orders',
            rightColumn: 'user_id',
            isManual: true,
          },
        ],
        where,
        orderBy: [{ table: 'users', column: 'name', direction: 'ASC' }],
        groupBy: [{ table: 'users', column: 'name' }],
        distinct: false,
        limit: 100,
        offset: 0,
        databaseType: 'postgresql',
      }),
    );
    expect(sql).toBe(
      'SELECT "users"."name", SUM("orders"."total") AS "sum_total" FROM "users"\n' +
        'INNER JOIN "orders" ON "users"."id" = "orders"."user_id"' +
        ' WHERE "users"."active" = 1 AND ("users"."role" = \'admin\')' +
        ' GROUP BY "users"."name" ORDER BY "users"."name" ASC LIMIT 100;',
    );
  });
});

/**
 * Regression: a per-column sort on an aggregated column must order by the
 * aggregate expression. `SELECT SUM(x) … GROUP BY y ORDER BY x` is rejected by
 * every engine ("x must appear in the GROUP BY clause or be used in an
 * aggregate function"), which made a visually valid builder state generate a
 * statement that could not run.
 */
describe('generateSql — ORDER BY on an aggregated column', () => {
  const where: QbConditionGroup = { id: 'root', logic: 'AND', conditions: [], groups: [] };

  const base: GenerateSqlInput = {
    selectedTables: ['sales'],
    selectedColumns: [
      { table: 'sales', column: 'region', groupBy: true },
      { table: 'sales', column: 'amount', aggregate: 'SUM', alias: 'total', sort: 'DESC' },
    ],
    joins: [],
    tableAliases: {},
    where,
    orderBy: [],
    groupBy: [],
    distinct: false,
    limit: null,
    offset: null,
    databaseType: 'postgresql',
  };

  it('wraps the sort key in its aggregate', () => {
    const sql = generateSql(base);
    expect(sql).toContain('ORDER BY SUM("sales"."amount") DESC');
    expect(sql).not.toMatch(/ORDER BY "sales"\."amount"/);
  });

  it('leaves non-aggregated sort keys untouched', () => {
    const sql = generateSql({
      ...base,
      selectedColumns: [
        { table: 'sales', column: 'region', groupBy: true },
        { table: 'sales', column: 'amount', aggregate: 'SUM', alias: 'total' },
      ],
      orderBy: [{ table: 'sales', column: 'region', direction: 'ASC' }],
    });
    expect(sql).toContain('ORDER BY "sales"."region" ASC');
  });
});
