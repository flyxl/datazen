import { describe, it, expect } from 'vitest';
import { getQbDialectAdapter, generateJoinClause, generateLimitOffset } from '../queryBuilder';
import type { QbDialectAdapter } from '../queryBuilder';
import type { QbJoin } from '../../../components/query-builder/types';

// ── Tests ─────────────────────────────────────────────────────

describe('getQbDialectAdapter', () => {
  // ── Factory resolution ─────────────────────────────────────

  describe('factory resolution', () => {
    it('returns postgresql adapter for "postgresql" family', () => {
      const adapter = getQbDialectAdapter('postgresql');
      expect(adapter.quoteIdentifier('col')).toBe('"col"');
      expect(adapter.supportsILike).toBe(true);
    });

    it('returns mysql adapter for "mysql" family', () => {
      const adapter = getQbDialectAdapter('mysql');
      expect(adapter.quoteIdentifier('col')).toBe('`col`');
      expect(adapter.supportsILike).toBe(false);
    });

    it('returns sqlite adapter for "sqlite" family', () => {
      const adapter = getQbDialectAdapter('sqlite');
      expect(adapter.quoteIdentifier('col')).toBe('"col"');
      expect(adapter.supportsILike).toBe(false);
    });

    it('returns sqlserver adapter for "sqlserver" family', () => {
      const adapter = getQbDialectAdapter('sqlserver');
      expect(adapter.quoteIdentifier('col')).toBe('[col]');
      expect(adapter.supportsILike).toBe(false);
    });

    it('returns generic adapter for unknown database type', () => {
      const adapter = getQbDialectAdapter('unknown_db');
      expect(adapter.quoteIdentifier('col')).toBe('"col"');
      expect(adapter.supportsILike).toBe(false);
    });

    it('returns generic adapter for undefined database type', () => {
      const adapter = getQbDialectAdapter(undefined);
      expect(adapter.quoteIdentifier('col')).toBe('"col"');
      expect(adapter.supportsILike).toBe(false);
    });
  });

  // ── PostgreSQL adapter ─────────────────────────────────────

  describe('postgresql adapter', () => {
    let adapter: QbDialectAdapter;
    it('setup', () => {
      adapter = getQbDialectAdapter('postgresql');
    });

    it('quotes identifiers with double quotes', () => {
      expect(adapter.quoteIdentifier('users')).toBe('"users"');
      expect(adapter.quoteIdentifier('user id')).toBe('"user id"');
    });

    it('supports ILIKE', () => {
      expect(adapter.supportsILike).toBe(true);
    });

    it('formats LIMIT without offset', () => {
      expect(adapter.formatLimitOffset(10, 0)).toBe('LIMIT 10');
    });

    it('formats LIMIT with offset', () => {
      expect(adapter.formatLimitOffset(10, 20)).toBe('LIMIT 10 OFFSET 20');
    });

    it('formats IS NULL', () => {
      expect(adapter.formatNullComparison('"col"', true)).toBe('"col" IS NULL');
    });

    it('formats IS NOT NULL', () => {
      expect(adapter.formatNullComparison('"col"', false)).toBe('"col" IS NOT NULL');
    });

    it('formats IN list', () => {
      expect(adapter.formatInList('"id"', ['1', '2', '3'], false)).toBe('"id" IN (1, 2, 3)');
    });

    it('formats NOT IN list', () => {
      expect(adapter.formatInList('"id"', ['1', '2'], true)).toBe('"id" NOT IN (1, 2)');
    });

    it('formats empty IN list', () => {
      expect(adapter.formatInList('"id"', [], false)).toBe('"id" IN ()');
    });
  });

  // ── MySQL adapter ──────────────────────────────────────────

  describe('mysql adapter', () => {
    let adapter: QbDialectAdapter;
    it('setup', () => {
      adapter = getQbDialectAdapter('mysql');
    });

    it('quotes identifiers with backticks', () => {
      expect(adapter.quoteIdentifier('users')).toBe('`users`');
    });

    it('does not support ILIKE', () => {
      expect(adapter.supportsILike).toBe(false);
    });

    it('formats LIMIT without offset', () => {
      expect(adapter.formatLimitOffset(10, 0)).toBe('LIMIT 10');
    });

    it('formats LIMIT with offset (MySQL syntax: LIMIT offset, count)', () => {
      expect(adapter.formatLimitOffset(10, 20)).toBe('LIMIT 20, 10');
    });

    it('formats IS NULL', () => {
      expect(adapter.formatNullComparison('`col`', true)).toBe('`col` IS NULL');
    });

    it('formats IS NOT NULL', () => {
      expect(adapter.formatNullComparison('`col`', false)).toBe('`col` IS NOT NULL');
    });

    it('formats IN list', () => {
      expect(adapter.formatInList('`id`', ['1', '2'], false)).toBe('`id` IN (1, 2)');
    });

    it('formats NOT IN list', () => {
      expect(adapter.formatInList('`id`', ['1'], true)).toBe('`id` NOT IN (1)');
    });
  });

  // ── SQLite adapter ─────────────────────────────────────────

  describe('sqlite adapter', () => {
    let adapter: QbDialectAdapter;
    it('setup', () => {
      adapter = getQbDialectAdapter('sqlite');
    });

    it('quotes identifiers with double quotes', () => {
      expect(adapter.quoteIdentifier('users')).toBe('"users"');
    });

    it('does not support ILIKE', () => {
      expect(adapter.supportsILike).toBe(false);
    });

    it('formats LIMIT without offset', () => {
      expect(adapter.formatLimitOffset(10, 0)).toBe('LIMIT 10');
    });

    it('formats LIMIT with offset', () => {
      expect(adapter.formatLimitOffset(10, 20)).toBe('LIMIT 10 OFFSET 20');
    });

    it('formats IS NULL', () => {
      expect(adapter.formatNullComparison('"col"', true)).toBe('"col" IS NULL');
    });

    it('formats IS NOT NULL', () => {
      expect(adapter.formatNullComparison('"col"', false)).toBe('"col" IS NOT NULL');
    });

    it('formats IN list', () => {
      expect(adapter.formatInList('"id"', ['1', '2'], false)).toBe('"id" IN (1, 2)');
    });

    it('formats NOT IN list', () => {
      expect(adapter.formatInList('"id"', [], true)).toBe('"id" NOT IN ()');
    });
  });

  // ── SQL Server adapter ─────────────────────────────────────

  describe('sqlserver adapter', () => {
    let adapter: QbDialectAdapter;
    it('setup', () => {
      adapter = getQbDialectAdapter('sqlserver');
    });

    it('quotes identifiers with square brackets', () => {
      expect(adapter.quoteIdentifier('users')).toBe('[users]');
    });

    it('does not support ILIKE', () => {
      expect(adapter.supportsILike).toBe(false);
    });

    it('returns null for LIMIT/OFFSET (SQL Server uses TOP/OFFSET-FETCH)', () => {
      expect(adapter.formatLimitOffset(10, 0)).toBeNull();
      expect(adapter.formatLimitOffset(10, 20)).toBeNull();
    });

    it('formats IS NULL', () => {
      expect(adapter.formatNullComparison('[col]', true)).toBe('[col] IS NULL');
    });

    it('formats IS NOT NULL', () => {
      expect(adapter.formatNullComparison('[col]', false)).toBe('[col] IS NOT NULL');
    });

    it('formats IN list', () => {
      expect(adapter.formatInList('[id]', ['1', '2'], false)).toBe('[id] IN (1, 2)');
    });

    it('formats NOT IN list', () => {
      expect(adapter.formatInList('[id]', ['a'], true)).toBe('[id] NOT IN (a)');
    });
  });

  // ── Generic adapter (fallback) ─────────────────────────────

  describe('generic adapter (fallback)', () => {
    let adapter: QbDialectAdapter;
    it('setup', () => {
      adapter = getQbDialectAdapter('unknown_db');
    });

    it('quotes identifiers with double quotes', () => {
      expect(adapter.quoteIdentifier('test')).toBe('"test"');
    });

    it('does not support ILIKE', () => {
      expect(adapter.supportsILike).toBe(false);
    });

    it('formats LIMIT without offset', () => {
      expect(adapter.formatLimitOffset(5, 0)).toBe('LIMIT 5');
    });

    it('formats LIMIT with offset', () => {
      expect(adapter.formatLimitOffset(5, 10)).toBe('LIMIT 5 OFFSET 10');
    });

    it('formats IS NULL', () => {
      expect(adapter.formatNullComparison('"c"', true)).toBe('"c" IS NULL');
    });

    it('formats IS NOT NULL', () => {
      expect(adapter.formatNullComparison('"c"', false)).toBe('"c" IS NOT NULL');
    });

    it('formats IN list', () => {
      expect(adapter.formatInList('"c"', ['x'], false)).toBe('"c" IN (x)');
    });

    it('formats NOT IN list', () => {
      expect(adapter.formatInList('"c"', ['x', 'y'], true)).toBe('"c" NOT IN (x, y)');
    });
  });
});

