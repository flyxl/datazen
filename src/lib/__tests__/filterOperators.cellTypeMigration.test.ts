/**
 * The type migration of DB-05: which column types changed their render
 * family, and which filter operators they gained or lost.
 *
 * The "before" side is a faithful copy of the baseline implementation at
 * `66fa0b72e` (`src/lib/dataTypeColors.ts` + `src/lib/filterOperators.ts`):
 * a substring ladder that answered `number` for `point` (because
 * `'point'.includes('int')`) and `text` for `UInt64` (for the same reason,
 * by accident).
 *
 * The list below is the **whole** migration: every type of the corpus that
 * changed, with the operators it lost and gained. A change that is not on
 * the list fails the second test, so a new rule cannot silently move a
 * column to another family — it has to be written down here (and in
 * `CHANGELOG.md`).
 */
import { describe, expect, it } from 'vitest';
import { classifyDataType, type DataTypeFamily } from '../dataTypeColors';
import { operatorsForColumn } from '../filterOperators';
import { resolveCellType, type CellType } from '../cellTypes';
import type { FilterOperator } from '../../types';

/** The baseline classifier, copied verbatim from `66fa0b72e`. */
function legacyClassifyDataType(dataType: string | undefined): DataTypeFamily {
  const type = (dataType ?? '').toLowerCase();
  if (!type) return 'text';
  if (type.includes('bool')) return 'bool';
  if (
    type.includes('int') ||
    type.includes('serial') ||
    type.includes('double') ||
    type.includes('numeric') ||
    type.includes('decimal') ||
    type.includes('real') ||
    type.includes('float')
  ) {
    return 'number';
  }
  if (
    type.includes('timestamp') ||
    type.includes('date') ||
    type.includes('time') ||
    type.includes('interval')
  ) {
    return 'datetime';
  }
  if (type.includes('json')) return 'json';
  if (type.includes('bytea') || type.includes('blob') || type.includes('binary')) return 'binary';
  return 'text';
}

const LEGACY_BY_FAMILY: Record<DataTypeFamily, readonly FilterOperator[]> = {
  text: [
    'eq',
    'ne',
    'gt',
    'lt',
    'gte',
    'lte',
    'like',
    'contains',
    'notContains',
    'startsWith',
    'endsWith',
    'in',
    'notIn',
    'isNull',
    'isNotNull',
    'isEmpty',
    'isNotEmpty',
  ],
  number: ['eq', 'ne', 'gt', 'lt', 'gte', 'lte', 'between', 'notBetween', 'in', 'notIn', 'isNull', 'isNotNull'],
  datetime: ['eq', 'ne', 'gt', 'lt', 'gte', 'lte', 'between', 'notBetween', 'isNull', 'isNotNull'],
  bool: ['eq', 'ne', 'isNull', 'isNotNull'],
  json: ['eq', 'ne', 'isNull', 'isNotNull'],
  binary: ['eq', 'ne', 'isNull', 'isNotNull'],
  null: ['eq', 'ne', 'isNull', 'isNotNull'],
};

const LEGACY_DIALECT_BY_FAMILY: Record<DataTypeFamily, readonly FilterOperator[]> = {
  text: ['regex', 'notRegex'],
  number: [],
  datetime: [],
  bool: [],
  json: ['jsonContains'],
  binary: [],
  null: [],
};

/** Every operator the shared whitelist allows (the driver whitelist only narrows). */
const ALL_SHARED: readonly FilterOperator[] = [
  'eq',
  'ne',
  'gt',
  'lt',
  'gte',
  'lte',
  'between',
  'notBetween',
  'like',
  'contains',
  'notContains',
  'startsWith',
  'endsWith',
  'in',
  'notIn',
  'isNull',
  'isNotNull',
  'isEmpty',
  'isNotEmpty',
  'regex',
  'notRegex',
  'jsonContains',
];

function legacyOperators(dataType: string | undefined): FilterOperator[] {
  const family = legacyClassifyDataType(dataType);
  const candidates = [...LEGACY_BY_FAMILY[family], ...LEGACY_DIALECT_BY_FAMILY[family]];
  return candidates.filter((op) => ALL_SHARED.includes(op));
}

interface Migration {
  /** The driver type name. */
  readonly type: string;
  /** The render family before the type table. */
  readonly familyBefore: DataTypeFamily;
  /** The render family now. */
  readonly familyAfter: DataTypeFamily;
  /** The normalised cell type now. */
  readonly cellType: CellType;
  /** The operators the column no longer offers. */
  readonly removed: readonly FilterOperator[];
  /** The operators the column now offers. */
  readonly added: readonly FilterOperator[];
}

