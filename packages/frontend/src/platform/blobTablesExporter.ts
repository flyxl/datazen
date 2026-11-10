/**
 * Web-shaped tables exporter (form-split F4 dual-flow demonstration, plan §3.3).
 *
 * The other half of the demonstration: the same `exportTables(request)`
 * signature, answered without a host. Table pages come from the (transport-
 * agnostic) table reader the caller injects — on a web build that is the
 * backend-client `getTableData` domain client, bound by the `packages/web`
 * assembly (`assembleWebPlatform`) over the HTTP transport — and the
 * assembled CSV is delivered with a Blob download, which is the browser's
 * own path-free equivalent of the save-dialog token writer.
 *
 * Kept in shared code on purpose: it uses only browser APIs (Blob / URL /
 * anchor click), never the host runtime — R6-clean by construction. The web
 * assembly binds this; the desktop assembly binds the Rust-streaming one;
 * `batchExportJob` and the export UI cannot tell them apart.
 *
 * Alignment with the desktop exporter (export cleanup item 5): file
 * planning rides the shared planner in `./exportPlan` — the TypeScript
 * mirror of the Rust planner (`crates/datazen/src/export/mod.rs`), so
 * extensions, file layout, the DDL-missing placeholder and the default
 * output name follow the same rules on both forms instead of each writing
 * its own. The single downloaded file is the desktop single-file merge:
 * one plan is written directly, several `.sql` plans are concatenated
 * with the same `-- ===== {filename} =====` separators the Rust
 * `run_single_export` merge uses.
 *
 * What the web form cannot do fails loudly instead of silently
 * degrading — the capabilities the browser build has no writer for:
 * - any data format other than CSV for a source that is not already
 *   loaded (`table` / `sql`): the web exporter reads pages and assembles
 *   CSV text only; TSV / JSON / Markdown / SQL / XLSX for those sources
 *   need the desktop exporter. A `provided` source is exempt — its rows
 *   are already in the frontend, so it formats through the shared
 *   `exportFormat.ts` (`formatRowsAsText` / `formatRowsAsXlsx`, plan
 *   §2.8) and every format is available. `structure_only` is exempt
 *   too — it plans no data file, so the format is irrelevant there,
 *   exactly as on the desktop;
 * - `zip` output, and any export whose plan needs a zip (several files
 *   that are not all `.sql`, e.g. multi-table data export or
 *   data-and-structure in CSV): the browser build has no zip writer, so
 *   it refuses rather than renaming a merged CSV to `.zip` or `.csv`;
 * - a `sql` source (a filtered export cannot run here; exporting the
 *   unfiltered table would be wrong rows, not just a wrong format);
 * - structure modes for a SQL / provided-data source without an explicit
 *   `ddl`: there is no table DDL to include, so planning fails instead of
 *   emitting a placeholder that pretends to be a table definition.
 * DDL itself comes from the request's `table.ddl` first (what the batch
 * job already carries), then the injected `loadDdl`, then the shared
 * `-- DDL unavailable for {table}` placeholder for real tables.
 */

import { escapeCsvField } from '../lib/gridClipboard';
import {
  applyCsvFormulaGuard,
  formatRowsAsText,
  formatRowsAsXlsx,
} from '../lib/exportFormat';
import type { TablesExporterPort, TablesExportRequest } from './contracts';
import { defaultExportName, prepareExport, type ExportFilePlan } from './exportPlan';

/** One page of a table, as the injected reader answers it. */
export interface TablePage {
  columns: string[];
  rows: unknown[][];
  /** Paging cursor the reader understands; `null` when the table is exhausted. */
  nextCursor: unknown;
}

/**
 * The request-scoped read target, forwarded to the reader on every page call
 * (WR-E3): `dbSessionId` and the database/schema/driver hints come from the
 * `exportTables(request)` — the reader cannot see the request, so the exporter
 * carries these across the port. The per-table `schema` (the table's own or
 * the request's fallback) overrides the request default per call.
 */