// ── generateJoinClause ────────────────────────────────────────

describe('generateJoinClause', () => {
  const pg = getQbDialectAdapter('postgresql');
  const mysql = getQbDialectAdapter('mysql');

  it('returns empty string for empty joins', () => {
    expect(generateJoinClause([], {}, pg)).toBe('');
  });

  it('generates INNER JOIN with no aliases', () => {
    const joins: QbJoin[] = [
      {
        id: 'j1',
        type: 'INNER',
        leftTable: 'users',
        leftColumn: 'id',
        rightTable: 'orders',
        rightColumn: 'user_id',
        isManual: true,
      },
    ];
    const result = generateJoinClause(joins, {}, pg);
    expect(result).toBe('\nINNER JOIN "orders" ON "users"."id" = "orders"."user_id"');
  });

  it('generates LEFT JOIN', () => {
    const joins: QbJoin[] = [
      {
        id: 'j1',
        type: 'LEFT',
        leftTable: 'users',
        leftColumn: 'id',
        rightTable: 'orders',
        rightColumn: 'user_id',
        isManual: false,
      },
    ];
    const result = generateJoinClause(joins, {}, pg);
    expect(result).toBe('\nLEFT JOIN "orders" ON "users"."id" = "orders"."user_id"');
  });

  it('generates RIGHT JOIN', () => {
    const joins: QbJoin[] = [
      {
        id: 'j1',
        type: 'RIGHT',
        leftTable: 'a',
        leftColumn: 'id',
        rightTable: 'b',
        rightColumn: 'a_id',
        isManual: true,
      },
    ];
    const result = generateJoinClause(joins, {}, pg);
    expect(result).toBe('\nRIGHT JOIN "b" ON "a"."id" = "b"."a_id"');
  });

  it('generates FULL JOIN', () => {
    const joins: QbJoin[] = [
      {
        id: 'j1',
        type: 'FULL',
        leftTable: 'a',
        leftColumn: 'id',
        rightTable: 'b',
        rightColumn: 'a_id',
        isManual: true,
      },
    ];
    const result = generateJoinClause(joins, {}, pg);
    expect(result).toBe('\nFULL JOIN "b" ON "a"."id" = "b"."a_id"');
  });

  it('uses alias for left table when provided', () => {
    const joins: QbJoin[] = [
      {
        id: 'j1',
        type: 'INNER',
        leftTable: 'users',
        leftColumn: 'id',
        rightTable: 'orders',
        rightColumn: 'user_id',
        isManual: true,
      },
    ];
    const result = generateJoinClause(joins, { users: 'u' }, pg);
    expect(result).toBe('\nINNER JOIN "orders" ON "u"."id" = "orders"."user_id"');
  });

  it('generates multiple JOINs', () => {
    const joins: QbJoin[] = [
      {
        id: 'j1',
        type: 'INNER',
        leftTable: 'a',
        leftColumn: 'id',
        rightTable: 'b',
        rightColumn: 'a_id',
        isManual: true,
      },
      {
        id: 'j2',
        type: 'LEFT',
        leftTable: 'b',
        leftColumn: 'id',
        rightTable: 'c',
        rightColumn: 'b_id',
        isManual: false,
      },
    ];
    const result = generateJoinClause(joins, {}, pg);
    expect(result).toContain('INNER JOIN "b"');
    expect(result).toContain('LEFT JOIN "c"');
  });

  it('uses MySQL backtick quoting', () => {
    const joins: QbJoin[] = [
      {
        id: 'j1',
        type: 'INNER',
        leftTable: 'users',
        leftColumn: 'id',
        rightTable: 'orders',
        rightColumn: 'user_id',
        isManual: true,
      },
    ];
    const result = generateJoinClause(joins, {}, mysql);
    expect(result).toBe('\nINNER JOIN `orders` ON `users`.`id` = `orders`.`user_id`');
  });
});

