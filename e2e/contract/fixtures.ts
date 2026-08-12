/**
 * Host Connection Contract — driver fixtures (pure logic, unit-testable).
 *
 * E2E WDIO glue lives in sibling modules that import these types/helpers.
 * Dialect SQL here is for seeding only; journeys must not assert dialect text.
 */

export type DriverFixtureId = 'postgres' | 'mysql' | 'sqlite';

export type HostContractJourneyId =
  | 'HC-CONN'
  | 'HC-QUERY'
  | 'HC-DATA'
  | 'HC-FILTER'
  | 'HC-EDIT'
  | 'HC-STRUCT'
  | 'HC-INDEX'
  | 'HC-EXPORT'
  | 'HC-OBJ'
  | 'HC-EXPLAIN';

export interface DriverCapabilities {
  readonly hasSqlEditor: boolean;
  readonly hasTableData: boolean;
  readonly hasStructure: boolean;
  readonly hasIndexes: boolean;
  readonly hasExplain: boolean;
  readonly hasObjects: boolean;
  readonly hasPrivileges: boolean;
  readonly hasInlineEdit: boolean;
  readonly hasExport: boolean;
  readonly connectionMode: 'sql' | 'redis' | 'other';
}

export interface DialectSeedHelpers {
  /** Qualified / quoted table name for this dialect (simple identifiers). */
  quoteIdent(name: string): string;
  createFilterSeedTable(table: string): string;
  insertFilterSeedRows(table: string): string;
  createDataSeedTable(table: string): string;
  insertDataSeedRows(table: string, count: number): string;
  dropTable(table: string): string;
  /** Prefix for disposable E2E objects, e.g. _e2e_hc_pg_ */
  tablePrefix: string;
}

export interface DriverFixtureDefinition {
  readonly id: DriverFixtureId;
  readonly displayName: string;
  readonly capabilities: DriverCapabilities;
  readonly dialect: DialectSeedHelpers;
}

const SQL_CAPABILITIES: DriverCapabilities = {
  hasSqlEditor: true,
  hasTableData: true,
  hasStructure: true,
  hasIndexes: true,
  hasExplain: true,
  hasObjects: true,
  hasPrivileges: true,
  hasInlineEdit: true,
  hasExport: true,
  connectionMode: 'sql',
};

const SQLITE_CAPABILITIES: DriverCapabilities = {
  ...SQL_CAPABILITIES,
  hasObjects: false,
  hasPrivileges: false,
  hasExplain: true,
};

function pgDialect(prefix: string): DialectSeedHelpers {
  return {
    tablePrefix: prefix,
    quoteIdent: (name) => `"${name.replace(/"/g, '""')}"`,
    createFilterSeedTable: (table) => `
      CREATE TABLE ${table} (
        id SERIAL PRIMARY KEY,
        name VARCHAR(50) NOT NULL,
        score INT NOT NULL
      )`,
    insertFilterSeedRows: (table) => `
      INSERT INTO ${table} (name, score) VALUES
        ('alpha', 10),
        ('beta', 20),
        ('gamma', 30)`,
    createDataSeedTable: (table) => `
      CREATE TABLE ${table} (
        id SERIAL PRIMARY KEY,
        name VARCHAR(100) NOT NULL,
        score INT NOT NULL
      )`,
    insertDataSeedRows: (table, count) => `
      INSERT INTO ${table} (name, score)
      SELECT 'user_' || i, (i * 7) % 100
      FROM generate_series(1, ${Math.max(0, Math.floor(count))}) AS s(i)`,
    dropTable: (table) => `DROP TABLE IF EXISTS ${table}`,
  };
}

function mysqlDialect(prefix: string): DialectSeedHelpers {
  return {
    tablePrefix: prefix,
    quoteIdent: (name) => `\`${name.replace(/`/g, '``')}\``,
    createFilterSeedTable: (table) => `
      CREATE TABLE ${table} (
        id BIGINT AUTO_INCREMENT PRIMARY KEY,
        name VARCHAR(50) NOT NULL,
        score INT NOT NULL
      )`,
    insertFilterSeedRows: (table) => `
      INSERT INTO ${table} (name, score) VALUES
        ('alpha', 10),
        ('beta', 20),
        ('gamma', 30)`,
    createDataSeedTable: (table) => `
      CREATE TABLE ${table} (
        id BIGINT AUTO_INCREMENT PRIMARY KEY,
        name VARCHAR(100) NOT NULL,
        score INT NOT NULL
      )`,
    insertDataSeedRows: (table, count) => {
      const n = Math.max(0, Math.floor(count));
      if (n === 0) return `SELECT 1`;
      const values = Array.from({ length: n }, (_, i) => {
        const v = i + 1;
        return `('user_${v}', ${(v * 7) % 100})`;
      }).join(',\n        ');
      return `INSERT INTO ${table} (name, score) VALUES\n        ${values}`;
    },
    dropTable: (table) => `DROP TABLE IF EXISTS ${table}`,
  };
}

