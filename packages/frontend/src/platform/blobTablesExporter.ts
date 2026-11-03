/**
 * Web-shaped tables exporter (form-split F4 dual-flow demonstration, plan §3.3).
 *
 * The other half of the demonstration: the same `exportTables(request)`
 * signature, answered without a host. Table pages come from the (transport-
 * agnostic) table reader the caller injects — on a web build that is the
 * backend-client `getTableData` domain client — and the assembled CSV is
 * delivered with a Blob download, which is the browser's own path-free
 * equivalent of the save-dialog token writer.
 *
 * Kept in shared code on purpose: it uses only browser APIs (Blob / URL /
 * anchor click), never the host runtime — R6-clean by construction. The web
 * assembly binds this; the desktop assembly binds the Rust-streaming one;
 * `batchExportJob` and the export UI cannot tell them apart.
 */

import type { TablesExporterPort, TablesExportRequest } from './contracts';

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

function csvCell(value: unknown): string {
  if (value === null || value === undefined) return '';
  const text = typeof value === 'string' ? value : JSON.stringify(value);
  // Quote when the value could re-enter the grid as two fields or a formula.
  if (/[",\n\r]/.test(text) || /^[=+\-@]/.test(text)) {
    return `"${text.replace(/"/g, '""')}"`;
  }
  return text;
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
 * Build the exporter over an injected page reader.
 *
 * `data_only` and `data_and_structure` both stream the table's rows as CSV
 * (structure exports carry the DDL as a leading comment per table);
 * `structure_only` emits only the DDL comments.
 */
export function createBlobTablesExporter(
  readPage: TablePageReader,
  options?: {
    /** Loads one table's DDL (structure modes). Absent ⇒ structure modes emit a placeholder comment. */
    loadDdl?: (tableName: string) => Promise<string | null>;
    /** Override for tests; the default performs the real Blob download. */
    deliver?: (contents: string, fileName: string) => void;
  },
): TablesExporterPort {
  const deliver = options?.deliver ?? downloadBlob;
  return {
    async exportTables(
      request: TablesExportRequest,
    ): Promise<{ Saved: number } | { Cancelled: null }> {
      const chunks: string[] = [];
      const requestContext: TablePageRequestContext = {
        dbSessionId: request.dbSessionId,
        database: request.database ?? null,
        schema: request.schema ?? null,
        driverType: request.databaseType ?? null,
      };
      for (const table of request.tables) {
        const name = table.schema ? `${table.schema}.${table.tableName}` : table.tableName;
        if (request.mode !== 'data_only') {
          const ddl =
            (await options?.loadDdl?.(table.tableName)) ?? `-- structure for ${name} unavailable`;
          chunks.push(`-- ${name}\n${ddl}\n`);
        }
        if (request.mode !== 'structure_only') {
          chunks.push(`-- data for ${name}\n`);
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
              chunks.push(row.map(csvCell).join(',') + '\n');
            }
            cursor = page.nextCursor;
            if (cursor === null || cursor === undefined) break;
          }
        }
      }
      const fileName =
        request.outputMode === 'zip'
          ? `${request.tables[0]?.tableName ?? 'export'}-tables.csv`
          : `${request.tables[0]?.tableName ?? 'export'}.csv`;
      deliver(chunks.join(''), fileName);
      return { Saved: request.tables.length };
    },
  };
}