export interface TablePageRequestContext {
  dbSessionId: string;
  database: string | null;
  schema: string | null;
  driverType: string | null;
}

/**
 * Reads one page of one table. The kernel `getTableData` fits this shape —
 * the third argument is the request-scoped context the reader needs to
 * address the read (see {@link TablePageRequestContext}); readers that close
 * over their own target may ignore it.
 */
export type TablePageReader = (
  tableName: string,
  cursor: unknown,
  context: TablePageRequestContext,
) => Promise<TablePage>;

/**
 * One cell as a CSV field: the standard escaping from `gridClipboard`
 * (`escapeCsvField` — RFC 4180 quoting, objects via JSON, null as empty),
 * plus the formula-injection guard shared with the other two CSV paths
 * (`applyCsvFormulaGuard` in `exportFormat.ts` — also the Rust
 * `StreamFormatter` rule). The guard stays out of the shared escaping
 * kernel, where copy/paste treats a formula prefix as inert text; it
 * belongs to the CSV export layer only.
 */
function csvCell(value: unknown): string {
  return applyCsvFormulaGuard(escapeCsvField(value));
}

/**
 * Download `contents` as a file — the browser's path-free save. `contents`
 * is a `BlobPart` so the text half (CSV) and the binary half (base64-decoded
 * bytes, `PlatformServices.saveBinaryWithDialog`) ride one implementation.
 */
export function downloadBlob(contents: BlobPart, fileName: string, mimeType = 'text/csv'): void {
  const blob = new Blob([contents], { type: `${mimeType};charset=utf-8` });
  const url = URL.createObjectURL(blob);
  const anchor = document.createElement('a');
  anchor.href = url;
  anchor.download = fileName;
  anchor.click();
  URL.revokeObjectURL(url);
}

/**
 * Base64 → bytes for an XLSX Blob download (the same decode
 * `ProvidedDataFetcher` does; kept local — importing it from
 * `dataFetcher.ts` would cycle back through this module's `downloadBlob`).
 */
function base64ToBytes(dataBase64: string): Uint8Array<ArrayBuffer> {
  const binary = atob(dataBase64);
  const bytes = new Uint8Array(binary.length);
  for (let i = 0; i < binary.length; i += 1) {
    bytes[i] = binary.charCodeAt(i);
  }
  return bytes;
}

/**
 * Build the exporter over an injected page reader.
 *
 * `data_only` streams the table's rows as CSV; `structure_only` emits only
 * the DDL plans; `data_and_structure` in CSV is a multi-file plan on the
 * desktop (DDL file + data file) and therefore needs a zip — which the web
 * exporter refuses (see the header) instead of merging the two files.
 * A `provided` source is the exception to the CSV-only rule: its rows are
 * already loaded, so every format goes through the shared frontend
 * formatter (`exportFormat.ts`) and downloads as a Blob (plan §2.8).
 */
