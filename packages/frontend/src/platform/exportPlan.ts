/**
 * Shared export file planning (export cleanup item 5).
 *
 * The TypeScript mirror of the Rust batch-export planner in
 * `crates/datazen/src/export/mod.rs` (`DataFormat::extension`,
 * `resolve_ddl`, `ddl_for_structure`, `build_file_plans`,
 * `prepare_export`, `unique_filename`, `default_export_name`) plus the
 * source validation in `TableDataSource::validate` /
 * `validate_provided_data` (`fetcher.rs`).
 *
 * The Rust module is the single source of truth for export file layout;
 * this file exists because the web exporter cannot call Rust — it must
 * plan with the same rules instead of inventing its own (the old web
 * exporter named files `{first}-tables.csv`, invented its own DDL
 * placeholder, and silently merged multi-file exports into one CSV).
 * Keep the two in sync: same extensions, same placeholder wording, same
 * DDL-per-source rule, same zip rule, same default-name shape. If a rule
 * changes on either side, change both and say so in the commit.
 *
 * Error wording mirrors the Rust `Validation` messages verbatim so a web
 * failure and a desktop failure for the same request read the same.
 */

import type { ExportDataFormat, ExportTableInput, ExportTablesRequest } from '../commands/file';

/** File extension of one data format — `DataFormat::extension`. */
export function formatExtension(format: ExportDataFormat): string {
  switch (format) {
    case 'csv':
      return 'csv';
    case 'tsv':
      return 'tsv';
    case 'json':
      return 'json';
    case 'markdown':
      return 'md';
    case 'sql_insert':
    case 'sql_update':
      return 'sql';
    case 'xlsx':
      return 'xlsx';
  }
}

/** Binary formats ride the parallel `BinaryFormatter` branch and can never
 * be concatenated as text — `DataFormat::is_binary` (today: XLSX only). */
export function isBinaryFormat(format: ExportDataFormat): boolean {
  return format === 'xlsx';
}

/** SQL data formats carry their DDL as a prefix in the same `.sql` file. */
export function isSqlDataFormat(format: ExportDataFormat): boolean {
  return format === 'sql_insert' || format === 'sql_update';
}

/** One planned output file — `FilePlan` in `export/mod.rs`. */
export type ExportFilePlan =
  | { kind: 'ddl'; filename: string; text: string }
  | {
      kind: 'data';
      filename: string;
      table: ExportTableInput;
      format: ExportDataFormat;
      /** DDL text prefixed to the same data file (only for sql + structure). */
      ddlPrefix?: string;
    };

export function planFilename(plan: ExportFilePlan): string {
  return plan.filename;
}

/**
 * DDL text for one table — `resolve_ddl`: the caller's `ddl` when it
 * carries non-blank text, else the shared placeholder. The placeholder
 * names the bare table, exactly as the Rust side does.
 */
export function resolveDdl(table: ExportTableInput): string {
  const ddl = table.ddl;
  if (typeof ddl === 'string' && ddl.trim().length > 0) {
    return ddl;
  }
  return `-- DDL unavailable for ${table.tableName}`;
}

/**
 * DDL text for a structure-mode plan — `ddl_for_structure`.
 *
 * A real table may fall back to the placeholder, but a SQL / provided-data
 * source has no table whose DDL could be merged: structure modes require
 * the caller to supply explicit `ddl` text for these items, and planning
 * fails with a clear error instead of emitting a placeholder that
 * pretends to be a table definition.
 */
export function ddlForStructure(table: ExportTableInput): string {
  const source = table.source;
  if (source?.type === 'sql' || source?.type === 'provided') {
    const ddl = table.ddl;
    if (typeof ddl === 'string' && ddl.trim().length > 0) {
      return ddl;
    }
    throw new Error(
      `structure export for non-table source '${table.tableName}' needs an explicit ddl: ` +
        'SQL and provided-data sources have no table DDL to include',
    );
  }
  return resolveDdl(table);
}

/**
 * Fail fast on a malformed source, before any file is planned or written
 * — `TableDataSource::validate` + `validate_provided_data`.
 */
export function validateTableSource(table: ExportTableInput): void {
  const source = table.source;
  if (source === undefined || source.type === 'table') {
    return;
  }
  if (source.type === 'sql') {
    if (source.sql.trim().length === 0) {
      throw new Error(`SQL data source '${table.tableName}' must not be empty`);
    }
    return;
  }
  const { columns, rows } = source;
  if (columns.length === 0) {
    throw new Error(`provided data source '${table.tableName}' has no columns (empty schema)`);
  }
  for (let i = 0; i < columns.length; i += 1) {
    const column = columns[i]!;
    if (columns.slice(0, i).some((c) => c.name === column.name)) {
      throw new Error(
        `provided data source '${table.tableName}' has a duplicate column name '${column.name}'`,
      );
    }
  }
  for (let i = 0; i < rows.length; i += 1) {
    const row = rows[i]!;
    if (row.length !== columns.length) {
      throw new Error(
        `provided data source '${table.tableName}' row ${i} has ${row.length} values ` +
          `but the schema has ${columns.length} columns`,
      );
    }
  }
}