// ── generateLimitOffset ───────────────────────────────────────

describe('generateLimitOffset', () => {
  const pg = getQbDialectAdapter('postgresql');
  const mysql = getQbDialectAdapter('mysql');

  it('returns empty string when both are null', () => {
    expect(generateLimitOffset(null, null, pg)).toBe('');
  });

  it('generates LIMIT only (PostgreSQL)', () => {
    expect(generateLimitOffset(50, null, pg)).toBe(' LIMIT 50');
  });

  it('generates LIMIT and OFFSET (PostgreSQL)', () => {
    expect(generateLimitOffset(10, 20, pg)).toBe(' LIMIT 10 OFFSET 20');
  });

  it('generates LIMIT only (MySQL)', () => {
    expect(generateLimitOffset(50, null, mysql)).toBe(' LIMIT 50');
  });

  it('generates LIMIT and OFFSET (MySQL) with reversed syntax', () => {
    expect(generateLimitOffset(10, 20, mysql)).toBe(' LIMIT 20, 10');
  });
});

// ── generateJoinClause: graph ordering (regression) ───────────

/**
 * A flat `JOIN rightTable ON left = right` per entry produced invalid SQL as
 * soon as three tables were chained: the FK detector returns the graph in
 * discovery order, which is not a valid join order. These pin the walk that
 * fixes it.
 */
