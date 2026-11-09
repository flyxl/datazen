import { describe, expect, it } from 'vitest';

import type { ExportTablesRequest } from '../../commands/file';
import { buildProvidedExportFilename } from '../../lib/exportFormat';
import type { ExportDataFormat } from '../../commands/file';
import {
  buildFilePlans,
  ddlForStructure,
  defaultExportName,
  formatExtension,
  isBinaryFormat,
  isSqlDataFormat,
  prepareExport,
  resolveDdl,
  uniqueFilename,
  validateTableSource,
} from '../exportPlan';

/**
 * Export cleanup item 5: the shared planner (`exportPlan.ts`) is the
 * TypeScript mirror of the Rust planner (`crates/datazen/src/export/
 * mod.rs`) — same extensions, file layout, DDL placeholder, zip rule and
 * default name. These tests pin the mirror so a drift on either side
 * fails here instead of shipping two planners.
 */

const REQUEST = (overrides?: Partial<ExportTablesRequest>): ExportTablesRequest => ({
  dbSessionId: 'sess-1',
  mode: 'data_only',
  dataFormat: 'csv',
  outputMode: 'single',
  tables: [{ tableName: 'users', columns: ['id', 'name'] }],
  ...overrides,
});

const PINNED_NOW = new Date('2026-11-09T05:00:00.000Z');

describe('formatExtension / isBinaryFormat / isSqlDataFormat', () => {
  it('maps every format to the Rust DataFormat::extension', () => {
    expect(formatExtension('csv')).toBe('csv');
    expect(formatExtension('tsv')).toBe('tsv');
    expect(formatExtension('json')).toBe('json');
    expect(formatExtension('markdown')).toBe('md');
    expect(formatExtension('sql_insert')).toBe('sql');
    expect(formatExtension('sql_update')).toBe('sql');
    expect(formatExtension('xlsx')).toBe('xlsx');
  });

  it('marks only xlsx binary', () => {
    expect(isBinaryFormat('xlsx')).toBe(true);
    expect(isBinaryFormat('csv')).toBe(false);
    expect(isBinaryFormat('sql_insert')).toBe(false);
  });

  it('marks only the sql formats as sql formats', () => {
    expect(isSqlDataFormat('sql_insert')).toBe(true);
    expect(isSqlDataFormat('sql_update')).toBe(true);
    expect(isSqlDataFormat('csv')).toBe(false);
  });
});

describe('resolveDdl / ddlForStructure', () => {
  it('uses the caller-supplied ddl when it carries text', () => {
    expect(
      resolveDdl({ tableName: 'users', columns: [], ddl: 'CREATE TABLE users (id INT);' }),
    ).toBe('CREATE TABLE users (id INT);');
  });

  it('falls back to the shared placeholder for a real table (Rust wording, bare table name)', () => {
    expect(resolveDdl({ tableName: 'users', columns: [] })).toBe('-- DDL unavailable for users');
    expect(resolveDdl({ tableName: 'users', columns: [], ddl: '   ' })).toBe(
      '-- DDL unavailable for users',
    );
    expect(resolveDdl({ tableName: 'users', columns: [], ddl: null })).toBe(
      '-- DDL unavailable for users',
    );
  });

  it('lets a table source fall back to the placeholder in structure modes', () => {
    expect(ddlForStructure({ tableName: 'users', columns: [] })).toBe(
      '-- DDL unavailable for users',
    );
  });

  it('accepts explicit ddl for sql / provided sources', () => {
    expect(
      ddlForStructure({
        tableName: 'users',
        columns: [],
        ddl: 'CREATE TABLE users (id INT);',
        source: { type: 'sql', sql: 'SELECT id FROM users' },
      }),
    ).toBe('CREATE TABLE users (id INT);');
    expect(
      ddlForStructure({
        tableName: 'users',
        columns: [],
        ddl: 'CREATE TABLE users (id INT);',
        source: { type: 'provided', columns: [], rows: [] },
      }),
    ).toBe('CREATE TABLE users (id INT);');
  });

  it('refuses structure modes for sql / provided sources without explicit ddl', () => {
    expect(() =>
      ddlForStructure({
        tableName: 'users',
        columns: [],
        source: { type: 'sql', sql: 'SELECT id FROM users' },
      }),
    ).toThrow(
      "structure export for non-table source 'users' needs an explicit ddl: " +
        'SQL and provided-data sources have no table DDL to include',
    );
    expect(() =>
      ddlForStructure({
        tableName: 'users',
        columns: [],
        source: { type: 'provided', columns: [], rows: [] },
      }),
    ).toThrow("structure export for non-table source 'users' needs an explicit ddl");
  });
});