function sqliteDialect(prefix: string): DialectSeedHelpers {
  return {
    tablePrefix: prefix,
    quoteIdent: (name) => `"${name.replace(/"/g, '""')}"`,
    createFilterSeedTable: (table) => `
      CREATE TABLE ${table} (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        name TEXT NOT NULL,
        score INTEGER NOT NULL
      )`,
    insertFilterSeedRows: (table) => `
      INSERT INTO ${table} (name, score) VALUES
        ('alpha', 10),
        ('beta', 20),
        ('gamma', 30)`,
    createDataSeedTable: (table) => `
      CREATE TABLE ${table} (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        name TEXT NOT NULL,
        score INTEGER NOT NULL
      )`,
    insertDataSeedRows: (table, count) => {
      const n = Math.max(0, Math.floor(count));
      if (n === 0) return `SELECT 1`;
      const values = Array.from({ length: n }, (_, i) => {
        const v = i + 1;
        return `('user_${v}', ${(v * 7) % 100})`;
      }).join(',\n        ');
      return `INSERT INTO ${table} (name, score) VALUES\n        ${values}`;
    },
    dropTable: (table) => `DROP TABLE IF EXISTS ${table}`,
  };
}

export const DRIVER_FIXTURES: Record<DriverFixtureId, DriverFixtureDefinition> = {
  postgres: {
    id: 'postgres',
    displayName: '本地 PostgreSQL',
    capabilities: SQL_CAPABILITIES,
    dialect: pgDialect('_e2e_hc_pg_'),
  },
  mysql: {
    id: 'mysql',
    displayName: 'E2E-MySQL',
    capabilities: SQL_CAPABILITIES,
    dialect: mysqlDialect('_e2e_hc_my_'),
  },
  sqlite: {
    id: 'sqlite',
    displayName: 'E2E-SQLite',
    capabilities: SQLITE_CAPABILITIES,
    dialect: sqliteDialect('_e2e_hc_lt_'),
  },
};

export const DEFAULT_MATRIX_DRIVERS: readonly DriverFixtureId[] = [
  'postgres',
  'mysql',
  'sqlite',
] as const;

/** Journey → required capability flags on the fixture. */
export const JOURNEY_REQUIREMENTS: Record<
  HostContractJourneyId,
  readonly (keyof DriverCapabilities)[]
> = {
  'HC-CONN': ['hasSqlEditor'],
  'HC-QUERY': ['hasSqlEditor'],
  'HC-DATA': ['hasTableData'],
  'HC-FILTER': ['hasTableData'],
  'HC-EDIT': ['hasInlineEdit', 'hasTableData'],
  'HC-STRUCT': ['hasStructure'],
  'HC-INDEX': ['hasIndexes'],
  'HC-EXPORT': ['hasExport', 'hasTableData'],
  'HC-OBJ': ['hasObjects'],
  'HC-EXPLAIN': ['hasExplain', 'hasSqlEditor'],
};

export function getFixture(id: DriverFixtureId): DriverFixtureDefinition {
  return DRIVER_FIXTURES[id];
}

export function listMatrixFixtures(
  ids: readonly DriverFixtureId[] = DEFAULT_MATRIX_DRIVERS,
): DriverFixtureDefinition[] {
  return ids.map((id) => DRIVER_FIXTURES[id]);
}

export function journeyAllowed(
  fixture: DriverFixtureDefinition,
  journey: HostContractJourneyId,
): boolean {
  if (fixture.capabilities.connectionMode !== 'sql') {
    return false;
  }
  const reqs = JOURNEY_REQUIREMENTS[journey];
  return reqs.every((key) => Boolean(fixture.capabilities[key]));
}

export function seedTableName(fixture: DriverFixtureDefinition, suffix: string): string {
  const safe = suffix.replace(/[^a-zA-Z0-9_]/g, '_');
  return `${fixture.dialect.tablePrefix}${safe}`;
}

export function filterSeedSql(fixture: DriverFixtureDefinition, table: string): string[] {
  return [
    fixture.dialect.dropTable(table),
    fixture.dialect.createFilterSeedTable(table),
    fixture.dialect.insertFilterSeedRows(table),
  ];
}

export function dataSeedSql(
  fixture: DriverFixtureDefinition,
  table: string,
  rowCount = 60,
): string[] {
  return [
    fixture.dialect.dropTable(table),
    fixture.dialect.createDataSeedTable(table),
    fixture.dialect.insertDataSeedRows(table, rowCount),
  ];
}

export function skipReason(
  fixture: DriverFixtureDefinition,
  journey: HostContractJourneyId,
): string | null {
  if (journeyAllowed(fixture, journey)) return null;
  const missing = JOURNEY_REQUIREMENTS[journey].filter((k) => !fixture.capabilities[k]);
  if (fixture.capabilities.connectionMode !== 'sql') {
    return `${fixture.id}: connectionMode=${fixture.capabilities.connectionMode} (SQL contract skipped)`;
  }
  return `${fixture.id}: missing capabilities ${missing.join(', ')} for ${journey}`;
}