/**
 * De-duplicated `.sql` filename — `unique_filename`. Only DDL plans and
 * SQL-format data plans go through it (a data plan's `{table}.{ext}` is
 * used verbatim, as on the Rust side).
 */
export function uniqueFilename(seen: Set<string>, name: string): string {
  if (!seen.has(name)) {
    seen.add(name);
    return name;
  }
  let stem = name;
  while (stem.endsWith('.sql')) {
    stem = stem.slice(0, -'.sql'.length);
  }
  let n = 1;
  for (;;) {
    const candidate = `${stem}.${n}.sql`;
    if (!seen.has(candidate)) {
      seen.add(candidate);
      return candidate;
    }
    n += 1;
  }
}

/**
 * Plan the output files for a request — `build_file_plans`.
 *
 * - `structure_only`: one `{table}.sql` DDL file per table.
 * - `data_only`: one `{table}.{ext}` data file per table.
 * - `data_and_structure`: SQL data formats carry their DDL as a prefix in
 *   the same `{table}.sql` file; every other format splits DDL
 *   (`{table}.sql`) and data (`{table}.{ext}`).
 */
export function buildFilePlans(request: ExportTablesRequest): ExportFilePlan[] {
  for (const table of request.tables) {
    validateTableSource(table);
  }
  const seen = new Set<string>();
  const plans: ExportFilePlan[] = [];
  for (const table of request.tables) {
    const base = table.tableName;
    if (request.mode === 'structure_only') {
      plans.push({
        kind: 'ddl',
        filename: uniqueFilename(seen, `${base}.sql`),
        text: ddlForStructure(table),
      });
    } else if (request.mode === 'data_only') {
      plans.push({
        kind: 'data',
        filename: `${base}.${formatExtension(request.dataFormat)}`,
        table,
        format: request.dataFormat,
      });
    } else if (isSqlDataFormat(request.dataFormat)) {
      plans.push({
        kind: 'data',
        filename: uniqueFilename(seen, `${base}.sql`),
        table,
        format: request.dataFormat,
        ddlPrefix: ddlForStructure(table),
      });
    } else {
      plans.push({
        kind: 'ddl',
        filename: uniqueFilename(seen, `${base}.sql`),
        text: ddlForStructure(table),
      });
      plans.push({
        kind: 'data',
        filename: `${base}.${formatExtension(request.dataFormat)}`,
        table,
        format: request.dataFormat,
      });
    }
  }
  return plans;
}

/**
 * Plan the output container — `prepare_export`.
 *
 * `useZip` is true when the caller asked for zip output, or when the plan
 * has more than one file and they are not all `.sql` (a multi-file
 * non-SQL export has nowhere to go as a single file). A single file, or
 * several `.sql` files, merge into one target instead.
 */
export function prepareExport(request: ExportTablesRequest): {
  plans: ExportFilePlan[];
  useZip: boolean;
} {
  const plans = buildFilePlans(request);
  if (plans.length === 0) {
    throw new Error('no tables selected');
  }
  const useZip =
    request.outputMode === 'zip' ||
    (plans.length > 1 && !plans.every((plan) => plan.filename.endsWith('.sql')));
  return { plans, useZip };
}

/**
 * Default output name — `default_export_name`: `{prefix}_{UTC timestamp}.{ext}`
 * with `export_structure` / `export_data` / `export_full` by mode and
 * `zip` (or the data format's extension) as the extension. `now` is
 * injectable so tests can pin the timestamp; production passes the
 * current time (UTC, as `chrono::Utc` does on the Rust side).
 */
export function defaultExportName(
  request: ExportTablesRequest,
  useZip: boolean,
  now: Date = new Date(),
): string {
  const pad = (n: number): string => String(n).padStart(2, '0');
  const ts =
    `${now.getUTCFullYear()}-${pad(now.getUTCMonth() + 1)}-${pad(now.getUTCDate())}` +
    `-${pad(now.getUTCHours())}-${pad(now.getUTCMinutes())}-${pad(now.getUTCSeconds())}`;
  const prefix =
    request.mode === 'structure_only'
      ? 'export_structure'
      : request.mode === 'data_only'
        ? 'export_data'
        : 'export_full';
  const ext = useZip ? 'zip' : formatExtension(request.dataFormat);
  return `${prefix}_${ts}.${ext}`;
}