describe('buildProvidedExportFilename parity with build_file_plans (task 6)', () => {
  it.each([
    'csv',
    'tsv',
    'json',
    'markdown',
    'sql_insert',
    'sql_update',
    'xlsx',
  ] as const)(
    'names a provided %s export exactly like the Rust data-only plan ({table}.{ext})',
    (dataFormat: ExportDataFormat) => {
      const plans = buildFilePlans(
        REQUEST({ dataFormat, tables: [{ tableName: 'users', columns: [] }] }),
      );
      expect(plans).toHaveLength(1);
      expect(plans[0]).toMatchObject({ kind: 'data' });
      expect(plans[0]!.filename).toBe(buildProvidedExportFilename('users', dataFormat));
      expect(plans[0]!.filename).toBe(`users.${formatExtension(dataFormat)}`);
    },
  );
});

describe('validateTableSource', () => {
  it('accepts table sources and empty sql-free requests', () => {
    expect(() => validateTableSource({ tableName: 'users', columns: [] })).not.toThrow();
    expect(() =>
      validateTableSource({
        tableName: 'users',
        columns: [],
        source: { type: 'sql', sql: 'SELECT 1' },
      }),
    ).not.toThrow();
  });

  it('rejects an empty sql source', () => {
    expect(() =>
      validateTableSource({ tableName: 'users', columns: [], source: { type: 'sql', sql: '  ' } }),
    ).toThrow("SQL data source 'users' must not be empty");
  });

  it('rejects provided data without columns, with duplicate columns, or with a short row', () => {    expect(() =>
      validateTableSource({
        tableName: 'users',
        columns: [],
        source: { type: 'provided', columns: [], rows: [] },
      }),
    ).toThrow("provided data source 'users' has no columns (empty schema)");
    expect(() =>
      validateTableSource({
        tableName: 'users',
        columns: [],
        source: {
          type: 'provided',
          columns: [
            { name: 'id', dataType: 'int', nullable: false },
            { name: 'id', dataType: 'int', nullable: false },
          ],
          rows: [],
        },
      }),
    ).toThrow("provided data source 'users' has a duplicate column name 'id'");
    expect(() =>
      validateTableSource({
        tableName: 'users',
        columns: [],
        source: {
          type: 'provided',
          columns: [
            { name: 'id', dataType: 'int', nullable: false },
            { name: 'name', dataType: 'text', nullable: true },
          ],
          rows: [[1]],
        },
      }),
    ).toThrow("provided data source 'users' row 0 has 1 values but the schema has 2 columns");
  });

  it('is the only provided check: planning funnels malformed provided data through it (task 6)', () => {
    // The Rust provided source is gone (task 5); `buildFilePlans`
    // must reject here, with this message, not somewhere downstream.
    expect(() =>
      buildFilePlans(
        REQUEST({
          tables: [
            {
              tableName: 'users',
              columns: [],
              source: {
                type: 'provided',
                columns: [
                  { name: 'id', dataType: 'int', nullable: false },
                  { name: 'id', dataType: 'int', nullable: false },
                ],
                rows: [],
              },
            },
          ],
        }),
      ),
    ).toThrow("provided data source 'users' has a duplicate column name 'id'");
  });
});

describe('uniqueFilename', () => {
  it('de-duplicates .sql names with a numeric suffix', () => {
    const seen = new Set<string>();
    expect(uniqueFilename(seen, 'users.sql')).toBe('users.sql');
    expect(uniqueFilename(seen, 'users.sql')).toBe('users.1.sql');
    expect(uniqueFilename(seen, 'users.sql')).toBe('users.2.sql');
    expect(uniqueFilename(seen, 'orders.sql')).toBe('orders.sql');
  });
});