describe('generateJoinClause — join graph ordering', () => {
  const pg = getQbDialectAdapter('postgresql');

  const join = (over: Partial<QbJoin> & Pick<QbJoin, 'leftTable' | 'rightTable'>): QbJoin => ({
    id: `${over.leftTable}-${over.rightTable}`,
    type: 'INNER',
    leftColumn: 'id',
    rightColumn: 'ref_id',
    isManual: false,
    ...over,
  });

  it('orients a join whose FROM table is on the right-hand side', () => {
    // FROM author; the FK was discovered as book.author_id → author.id, so the
    // FROM table is the *right* side and must not be re-joined.
    const result = generateJoinClause(
      [
        join({
          leftTable: 'book',
          leftColumn: 'author_id',
          rightTable: 'author',
          rightColumn: 'id',
        }),
      ],
      {},
      pg,
      'author',
    );
    expect(result).toBe('\nINNER JOIN "book" ON "author"."id" = "book"."author_id"');
  });

  it('orders a three-table chain from the FROM table outward', () => {
    // Discovery order (book→author, sale→book) is not a usable join order.
    const result = generateJoinClause(
      [
        join({
          leftTable: 'book',
          leftColumn: 'author_id',
          rightTable: 'author',
          rightColumn: 'id',
        }),
        join({
          leftTable: 'sale',
          leftColumn: 'book_id',
          rightTable: 'book',
          rightColumn: 'id',
        }),
      ],
      {},
      pg,
      'author',
    );
    expect(result).toBe(
      '\nINNER JOIN "book" ON "author"."id" = "book"."author_id"' +
        '\nINNER JOIN "sale" ON "book"."id" = "sale"."book_id"',
    );
  });

  it('merges repeated pairs into one JOIN with AND predicates (composite key)', () => {
    const result = generateJoinClause(
      [
        join({
          leftTable: 'child',
          leftColumn: 'a_id',
          rightTable: 'parent',
          rightColumn: 'a_id',
        }),
        join({
          leftTable: 'child',
          leftColumn: 'b_id',
          rightTable: 'parent',
          rightColumn: 'b_id',
        }),
      ],
      {},
      pg,
      'parent',
    );
    expect(result).toBe(
      '\nINNER JOIN "child" ON "parent"."a_id" = "child"."a_id"' +
        ' AND "parent"."b_id" = "child"."b_id"',
    );
  });

  it('never emits a second JOIN for a table already in the query', () => {
    const result = generateJoinClause(
      [
        join({ leftTable: 'a', leftColumn: 'b_id', rightTable: 'b', rightColumn: 'id' }),
        join({ leftTable: 'c', leftColumn: 'b_id', rightTable: 'b', rightColumn: 'id' }),
      ],
      {},
      pg,
      'b',
    );
    expect((result.match(/JOIN/g) ?? []).length).toBe(2);
    expect(result).not.toContain('ON "b"."id" = "b"."id"');
  });
});