/** The whole migration, column by column. */
const MIGRATIONS: readonly Migration[] = [
  {
    type: 'ARRAY',
    familyBefore: 'text',
    familyAfter: 'json',
    cellType: 'json',
    removed: ['gt', 'lt', 'gte', 'lte', 'like', 'contains', 'notContains', 'startsWith', 'endsWith', 'in', 'notIn', 'isEmpty', 'isNotEmpty', 'regex', 'notRegex'],
    added: ['jsonContains'],
  },
  {
    type: 'ARRAY[]',
    familyBefore: 'text',
    familyAfter: 'json',
    cellType: 'json',
    removed: ['gt', 'lt', 'gte', 'lte', 'like', 'contains', 'notContains', 'startsWith', 'endsWith', 'in', 'notIn', 'isEmpty', 'isNotEmpty', 'regex', 'notRegex'],
    added: ['jsonContains'],
  },
  {
    type: 'Array(Int32)',
    familyBefore: 'number',
    familyAfter: 'json',
    cellType: 'json',
    removed: ['gt', 'lt', 'gte', 'lte', 'between', 'notBetween', 'in', 'notIn'],
    added: ['jsonContains'],
  },
  {
    type: 'Enum',
    familyBefore: 'text',
    familyAfter: 'text',
    cellType: 'enum',
    removed: ['gt', 'lt', 'gte', 'lte', 'like', 'contains', 'notContains', 'startsWith', 'endsWith', 'isEmpty', 'isNotEmpty', 'regex', 'notRegex'],
    added: [],
  },
  {
    type: "Enum8('a' = 1)",
    familyBefore: 'text',
    familyAfter: 'text',
    cellType: 'enum',
    removed: ['gt', 'lt', 'gte', 'lte', 'like', 'contains', 'notContains', 'startsWith', 'endsWith', 'isEmpty', 'isNotEmpty', 'regex', 'notRegex'],
    added: [],
  },
  {
    type: 'FixedString(16)',
    familyBefore: 'text',
    familyAfter: 'binary',
    cellType: 'bytes',
    removed: ['gt', 'lt', 'gte', 'lte', 'like', 'contains', 'notContains', 'startsWith', 'endsWith', 'in', 'notIn', 'isEmpty', 'isNotEmpty', 'regex', 'notRegex'],
    added: [],
  },
  {
    type: 'NUMBER(38,0)',
    familyBefore: 'text',
    familyAfter: 'number',
    cellType: 'number',
    removed: ['like', 'contains', 'notContains', 'startsWith', 'endsWith', 'isEmpty', 'isNotEmpty', 'regex', 'notRegex'],
    added: ['between', 'notBetween'],
  },
  {
    type: 'RAW',
    familyBefore: 'text',
    familyAfter: 'binary',
    cellType: 'bytes',
    removed: ['gt', 'lt', 'gte', 'lte', 'like', 'contains', 'notContains', 'startsWith', 'endsWith', 'in', 'notIn', 'isEmpty', 'isNotEmpty', 'regex', 'notRegex'],
    added: [],
  },
  {
    type: 'UUID',
    familyBefore: 'text',
    familyAfter: 'text',
    cellType: 'uuid',
    removed: ['gt', 'lt', 'gte', 'lte', 'like', 'contains', 'notContains', 'startsWith', 'endsWith', 'isEmpty', 'isNotEmpty', 'regex', 'notRegex'],
    added: [],
  },
  {
    type: 'abstime',
    familyBefore: 'datetime',
    familyAfter: 'text',
    cellType: 'unknown',
    removed: ['between', 'notBetween'],
    added: ['like', 'contains', 'notContains', 'startsWith', 'endsWith', 'in', 'notIn', 'isEmpty', 'isNotEmpty', 'regex', 'notRegex'],
  },
  {
    type: 'dec',
    familyBefore: 'text',
    familyAfter: 'number',
    cellType: 'number',
    removed: ['like', 'contains', 'notContains', 'startsWith', 'endsWith', 'isEmpty', 'isNotEmpty', 'regex', 'notRegex'],
    added: ['between', 'notBetween'],
  },
  {
    type: 'dec(10,2)',
    familyBefore: 'text',
    familyAfter: 'number',
    cellType: 'number',
    removed: ['like', 'contains', 'notContains', 'startsWith', 'endsWith', 'isEmpty', 'isNotEmpty', 'regex', 'notRegex'],
    added: ['between', 'notBetween'],
  },
  {
    type: "enum('a','b')",
    familyBefore: 'text',
    familyAfter: 'text',
    cellType: 'enum',
    removed: ['gt', 'lt', 'gte', 'lte', 'like', 'contains', 'notContains', 'startsWith', 'endsWith', 'isEmpty', 'isNotEmpty', 'regex', 'notRegex'],
    added: [],
  },
  {
    type: 'image',
    familyBefore: 'text',
    familyAfter: 'binary',
    cellType: 'bytes',
    removed: ['gt', 'lt', 'gte', 'lte', 'like', 'contains', 'notContains', 'startsWith', 'endsWith', 'in', 'notIn', 'isEmpty', 'isNotEmpty', 'regex', 'notRegex'],
    added: [],
  },
  {
    type: 'integer[]',
    familyBefore: 'number',
    familyAfter: 'json',
    cellType: 'json',
    removed: ['gt', 'lt', 'gte', 'lte', 'between', 'notBetween', 'in', 'notIn'],
    added: ['jsonContains'],
  },
  {
    type: 'interval',
    familyBefore: 'number',
    familyAfter: 'text',
    cellType: 'text',
    removed: ['between', 'notBetween'],
    added: ['like', 'contains', 'notContains', 'startsWith', 'endsWith', 'isEmpty', 'isNotEmpty', 'regex', 'notRegex'],
  },
  {
    type: 'point',
    familyBefore: 'number',
    familyAfter: 'text',
    cellType: 'text',
    removed: ['between', 'notBetween'],
    added: ['like', 'contains', 'notContains', 'startsWith', 'endsWith', 'isEmpty', 'isNotEmpty', 'regex', 'notRegex'],
  },
  {
    type: 'reltime',
    familyBefore: 'datetime',
    familyAfter: 'text',
    cellType: 'unknown',
    removed: ['between', 'notBetween'],
    added: ['like', 'contains', 'notContains', 'startsWith', 'endsWith', 'in', 'notIn', 'isEmpty', 'isNotEmpty', 'regex', 'notRegex'],
  },
  {
    type: "set('a','b')",
    familyBefore: 'text',
    familyAfter: 'text',
    cellType: 'enum',
    removed: ['gt', 'lt', 'gte', 'lte', 'like', 'contains', 'notContains', 'startsWith', 'endsWith', 'isEmpty', 'isNotEmpty', 'regex', 'notRegex'],
    added: [],
  },
  {
    type: 'text[]',
    familyBefore: 'text',
    familyAfter: 'json',
    cellType: 'json',
    removed: ['gt', 'lt', 'gte', 'lte', 'like', 'contains', 'notContains', 'startsWith', 'endsWith', 'in', 'notIn', 'isEmpty', 'isNotEmpty', 'regex', 'notRegex'],
    added: ['jsonContains'],
  },
  {
    type: 'uniqueidentifier',
    familyBefore: 'text',
    familyAfter: 'text',
    cellType: 'uuid',
    removed: ['gt', 'lt', 'gte', 'lte', 'like', 'contains', 'notContains', 'startsWith', 'endsWith', 'isEmpty', 'isNotEmpty', 'regex', 'notRegex'],
    added: [],
  },
  {
    type: 'uuid',
    familyBefore: 'text',
    familyAfter: 'text',
    cellType: 'uuid',
    removed: ['gt', 'lt', 'gte', 'lte', 'like', 'contains', 'notContains', 'startsWith', 'endsWith', 'isEmpty', 'isNotEmpty', 'regex', 'notRegex'],
    added: [],
  },
];