describe('buildFilePlans', () => {
  it('structure_only plans one .sql DDL file per table', () => {
    const plans = buildFilePlans(
      REQUEST({
        mode: 'structure_only',
        tables: [
          { tableName: 'users', columns: [], ddl: 'CREATE TABLE users (id INT);' },
          { tableName: 'orders', columns: [] },
        ],
      }),
    );
    expect(plans).toEqual([
      { kind: 'ddl', filename: 'users.sql', text: 'CREATE TABLE users (id INT);' },
      { kind: 'ddl', filename: 'orders.sql', text: '-- DDL unavailable for orders' },
    ]);
  });

  it('data_only plans one data file per table with the format extension', () => {
    const plans = buildFilePlans(
      REQUEST({
        dataFormat: 'json',
        tables: [
          { tableName: 'users', columns: [] },
          { tableName: 'orders', columns: [] },
        ],
      }),
    );
    expect(plans.map((p) => p.filename)).toEqual(['users.json', 'orders.json']);
    expect(plans.every((p) => p.kind === 'data')).toBe(true);
  });

  it('data_and_structure splits DDL and data for text formats', () => {
    const plans = buildFilePlans(
      REQUEST({
        mode: 'data_and_structure',
        tables: [{ tableName: 'users', columns: [], ddl: 'CREATE TABLE users (id INT);' }],
      }),
    );
    expect(plans).toEqual([
      { kind: 'ddl', filename: 'users.sql', text: 'CREATE TABLE users (id INT);' },
      {
        kind: 'data',
        filename: 'users.csv',
        table: expect.objectContaining({ tableName: 'users' }),
        format: 'csv',
      },
    ]);
  });

  it('data_and_structure prefixes DDL into the same .sql file for sql formats', () => {
    const plans = buildFilePlans(
      REQUEST({
        mode: 'data_and_structure',
        dataFormat: 'sql_insert',
        tables: [{ tableName: 'users', columns: [], ddl: 'CREATE TABLE users (id INT);' }],
      }),
    );
    expect(plans).toEqual([
      {
        kind: 'data',
        filename: 'users.sql',
        table: expect.objectContaining({ tableName: 'users' }),
        format: 'sql_insert',
        ddlPrefix: 'CREATE TABLE users (id INT);',
      },
    ]);
  });

  it('de-duplicates colliding .sql names across tables', () => {
    const plans = buildFilePlans(
      REQUEST({
        mode: 'structure_only',
        tables: [
          { tableName: 'users', columns: [] },
          { tableName: 'users', columns: [] },
        ],
      }),
    );
    expect(plans.map((p) => p.filename)).toEqual(['users.sql', 'users.1.sql']);
  });
});

describe('prepareExport', () => {
  it('refuses an empty table list', () => {
    expect(() => prepareExport(REQUEST({ tables: [] }))).toThrow('no tables selected');
  });

  it('routes zip output to zip', () => {
    const { useZip } = prepareExport(REQUEST({ outputMode: 'zip' }));
    expect(useZip).toBe(true);
  });

  it('routes multi-table non-sql plans to zip even in single mode', () => {
    const { useZip } = prepareExport(
      REQUEST({
        tables: [
          { tableName: 'users', columns: [] },
          { tableName: 'orders', columns: [] },
        ],
      }),
    );
    expect(useZip).toBe(true);
  });

  it('keeps a single data table and multi-table all-.sql plans as a single file', () => {
    expect(prepareExport(REQUEST()).useZip).toBe(false);
    expect(
      prepareExport(
        REQUEST({
          mode: 'structure_only',
          tables: [
            { tableName: 'users', columns: [] },
            { tableName: 'orders', columns: [] },
          ],
        }),
      ).useZip,
    ).toBe(false);
  });
});

describe('defaultExportName', () => {
  it('names data / structure / full exports with the UTC timestamp and format extension', () => {
    expect(defaultExportName(REQUEST(), false, PINNED_NOW)).toBe(
      'export_data_2026-11-09-05-00-00.csv',
    );
    expect(defaultExportName(REQUEST({ mode: 'structure_only' }), false, PINNED_NOW)).toBe(
      'export_structure_2026-11-09-05-00-00.csv',
    );
    expect(defaultExportName(REQUEST({ mode: 'data_and_structure' }), false, PINNED_NOW)).toBe(
      'export_full_2026-11-09-05-00-00.csv',
    );
    expect(defaultExportName(REQUEST({ dataFormat: 'xlsx' }), false, PINNED_NOW)).toBe(
      'export_data_2026-11-09-05-00-00.xlsx',
    );
  });

  it('uses .zip when the plan needs a zip', () => {
    expect(defaultExportName(REQUEST(), true, PINNED_NOW)).toBe(
      'export_data_2026-11-09-05-00-00.zip',
    );
  });
});