export function createBlobTablesExporter(
  readPage: TablePageReader,
  options?: {
    /** Loads one table's DDL (structure modes). Absent ⇒ structure modes emit a placeholder comment. */
    loadDdl?: (tableName: string) => Promise<string | null>;
    /**
     * Override for tests; the default performs the real Blob download.
     * `contents` is a `BlobPart` — text for every format except XLSX,
     * which delivers decoded workbook bytes.
     */
    deliver?: (contents: BlobPart, fileName: string, mimeType?: string) => void;
    /** Clock for the default output name; tests pin it, production uses now. */
    now?: () => Date;
  },
): TablesExporterPort {
  const deliver =
    options?.deliver ?? ((contents, fileName, mimeType) => downloadBlob(contents, fileName, mimeType));
  const now = options?.now ?? (() => new Date());
  return {
    async exportTables(
      request: TablesExportRequest,
    ): Promise<{ Saved: number } | { Cancelled: null }> {
      // Capability gates first: fail before any page is read or any file is
      // delivered, so a refused export never leaves a half-written download
      // behind.
      if (request.tables.length === 0) {
        throw new Error('no tables selected');
      }
      // The web exporter assembles CSV text only — except for a `provided`
      // source, whose rows are already loaded and format in JS through
      // `exportFormat.ts` (plan §2.8, task 6). A structure-only export
      // plans no data file, so the format is irrelevant there — exactly as
      // on the desktop, where `build_file_plans` never consults the format
      // for `structure_only`. A mixed request (a provided table next to a
      // table that still has to be read) stays CSV-only: the unread table
      // has no JS formatter to ride.
      const allDataFromProvided = request.tables.every(
        (table) => table.source?.type === 'provided',
      );
      if (
        request.mode !== 'structure_only' &&
        request.dataFormat !== 'csv' &&
        !allDataFromProvided
      ) {
        throw new Error(`'${request.dataFormat}' format is not supported by the web exporter`);
      }
      if (request.outputMode === 'zip') {
        throw new Error('zip output is not supported by the web exporter');
      }
      for (const table of request.tables) {
        // Item 3c: a `sql` source cannot run here at all — refuse honestly
        // rather than export the unfiltered table. The desktop Rust
        // exporter handles it properly. `structure_only` plans no data
        // file, so it is exempt here: planning decides it via
        // `ddl_for_structure` (explicit `ddl` required, as on desktop).
        if (request.mode !== 'structure_only' && table.source?.type === 'sql') {
          throw new Error('filtered export is not supported by the web exporter');
        }
      }

      // Resolve DDL for structure modes before planning: the request's own
      // `table.ddl` first (what the batch job carries — the desktop planner
      // reads the same field), then the injected loader, then nothing and
      // the planner falls back to the shared placeholder for real tables
      // (`ddl_for_structure` still refuses structure modes for SQL /
      // provided-data sources without an explicit `ddl`).
      const tables = await Promise.all(
        request.tables.map(async (table) => {
          if (request.mode === 'data_only') {
            return table;
          }
          const own = table.ddl;
          if (typeof own === 'string' && own.trim().length > 0) {
            return table;
          }
          const loaded = await options?.loadDdl?.(table.tableName);
          if (typeof loaded === 'string' && loaded.trim().length > 0) {
            return { ...table, ddl: loaded };
          }
          return table;
        }),
      );
      const effective: TablesExportRequest = { ...request, tables };
      // Shared planning (same rules as the Rust side): validates sources,
      // plans the files, and decides zip-vs-single. The web build has no
      // zip writer, so a plan that needs a zip is refused here — never a
      // merged CSV masquerading as the requested multi-file export.
      const { plans, useZip } = prepareExport(effective);
      if (useZip) {
        throw new Error(
          'this export produces multiple files and needs a zip archive, ' +
            'which is not supported by the web exporter; export one table at a time ' +
            'in data-only mode, or use the desktop app',
        );
      }

      const requestContext: TablePageRequestContext = {
        dbSessionId: request.dbSessionId,
        database: request.database ?? null,
        schema: request.schema ?? null,
        driverType: request.databaseType ?? null,
      };

      // Emit one planned file. A `provided` source carries the caller's own
      // rows — format them in JS instead of re-reading the table (on web
      // there is no Rust side to hand them to, and silently exporting the
      // whole table for a current-page/selected-rows request would be wrong
      // rows, not just a wrong format). Every format is available here
      // (plan §2.8), with one exception kept local: CSV still goes through
      // the `csvCell` emission below instead of `formatRowsAsText`, because
      // this exporter writes `\n`-terminated lines (the streaming shape the
      // Rust writer produces) while `formatRowsAsText` joins lines with no
      // trailing newline. The formula-injection guard is no longer the
      // reason: it now lives in `exportFormat.ts` (`applyCsvFormulaGuard`)
      // and all three CSV paths share it.
      const emitDataPlan = async (
        plan: Extract<ExportFilePlan, { kind: 'data' }>,
      ): Promise<{ content: BlobPart; mimeType: string }> => {
        const table = plan.table;
        if (table.source?.type === 'provided') {
          const columns = table.source.columns;
          const rows = table.source.rows;
          if (request.dataFormat === 'csv') {
            // No DDL prefix here: `buildFilePlans` only ever prefixes DDL
            // to a SQL-format data plan, so a CSV plan never carries one.
            const chunks: string[] = [];
            chunks.push(columns.map((c) => csvCell(c.name)).join(',') + '\n');
            for (const row of rows) {
              chunks.push(row.map((cell) => csvCell(cell)).join(',') + '\n');
            }
            return { content: chunks.join(''), mimeType: 'text/csv' };
          }
          if (request.dataFormat === 'xlsx') {
            // A DDL prefix cannot ride a workbook; it only ever accompanies
            // a SQL data format, and a SQL data plan is text — reaching
            // here with one would be a planner bug, not a user error.
            if (plan.ddlPrefix !== undefined) {
              throw new Error('DDL cannot be prefixed to an XLSX export');
            }
            const { dataBase64, mimeType } = formatRowsAsXlsx({
              tableName: table.tableName,
              columns,
              rows,
              format: 'xlsx',
            });
            return { content: base64ToBytes(dataBase64), mimeType };
          }
          const { content, mimeType } = formatRowsAsText({
            tableName: table.tableName,
            columns,
            rows,
            format: request.dataFormat,
            databaseType: request.databaseType ?? undefined,
            primaryKey: table.primaryKey ?? undefined,
          });
          return {
            content:
              plan.ddlPrefix !== undefined ? `${plan.ddlPrefix}\n\n${content}` : content,
            mimeType,
          };
        }
        const chunks: string[] = [];
        if (plan.ddlPrefix !== undefined) {
          chunks.push(`${plan.ddlPrefix}\n\n`);
        }
        let cursor: unknown = null;
        let first = true;
        // Page until the reader says the table is exhausted; each page is
        // appended so peak memory is one page, mirroring the streaming form.
        for (;;) {
          const page = await readPage(table.tableName, cursor, {
            ...requestContext,
            schema: table.schema ?? requestContext.schema,
          });
          if (first) {
            chunks.push(page.columns.map(csvCell).join(',') + '\n');
            first = false;
          }
          for (const row of page.rows) {
            chunks.push(row.map((cell) => csvCell(cell)).join(',') + '\n');
          }
          cursor = page.nextCursor;
          if (cursor === null || cursor === undefined) break;
        }
        return { content: chunks.join(''), mimeType: 'text/csv' };
      };

      const fileName = defaultExportName(effective, false, now());
      // A single data plan delivers as-is (binary for XLSX, text for the
      // rest) with its own MIME type. Several plans only reach here when
      // they are all `.sql` — anything else needed a zip and was refused
      // above — so the merge below is text-only by construction.
      if (plans.length === 1 && plans[0]?.kind === 'data') {
        const emitted = await emitDataPlan(plans[0]);
        deliver(emitted.content, fileName, emitted.mimeType);
        return { Saved: request.tables.length };
      }
      const chunks: string[] = [];
      // The desktop single-file merge: one plan is written directly, several
      // plans are concatenated with `-- ===== {filename} =====` separators.
      // (Several plans only reach here when they are all `.sql` — anything
      // else needed a zip and was refused above.)
      const multi = plans.length > 1;
      for (const plan of plans) {
        if (multi) {
          chunks.push(`-- ===== ${plan.filename} =====\n\n`);
        }
        if (plan.kind === 'ddl') {
          chunks.push(plan.text);
        } else {
          const emitted = await emitDataPlan(plan);
          if (typeof emitted.content !== 'string') {
            throw new Error(
              'this export produces a binary file that cannot be merged into a single download',
            );
          }
          chunks.push(emitted.content);
        }
        if (multi) {
          chunks.push('\n\n');
        }
      }
      deliver(chunks.join(''), fileName);
      return { Saved: request.tables.length };
    },
  };
}