/**
 * The corpus the migration is measured on: the type names the drivers
 * actually report.
 */
const CORPUS: readonly string[] = [
  // postgres
  'int2', 'int4', 'int8', 'integer', 'bigint', 'smallint', 'serial', 'bigserial',
  'text', 'varchar(255)', 'character varying(255)', 'char(10)', 'name', 'citext',
  'timestamp without time zone', 'timestamp with time zone', 'timestamptz', 'timestamp(6)',
  'date', 'time without time zone', 'time with time zone', 'timetz', 'time(6)',
  'interval', 'boolean', 'bool', 'json', 'jsonb', 'jsonpath', 'uuid',
  'bytea', 'numeric(10,2)', 'decimal(38,0)', 'double precision', 'real', 'float8',
  'money', 'inet', 'cidr', 'macaddr', 'macaddr8', 'point', 'line', 'lseg', 'box',
  'path', 'polygon', 'circle', 'geometry', 'geography', 'hstore', 'xml', 'bit', 'varbit',
  'ARRAY', 'USER-DEFINED', 'ARRAY[]', 'text[]', 'integer[]', 'tsvector',
  // mysql
  'int(11) unsigned', 'tinyint(1)', 'mediumint', "enum('a','b')", "set('a','b')",
  'tinytext', 'mediumtext', 'longtext', 'tinyblob', 'mediumblob', 'longblob',
  'blob', 'binary(16)', 'varbinary(max)', 'datetime(6)', 'timestamp(6)', 'double',
  'year(4)', 'bit(8)', 'geometry', 'point',
  // sqlserver
  'nvarchar(max)', 'nchar(10)', 'uniqueidentifier', 'datetime2(7)', 'smalldatetime',
  'datetimeoffset', 'timestamp', 'rowversion', 'varbinary(max)', 'binary(16)', 'image',
  'sql_variant', 'xml', 'decimal(10,2)', 'money',
  // clickhouse
  'Nullable(Int32)', 'LowCardinality(String)', 'DateTime64(3)', 'FixedString(16)',
  "Enum8('a' = 1)", 'Int128', 'UInt64', 'Decimal(10,2)',
  // duckdb / sqlite / others
  'INTEGER', 'TEXT', 'REAL', 'NUMERIC', 'BLOB', 'DOUBLE', 'VARCHAR',
  // extra realistic corpus
  'UInt8', 'UInt16', 'UInt32', 'UInt128', 'UInt256', 'Nullable(UInt32)',
  'Int128', 'Int256', 'Decimal32(2)', 'Date32', 'DateTime', 'UUID', 'IPv4', 'IPv6',
  'Enum', 'Nothing', 'LowCardinality(Nullable(String))', 'Map(String, String)',
  'Tuple(Int8, String)', 'Array(Int32)', 'Nested(Int32, Int32)',
  'abstime', 'reltime', 'oid', 'xid', 'tid', 'regclass', 'int4range', 'tsvector',
  'tsquery', 'jsonpath', 'bpchar', 'name', 'oidvector', 'ntext', 'sql_variant',
  'character', 'character varying', 'bit varying', 'serial2', 'serial4', 'serial8',
  'smallserial', 'double precision', 'dec', 'dec(10,2)', 'fixed', 'money(10,2)',
  'boolean', 'bool', 'int', 'integer', 'int2', 'int4', 'int8',
  // oracle-ish
  'NUMBER(38,0)', 'RAW', 'CLOB', 'VARCHAR2(20)', 'TIMESTAMP(6) WITH TIME ZONE',
];

