/**
 * Clipboard copy/paste for the data grid (DB-03) — pure functions only.
 *
 * No React, no IPC, no store: everything here is a string/array computation, so the
 * malformed-input cases the chapter requires (ragged rows, unclosed quotes, tabs and
 * newlines inside values, emoji, byte/cell limits) can be tested without a component tree.
 * `src/lib/` does not depend on `src/stores/`, and this file keeps that direction: the
 * adapter between the selection model's `CellCoord` and this module's coordinates lives in
 * `src/hooks/useGridClipboard.ts`, which may import both.
 *
 * Two hard rules from the chapter live here:
 *
 * 1. **One serializer.** `serializeCells` is the only TSV/CSV/JSON kernel; the eight
 *    existing `dataTableContextMenu` helpers delegate to it (Step 9) so copy and export
 *    can never drift apart on escaping rules again.
 * 2. **The clipboard is a data interchange channel, not a screenshot.** Values always come
 *    from `TableState.rows`; nothing here reads the DOM, so a value the renderer truncates
 *    to 120 characters is still copied in full (§3.8 R-A/R-D).
 */
import type { PasteProblem } from './gridErrors';

// ── Limits (one place, so they can be tuned without hunting through a hook) ───────────

/** Bytes of source text above which a paste is refused outright. */
export const GRID_PASTE_MAX_BYTES = 10 * 1024 * 1024;
/** Cells above which a paste is refused outright. */
export const GRID_PASTE_MAX_CELLS = 100_000;
/** Above this cell count the caller asks for confirmation; below it runs silently. */
export const GRID_PASTE_CONFIRM_CELLS = 2_000;
/** Cells written per state patch; a larger paste is chunked so a frame stays under ~200ms. */
export const GRID_PASTE_CHUNK_CELLS = 5_000;

/** Delimiters the parser understands. Tab wins because that is what spreadsheets emit. */
export type ClipboardDelimiter = '\t' | ',' | ';';

/** A structural equivalent of DB-01's `CellCoord`, kept local to preserve the `lib/` layering. */
export interface ClipboardCellCoord {
  rowIndex: number;
  /** Column **name**: an index drifts as soon as a column is hidden or moved. */
  columnName: string;
}

/** A rectangular landing area. Columns by name, rows by current-page index. */
export interface GridClipboardRange {
  topRow: number;
  bottomRow: number;
  leftColumn: string;
  rightColumn: string;
}

export type GridClipboardAnchor = ClipboardCellCoord;

/** A parsed two-dimensional block plus the per-cell quoting that carries NULL semantics. */
export interface ClipboardMatrix {
  /** Cell text: quotes stripped, `""` restored to `"`. At least one row, all rows equal width. */
  cells: string[][];
  /** Same shape as `cells`. `true` = the source quoted this field, so `''` is an explicit empty string. */
  quoted: boolean[][];
  delimiter: ClipboardDelimiter;
  /** The source had at least one line break (used to tell "one cell" from "N rows"). */
  multiline: boolean;
  /** UTF-8 byte length of the source text. */
  byteLength: number;
}

export type ClipboardParseResult =
  | { ok: true; matrix: ClipboardMatrix; problems: PasteProblem[] }
  | {
      ok: false;
      code: 'grid.paste.empty' | 'grid.paste.tooLarge' | 'grid.paste.raggedRow';
      cells: number;
      byteLength: number;
      /** First inconsistent matrix row (0-based) — `grid.paste.raggedRow` only. */
      row?: number;
      /** Field count of the first row — `grid.paste.raggedRow` only. */
      expected?: number;
      /** Field count of the offending row — `grid.paste.raggedRow` only. */
      actual?: number;
    };

/**
 * UTF-8 byte length without `TextEncoder`: the counting has to work in every test
 * environment, and a code-unit loop is exact (a surrogate pair counts 4 bytes).
 */
export function utf8ByteLength(text: string): number {
  let bytes = 0;
  for (let index = 0; index < text.length; index += 1) {
    const code = text.codePointAt(index) ?? 0;
    if (code < 0x80) bytes += 1;
    else if (code < 0x800) bytes += 2;
    else if (code < 0x10000) bytes += 3;
    else {
      bytes += 4;
      index += 1;
    }
  }
  return bytes;
}

/**
 * Sniff the delimiter **outside quotes**: tab > comma > semicolon.
 *
 * When none of them occurs the block is one column — deliberately **not** a comma, because
 * guessing a comma would shred a single-cell text value containing one.
 */