describe('the type migration of DB-05', () => {
  it('moves exactly the documented column types', () => {
    for (const migration of MIGRATIONS) {
      expect(legacyClassifyDataType(migration.type), migration.type).toBe(migration.familyBefore);
      expect(classifyDataType(migration.type), migration.type).toBe(migration.familyAfter);
      expect(resolveCellType(migration.type), migration.type).toBe(migration.cellType);

      const before = legacyOperators(migration.type);
      const now = operatorsForColumn(migration.type, ALL_SHARED);
      expect(
        before.filter((operator) => !now.includes(operator)),
        migration.type,
      ).toEqual([...migration.removed]);
      expect(
        now.filter((operator) => !before.includes(operator)),
        migration.type,
      ).toEqual([...migration.added]);
    }
  });

  it('moves nothing else in the corpus', () => {
    const documented = new Set(MIGRATIONS.map((migration) => migration.type));
    const undocumented: string[] = [];
    for (const type of CORPUS) {
      if (documented.has(type)) continue;
      const before = legacyOperators(type);
      const now = operatorsForColumn(type, ALL_SHARED);
      const sameFamily = legacyClassifyDataType(type) === classifyDataType(type);
      const sameOperators =
        before.length === now.length && before.every((operator) => now.includes(operator));
      if (!sameFamily || !sameOperators) undocumented.push(type);
    }
    expect(undocumented).toEqual([]);
  });

  it('keeps the unsigned integers numeric', () => {
    // `'uint64'.includes('int')` was true, so the baseline answered
    // "number" by accident; the rule table has to say it explicitly.
    for (const type of ['UInt8', 'UInt16', 'UInt32', 'UInt64', 'UInt128', 'UInt256']) {
      expect(resolveCellType(type), type).toBe('number');
      expect(classifyDataType(type), type).toBe('number');
    }
  });
});