export function sniffClipboardDelimiter(text: string): ClipboardDelimiter {
  let tabs = 0;
  let commas = 0;
  let semicolons = 0;
  let inQuotes = false;
  let index = 0;
  while (index < text.length) {
    const char = text[index];
    if (inQuotes) {
      if (char === '"') {
        if (text[index + 1] === '"') {
          index += 2;
          continue;
        }
        inQuotes = false;
      }
      index += 1;
      continue;
    }
    if (char === '"') {
      inQuotes = true;
      index += 1;
      continue;
    }
    if (char === '\t') tabs += 1;
    else if (char === ',') commas += 1;
    else if (char === ';') semicolons += 1;
    index += 1;
  }
  if (tabs > 0) return '\t';
  if (commas > 0) return ',';
  if (semicolons > 0) return ';';
  return '\t';
}

interface ScannedBlock {
  cells: string[][];
  quoted: boolean[][];
  unclosedQuote: boolean;
}

/**
 * Scan the block one **code unit** at a time.
 *
 * Splitting on `''` or slicing by `length` would tear surrogate pairs apart; a per-code-unit
 * loop that only ever looks at ASCII (delimiter, `"`, CR, LF) cannot, because every
 * multi-byte character is simply copied through.
 */
function scanBlock(text: string, delimiter: ClipboardDelimiter): ScannedBlock {
  const cells: string[][] = [];
  const quoted: boolean[][] = [];
  let row: string[] = [];
  let rowQuoted: boolean[] = [];
  let field = '';
  let fieldStarted = false;
  let fieldQuoted = false;
  let inQuotes = false;
  let lastWasRowBreak = false;
  let index = 0;

  const endField = (): void => {
    row.push(field);
    rowQuoted.push(fieldQuoted);
    field = '';
    fieldStarted = false;
    fieldQuoted = false;
  };
  const endRow = (): void => {
    endField();
    cells.push(row);
    quoted.push(rowQuoted);
    row = [];
    rowQuoted = [];
    lastWasRowBreak = true;
  };

  while (index < text.length) {
    const char = text[index];
    if (inQuotes) {
      if (char === '"') {
        if (text[index + 1] === '"') {
          field += '"';
          index += 2;
          continue;
        }
        inQuotes = false;
        index += 1;
        continue;
      }
      if (char === '\r') {
        field += '\n';
        index += text[index + 1] === '\n' ? 2 : 1;
        continue;
      }
      field += char;
      lastWasRowBreak = false;
      index += 1;
      continue;
    }
    if (char === '"' && !fieldStarted) {
      inQuotes = true;
      fieldQuoted = true;
      fieldStarted = true;
      lastWasRowBreak = false;
      index += 1;
      continue;
    }
    if (char === delimiter) {
      endField();
      lastWasRowBreak = false;
      index += 1;
      continue;
    }
    if (char === '\n') {
      endRow();
      index += 1;
      continue;
    }
    if (char === '\r') {
      endRow();
      index += text[index + 1] === '\n' ? 2 : 1;
      continue;
    }
    field += char;
    fieldStarted = true;
    lastWasRowBreak = false;
    index += 1;
  }

  const unclosedQuote = inQuotes;
  // A trailing line break must not fabricate an empty row: `'a\n'` is one row, not two.
  if (!lastWasRowBreak) endRow();
  return { cells, quoted, unclosedQuote };
}

/**
 * Parse clipboard text into a rectangular block (RFC 4180 style quoting).
 *
 * Refusals are structured and total: an empty block, a block over the byte/cell limit and a
 * block whose rows disagree on width never produce partial output — a half-applied paste is
 * worse than a refusal. An unclosed quote is **not** a refusal: everything after it becomes
 * one cell, and the caller is told so.
 */
export function parseClipboardMatrix(text: string): ClipboardParseResult {
  const byteLength = utf8ByteLength(text);
  /**
   * Empty means "there is nothing here", not "there is no visible character": a block of
   * unquoted empty fields (`'\t'`, `';'`) is a perfectly good paste that writes NULLs, and
   * refusing it as empty would contradict the NULL/empty rules the planner implements.
   */
  const hasDelimiter = /[\t,;"]/.test(text);
  if (!hasDelimiter && text.trim() === '') {
    return { ok: false, code: 'grid.paste.empty', cells: 0, byteLength };
  }
  if (byteLength > GRID_PASTE_MAX_BYTES) {
    return { ok: false, code: 'grid.paste.tooLarge', cells: 0, byteLength };
  }

  const delimiter = sniffClipboardDelimiter(text);
  const block = scanBlock(text, delimiter);
  const width = block.cells[0]?.length ?? 0;
  const cellCount = block.cells.length * width;
  const raggedRow = block.cells.findIndex((row) => row.length !== width);
  if (raggedRow >= 0) {
    return {
      ok: false,
      code: 'grid.paste.raggedRow',
      cells: cellCount,
      byteLength,
      row: raggedRow,
      expected: width,
      actual: block.cells[raggedRow]?.length ?? 0,
    };
  }
  if (cellCount > GRID_PASTE_MAX_CELLS) {
    return { ok: false, code: 'grid.paste.tooLarge', cells: cellCount, byteLength };
  }

  const problems: PasteProblem[] = [];
  if (block.unclosedQuote) {
    problems.push({ code: 'grid.paste.unclosedQuote', row: null, column: null });
  }
  return {
    ok: true,
    matrix: {
      cells: block.cells,
      quoted: block.quoted,
      delimiter,
      multiline: text.includes('\n') || text.includes('\r'),
      byteLength,
    },
    problems,
  };
}

// ── Selection → matrix ───────────────────────────────────────────────────────────────

/**
 * Two already-normalized corners + render column order → a rectangle.
 *
 * `normalized` is the output of DB-01's `normalizeCellRange`; this function does **not**
 * re-decide which corner comes first (00-contracts §4.1 wants exactly one implementation of
 * that rule), it only maps the names onto render positions.
 */
export function toClipboardRange(
  normalized: { start: ClipboardCellCoord; end: ClipboardCellCoord },
  columnOrder: readonly string[],
): GridClipboardRange | null {
  const left = columnOrder.indexOf(normalized.start.columnName);
  const right = columnOrder.indexOf(normalized.end.columnName);
  if (left < 0 || right < 0) return null;
  const topRow = Math.min(normalized.start.rowIndex, normalized.end.rowIndex);
  const bottomRow = Math.max(normalized.start.rowIndex, normalized.end.rowIndex);
  if (!Number.isInteger(topRow) || topRow < 0) return null;
  return {
    topRow,
    bottomRow,
    leftColumn: columnOrder[Math.min(left, right)],
    rightColumn: columnOrder[Math.max(left, right)],
  };
}

/**
 * Coordinates (produced by DB-01's region expansion) → positional matrix.
 *
 * This is the **only** place a column index maps back to a name: values are read from the
 * row record by name. The coordinates arrive in row-major order; the function groups them
 * into rows and never walks a rectangle itself.
 */
export function buildSelectionMatrix(
  rows: readonly Record<string, unknown>[],
  coords: readonly ClipboardCellCoord[],
): unknown[][] {
  const out: unknown[][] = [];
  let currentRowIndex = -1;
  let current: unknown[] | null = null;
  for (const coord of coords) {
    if (coord.rowIndex !== currentRowIndex) {
      current = [];
      out.push(current);
      currentRowIndex = coord.rowIndex;
    }
    current?.push(rows[coord.rowIndex]?.[coord.columnName] ?? null);
  }
  return out;
}

/**
 * Row indices → positional matrix over `columnOrder` (whole-row copy).
 *
 * `rowIndices` must come from `rowsCoveredBySelection(state)`; a caller recomputing a row
 * set here would create the second source of truth the contracts forbid.
 */
export function buildRowsMatrix(
  rows: readonly Record<string, unknown>[],
  columnOrder: readonly string[],
  rowIndices: Iterable<number>,
): unknown[][] {
  const ordered = [...rowIndices]
    .filter((rowIndex) => Number.isInteger(rowIndex) && rowIndex >= 0)
    .sort((left, right) => left - right);
  return ordered.map((rowIndex) => {
    const row = rows[rowIndex];
    return columnOrder.map((columnName) => row?.[columnName] ?? null);
  });
}

// ── Serialization (the single kernel) ────────────────────────────────────────────────

/**
 * The formats the one serializer speaks (DB-03's three + DB-10's three).
 *
 * - `tsv` / `csv` — delimited with RFC 4180 quoting (a delimiter, a quote or
 *   a line break inside a value quotes the field and doubles inner quotes).
 * - `json` — positional arrays or named records.
 * - `plain` — tab/newline joined with **no quoting at all**: what you see is
 *   what you get, for targets that would show the quotes as characters.
 * - `html` — a `<table>` (optionally with a `<thead>`), HTML-escaped; meant
 *   for the `text/html` clipboard flavor Notion/Excel/Google Sheets render
 *   as a real table.
 * - `markdown` — a GitHub-style pipe table; `\|` escapes a pipe, `<br>`
 *   stands in for a line break (GitHub and Notion both render it).
 */
export type ClipboardFormat = 'tsv' | 'csv' | 'json' | 'plain' | 'html' | 'markdown';

export interface SerializeCellsOptions {
  format: ClipboardFormat;
  /** Column names, in the order of the selection. */
  columnNames: readonly string[];
  /**
   * tsv/csv/plain: write a header row. Default `false` for tsv and plain
   * (Excel alignment), `true` for csv. html/markdown always write one — a
   * Markdown table without a header separator is not a table, and an HTML
   * table without `<thead>` still renders, but the caller that asked for a
   * *table* asked for the names too.
   */
  includeHeader?: boolean;
  /** Text used for NULL. Default `''`; pass `'NULL'` to keep NULL distinguishable from `''`. */
  nullLiteral?: string;
  /** json: emit `{ column: value }` objects instead of positional arrays. Default `false`. */
  jsonObjects?: boolean;
  /**
   * json: indentation for `JSON.stringify`. Default `0` (compact, which is what a clipboard
   * consumer wants); the legacy `serializeDataTableRowsAsJson` passes `2` so its output stays
   * byte-identical.
   */
  jsonIndent?: number;
}

/** One cell value as clipboard text. Objects become JSON, never `[object Object]`. */
function cellToText(value: unknown, nullLiteral: string): string {
  if (value === null || value === undefined) return nullLiteral;
  if (typeof value === 'string') return value;
  if (typeof value === 'number' || typeof value === 'boolean' || typeof value === 'bigint') {
    return String(value);
  }
  if (typeof value === 'object') {
    try {
      const json = JSON.stringify(value);
      return json === undefined ? String(value) : json;
    } catch {
      return String(value);
    }
  }
  return String(value);
}

/**
 * Quote one field. A field is quoted when it contains the delimiter, a quote or a line
 * break; inner quotes are doubled. This is what fixes "a tab inside a value silently adds a
 * column" (defect G2) and matches how Excel reads the text back.
 */
function escapeDelimited(text: string, delimiter: string): string {
  if (
    text.includes(delimiter) ||
    text.includes('"') ||
    text.includes('\n') ||
    text.includes('\r')
  ) {
    return `"${text.replaceAll('"', '""')}"`;
  }
  return text;
}

/** Positional row + column names → named record (`null` for a missing cell). */
export function rowToNamedRecord(
  columnNames: readonly string[],
  row: readonly unknown[],
): Record<string, unknown> {
  const record: Record<string, unknown> = {};
  for (let index = 0; index < columnNames.length; index += 1) {
    const name = columnNames[index];
    if (name) record[name] = row[index] ?? null;
  }
  return record;
}

/**
 * Escape one HTML text node: `&`, `<` and `>` (DB-10).
 *
 * Quotes are not escaped — the value never lands in an attribute, and
 * `&quot;` in a text node is noise a Markdown/Notion paste would show
 * verbatim.
 */
export function escapeHtmlCell(text: string): string {
  return text.replaceAll('&', '&amp;').replaceAll('<', '&lt;').replaceAll('>', '&gt;');
}

/**
 * Escape one Markdown table cell (DB-10): a pipe would end the cell, a line
 * break would end the **row**. `\|` and `<br>` are what GitHub and Notion
 * both render back into the original characters.
 */
export function escapeMarkdownCellText(text: string): string {
  return text
    .replaceAll('\\', '\\\\')
    .replaceAll('|', '\\|')
    .replaceAll('\r\n', '\n')
    .replaceAll('\r', '\n')
    .replaceAll('\n', '<br>');
}

/**
 * One value as an escaped CSV field — the single-field face of the kernel
 * (DB-10), for callers that format one cell at a time instead of a matrix.
 */
export function escapeCsvField(value: unknown): string {
  return escapeDelimited(cellToText(value, ''), ',');
}

/**
 * One value as an escaped Markdown table cell — the single-field face of the
 * kernel (DB-10), same contract as `escapeCsvField`.
 */
export function escapeMarkdownField(value: unknown): string {
  return escapeMarkdownCellText(cellToText(value, ''));
}

/** The width a serialized table spans: the column list, or the widest row. */
function tableWidth(rows: readonly (readonly unknown[])[], columnNames: readonly string[]): number {
  if (columnNames.length > 0) return columnNames.length;
  return rows.reduce((widest, row) => Math.max(widest, row.length), 0);
}

/**
 * Positional matrix → clipboard text. **The one serializer of the repository.**
 *
 * The helpers in `dataTableContextMenu` and the exporters in `exportData`
 * keep their names and signatures and delegate here, so there is no second
 * escaping strategy to drift from (§5 Step 9, DB-10).
 */
export function serializeCells(
  rows: readonly (readonly unknown[])[],
  options: SerializeCellsOptions,
): string {
  const { format, columnNames } = options;
  const nullLiteral = options.nullLiteral ?? '';

  if (format === 'json') {
    const payload =
      options.jsonObjects === true
        ? rows.map((row) => rowToNamedRecord(columnNames, row))
        : rows.map((row) => [...row]);
    return JSON.stringify(payload, null, options.jsonIndent ?? 0);
  }

  const width = tableWidth(rows, columnNames);
  const cellsOf = (row: readonly unknown[]): string[] => {
    const cells: string[] = [];
    for (let index = 0; index < width; index += 1) {
      cells.push(cellToText(row[index], nullLiteral));
    }
    return cells;
  };

  if (format === 'plain') {
    // No quoting at all (DB-10): the tab and the newline are the only
    // structure, exactly as a "paste as plain text" target expects.
    const lines: string[] = [];
    if (options.includeHeader === true && columnNames.length > 0) {
      lines.push([...columnNames].join('\t'));
    }
    for (const row of rows) lines.push(cellsOf(row).join('\t'));
    return lines.join('\n');
  }

  if (format === 'html') {
    // A `<table>` for the `text/html` clipboard flavor (DB-10). The header is
    // not optional here the way it is for tsv: a caller asking for a *table*
    // asked for the column names too, and Notion/Excel render `<th>` bold.
    const lines: string[] = ['<table>'];
    if (columnNames.length > 0) {
      const head = columnNames.map((name) => `<th>${escapeHtmlCell(name)}</th>`).join('');
      lines.push(`<thead><tr>${head}</tr></thead>`);
    }
    if (rows.length > 0) {
      lines.push('<tbody>');
      for (const row of rows) {
        const tds = cellsOf(row)
          .map((cell) => `<td>${escapeHtmlCell(cell)}</td>`)
          .join('');
        lines.push(`<tr>${tds}</tr>`);
      }
      lines.push('</tbody>');
    }
    lines.push('</table>');
    return lines.join('\n');
  }

  if (format === 'markdown') {
    // A GitHub-style pipe table (DB-10). The header row and its separator
    // are the syntax — without them GitHub renders prose, not a table.
    const head = columnNames.length > 0 ? columnNames : Array.from({ length: width }, () => '');
    const headCells = head.map((name) => escapeMarkdownCellText(name));
    const separator = head.map(() => '---');
    const lines = [`| ${headCells.join(' | ')} |`, `| ${separator.join(' | ')} |`];
    for (const row of rows) {
      const cells = cellsOf(row).map((cell) => escapeMarkdownCellText(cell));
      lines.push(`| ${cells.join(' | ')} |`);
    }
    return lines.join('\n');
  }

  const delimiter = format === 'csv' ? ',' : '\t';
  const includeHeader = options.includeHeader ?? format === 'csv';
  // A caller that has no column list (whole-row copy over positional arrays) still needs the
  // widest row as the width; an empty header must not collapse every row to a blank line.
  const lines: string[] = [];
  if (includeHeader) {
    const header: string[] = [];
    for (let index = 0; index < width; index += 1) {
      header.push(escapeDelimited(columnNames[index] ?? '', delimiter));
    }
    lines.push(header.join(delimiter));
  }
  for (const row of rows) {
    const cells: string[] = [];
    for (let index = 0; index < width; index += 1) {
      cells.push(escapeDelimited(cellToText(row[index], nullLiteral), delimiter));
    }
    lines.push(cells.join(delimiter));
  }
  return lines.join('\n');
}
