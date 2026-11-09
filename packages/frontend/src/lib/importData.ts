import * as XLSX from 'xlsx';
import type { DatabaseType } from '../types';
import { escapeIdent, getEscapeSqlValue } from './databaseTypes';

/** The legacy gateway took `databaseType` as a plain string; the escapers
 * want the `DatabaseType` union, so the wizard-facing surface keeps `string?`
 * and casts at the single seam. */
function ident(name: string, databaseType?: string): string {
  return escapeIdent(name, databaseType as DatabaseType | undefined);
}

/**
 * Import parsing (DBR-01).
 *
 * The old parser split the text on `\n` first and then handled quotes per
 * line, so a quoted field containing a newline was silently torn in two and a
 * CRLF file kept a stray `\r` on the last field of every record. This module
 * replaces it with a single-pass RFC-4180 scanner (the same discipline the
 * grid paste scanner uses) that treats a quoted newline as field content,
 * accepts CR, LF and CRLF row breaks, refuses an unterminated quote instead of
 * guessing, and records the physical source line every record started on so
 * import errors can name the file line they came from.
 *
 * Compatibility: `parseImportData` / `parseXLSX` / `detectImportFormat` /
 * `generateInsertSQL` keep their old signatures and behavior for existing
 * callers; the wizard drives the option-aware surface on top of the same
 * scanner.
 */

export interface ParsedData {
  columns: string[];
  rows: Record<string, unknown>[];
}

export type ImportFormat = 'csv' | 'json' | 'xlsx';

/** Fatal parse problems — the file is refused before anything reaches the DB. */
export type ImportParseErrorKind =
  | 'unterminated-quote'
  | 'invalid-json'
  | 'json-shape'
  | 'empty-file'
  | 'unsupported-sheet';

export class ImportParseError extends Error {
  readonly kind: ImportParseErrorKind;
  /** 1-based physical line the problem was detected on, when it has one. */
  readonly line: number | null;

  constructor(kind: ImportParseErrorKind, message: string, line: number | null = null) {
    super(message);
    this.name = 'ImportParseError';
    this.kind = kind;
    this.line = line;
  }
}

/** Non-fatal problems the wizard must surface before an import may start. */
export type ImportDiagnostic =
  | { kind: 'duplicate-column'; column: string }
  | { kind: 'ragged-row'; line: number; expected: number; got: number };

export interface ImportParseResult extends ParsedData {
  /** Physical source line each record started on (1-based); 0 when the format
   * has no line concept (JSON, XLSX). Used to attribute execution errors. */
  rowOrigins: number[];
  /** Non-fatal problems: duplicate source column names, ragged data rows. */
  diagnostics: ImportDiagnostic[];
}

/** How an unquoted empty CSV field is imported. A quoted empty field is always
 * the empty string — quoting is the only way to say so, so it never becomes
 * NULL implicitly (DBR-01 NULL semantics). */
export type ImportEmptyFieldPolicy = 'null' | 'empty';

export interface CsvParseOptions {
  /** Explicit delimiter, or `'auto'` to sniff comma / tab / semicolon from the
   * first record. */
  delimiter?: string | 'auto';
  /** Quote character (default `"`). Must be a single character. */
  quoteChar?: string;
  /** Whether the first record holds column names (default `true`). */
  firstRowIsHeader?: boolean;
  /** Policy for unquoted empty fields (default `'null'`). */
  emptyUnquotedAs?: ImportEmptyFieldPolicy;
}

interface ScannedCell {
  value: string;
  quoted: boolean;
}

interface ScannedRecord {
  cells: ScannedCell[];
  /** 1-based physical line the record started on. */
  line: number;
}

/**
 * Scan delimited text record by record, one code unit at a time. Only ASCII
 * structure characters (delimiter, quote, CR, LF) are interpreted, so
 * multi-byte content simply passes through and surrogate pairs cannot tear.
 */
function scanDelimited(
  text: string,
  delimiter: string,
  quoteChar: string,
): { records: ScannedRecord[]; unterminatedQuoteAtLine: number | null } {
  const records: ScannedRecord[] = [];
  let cells: ScannedCell[] = [];
  let value = '';
  let quoted = false;
  let inQuotes = false;
  let valueStarted = false;
  let line = 1;
  let recordLine = 1;
  let lastWasRowBreak = true;

  const endCell = (): void => {
    cells.push({ value, quoted });
    value = '';
    quoted = false;
    valueStarted = false;
  };
  const endRecord = (): void => {
    endCell();
    // A record made of nothing but one unquoted cell of whitespace between
    // two row breaks is a blank line: formatting noise, not a row of spaces.
    const isBlankLine = cells.length === 1 && !cells[0].quoted && cells[0].value.trim() === '';
    if (!isBlankLine) records.push({ cells, line: recordLine });
    cells = [];
  };

  let index = 0;
  while (index < text.length) {
    const char = text[index];
    if (inQuotes) {
      if (char === quoteChar) {
        if (text[index + 1] === quoteChar) {
          value += quoteChar;
          index += 2;
          continue;
        }
        inQuotes = false;
        index += 1;
        continue;
      }
      if (char === '\r') {
        // A newline inside quotes is field content; CRLF inside quotes
        // normalizes to LF (RFC 4180 keeps the raw pair; a single canonical
        // newline round-trips better through SQL literals).
        value += '\n';
        line += 1;
        index += text[index + 1] === '\n' ? 2 : 1;
        continue;
      }
      if (char === '\n') {
        value += '\n';
        line += 1;
        index += 1;
        continue;
      }
      value += char;
      index += 1;
      continue;
    }
    if (char === quoteChar && !valueStarted) {
      inQuotes = true;
      quoted = true;
      valueStarted = true;
      lastWasRowBreak = false;
      index += 1;
      continue;
    }
    if (char === delimiter) {
      endCell();
      lastWasRowBreak = false;
      index += 1;
      continue;
    }
    if (char === '\n' || char === '\r') {
      endRecord();
      line += 1;
      recordLine = line;
      lastWasRowBreak = true;
      index += char === '\r' && text[index + 1] === '\n' ? 2 : 1;
      continue;
    }
    value += char;
    valueStarted = true;
    lastWasRowBreak = false;
    index += 1;
  }

  if (inQuotes) return { records, unterminatedQuoteAtLine: recordLine };
  // A trailing row break must not fabricate an empty record: `'a,b\n'` is one
  // record, not two.
  if (!lastWasRowBreak) endRecord();
  return { records, unterminatedQuoteAtLine: null };
}

/** Sniff the delimiter from the first record's content outside quotes. */
export function detectCsvDelimiter(text: string): string {
  let inQuotes = false;
  let counts = { ',': 0, '\t': 0, ';': 0 };
  for (let i = 0; i < Math.min(text.length, 8192); i++) {
    const char = text[i];
    if (char === '"') {
      inQuotes = !inQuotes;
      continue;
    }
    if (inQuotes) continue;
    if (char === '\n' || char === '\r') break;
    if (char === ',' || char === '\t' || char === ';') counts[char] += 1;
  }
  const best = Object.entries(counts).sort((a, b) => b[1] - a[1])[0];
  return best[1] > 0 ? best[0] : ',';
}

/** Name empty header cells deterministically instead of colliding on `''`. */
function autoColumnName(index: number): string {
  return `column_${index + 1}`;
}

function dedupeColumns(raw: string[], diagnostics: ImportDiagnostic[]): string[] {
  const seen = new Set<string>();
  return raw.map((name, index) => {
    const trimmed = name.trim();
    const candidate = trimmed === '' ? autoColumnName(index) : trimmed;
    if (seen.has(candidate.toLowerCase())) {
      diagnostics.push({ kind: 'duplicate-column', column: candidate });
      return candidate;
    }
    seen.add(candidate.toLowerCase());
    return candidate;
  });
}

/** Parse CSV (or any delimited text) with the full option surface. */
export function parseCsvImport(text: string, options: CsvParseOptions = {}): ImportParseResult {
  const {
    delimiter = 'auto',
    quoteChar = '"',
    firstRowIsHeader = true,
    emptyUnquotedAs = 'null',
  } = options;
  if (quoteChar.length !== 1) {
    throw new ImportParseError('unterminated-quote', 'quoteChar must be a single character');
  }
  const effectiveDelimiter = delimiter === 'auto' ? detectCsvDelimiter(text) : delimiter;
  if (effectiveDelimiter.length !== 1) {
    throw new ImportParseError('empty-file', 'delimiter must be a single character');
  }

  const { records, unterminatedQuoteAtLine } = scanDelimited(text, effectiveDelimiter, quoteChar);
  if (unterminatedQuoteAtLine != null) {
    throw new ImportParseError(
      'unterminated-quote',
      `Unterminated quoted field (started around line ${unterminatedQuoteAtLine})`,
      unterminatedQuoteAtLine,
    );
  }
  if (records.length === 0) {
    return { columns: [], rows: [], rowOrigins: [], diagnostics: [] };
  }

  const diagnostics: ImportDiagnostic[] = [];
  const headerRecord = firstRowIsHeader ? records[0] : null;
  const dataRecords = firstRowIsHeader ? records.slice(1) : records;
  const rawColumns =
    headerRecord?.cells.map((cell) => cell.value) ??
    Array.from({ length: Math.max(...dataRecords.map((r) => r.cells.length), 0) }, (_, i) =>
      autoColumnName(i),
    );
  const columns = dedupeColumns(rawColumns, diagnostics);

  const rows: Record<string, unknown>[] = [];
  const rowOrigins: number[] = [];
  for (const record of dataRecords) {
    if (record.cells.length !== columns.length) {
      diagnostics.push({
        kind: 'ragged-row',
        line: record.line,
        expected: columns.length,
        got: record.cells.length,
      });
    }
    const row: Record<string, unknown> = {};
    columns.forEach((column, columnIndex) => {
      const cell = record.cells[columnIndex];
      if (cell == null) {
        // Fewer cells than headers: the missing tail is the empty-field policy
        // (an absent value and an unquoted empty value mean the same thing).
        row[column] = emptyUnquotedAs === 'null' ? null : '';
        return;
      }
      if (cell.value === '' && !cell.quoted) {
        row[column] = emptyUnquotedAs === 'null' ? null : '';
        return;
      }
      row[column] = cell.value;
    });
    rows.push(row);
    rowOrigins.push(record.line);
  }

  return { columns, rows, rowOrigins, diagnostics };
}

export interface JsonParseOptions {
  /** Unused placeholder for future object/array mode options; kept so the
   * wizard's option object can be passed straight through. */
  mode?: 'auto';
}

/** Parse a JSON document: array of objects or a single object. */
export function parseJsonImport(text: string, _options: JsonParseOptions = {}): ImportParseResult {
  let parsed: unknown;
  try {
    parsed = JSON.parse(text);
  } catch (error) {
    throw new ImportParseError(
      'invalid-json',
      error instanceof Error ? error.message : String(error),
    );
  }
  if (parsed == null || typeof parsed !== 'object' || Array.isArray(parsed) === false) {
    if (parsed == null || typeof parsed !== 'object') {
      throw new ImportParseError('json-shape', 'JSON payload must be an object or an array');
    }
    parsed = [parsed];
  }
  const arr = parsed as Record<string, unknown>[];
  const nonNullRows = arr.filter(
    (row) => row != null && typeof row === 'object' && !Array.isArray(row),
  );
  if (nonNullRows.length !== arr.length) {
    throw new ImportParseError('json-shape', 'JSON array items must all be objects');
  }
  if (nonNullRows.length === 0) {
    return { columns: [], rows: [], rowOrigins: [], diagnostics: [] };
  }

  const diagnostics: ImportDiagnostic[] = [];
  const columnSet: string[] = [];
  const seen = new Set<string>();
  for (const row of nonNullRows) {
    for (const key of Object.keys(row)) {
      if (!seen.has(key)) {
        seen.add(key);
        columnSet.push(key);
      }
    }
  }
  const columns = dedupeColumns(columnSet, diagnostics);
  const rows = nonNullRows.map((item) => {
    const record: Record<string, unknown> = {};
    for (const column of columns) {
      const value = item[column];
      record[column] = value === undefined ? null : value;
    }
    return record;
  });

  return { columns, rows, rowOrigins: rows.map(() => 0), diagnostics };
}

export interface XlsxParseOptions {
  /** Which sheet to read; omit for the first. `sheetNames` on the result lists
   * every sheet so the wizard can offer the choice (DBR-01). */
  sheetName?: string;
}

export interface XlsxParseResult extends ImportParseResult {
  /** Every worksheet name in the workbook, in order. */
  sheetNames: string[];
  /** The sheet actually read (matches `options.sheetName` when given). */
  chosenSheet: string | null;
}

/** Backward-compatible first-sheet reader used by export round-trip tests. */
export function parseXLSX(base64: string): ParsedData {
  return parseXlsxImport(base64);
}

/** Parse an XLSX workbook with worksheet selection. */
export function parseXlsxImport(base64: string, options: XlsxParseOptions = {}): XlsxParseResult {
  const workbook = XLSX.read(base64, { type: 'base64' });
  const sheetNames = workbook.SheetNames;
  if (sheetNames.length === 0) {
    throw new ImportParseError('unsupported-sheet', 'The workbook has no worksheets');
  }
  const chosen =
    options.sheetName && sheetNames.includes(options.sheetName) ? options.sheetName : sheetNames[0];
  if (options.sheetName && options.sheetName !== chosen) {
    throw new ImportParseError('unsupported-sheet', `No worksheet named "${options.sheetName}"`);
  }

  const sheet = workbook.Sheets[chosen];
  const rawRows = XLSX.utils.sheet_to_json<Record<string, unknown>>(sheet, { defval: null });
  if (rawRows.length === 0) {
    return {
      columns: [],
      rows: [],
      rowOrigins: [],
      diagnostics: [],
      sheetNames,
      chosenSheet: chosen,
    };
  }

  const diagnostics: ImportDiagnostic[] = [];
  const columnSet: string[] = [];
  const seen = new Set<string>();
  for (const row of rawRows) {
    for (const key of Object.keys(row)) {
      if (!seen.has(key)) {
        seen.add(key);
        columnSet.push(key);
      }
    }
  }
  const columns = dedupeColumns(columnSet, diagnostics);
  const rows = rawRows.map((item) => {
    const record: Record<string, unknown> = {};
    for (const column of columns) {
      const value = item[column];
      record[column] = value === undefined ? null : value;
    }
    return record;
  });

  return {
    columns,
    rows,
    rowOrigins: rows.map(() => 0),
    diagnostics,
    sheetNames,
    chosenSheet: chosen,
  };
}

/** Parse CSV/JSON text (format-aware wrapper over the option-aware parsers). */
export function parseImportData(
  content: string,
  format: 'csv' | 'json',
  options: CsvParseOptions & JsonParseOptions = {},
): ParsedData {
  return format === 'csv' ? parseCsvImport(content, options) : parseJsonImport(content, options);
}

export function detectImportFormat(fileName: string): ImportFormat | null {
  const lower = fileName.toLowerCase();
  if (lower.endsWith('.json')) return 'json';
  if (lower.endsWith('.csv')) return 'csv';
  if (lower.endsWith('.xlsx')) return 'xlsx';
  return null;
}

/** Encodings the wizard offers. `TextDecoder` decides availability — an
 * unsupported label throws `RangeError`, which the wizard surfaces. */
export const IMPORT_ENCODINGS = ['utf-8', 'gbk', 'gb18030', 'big5', 'shift_jis', 'latin1'] as const;
export type ImportEncoding = (typeof IMPORT_ENCODINGS)[number];

/** Decode file bytes with the chosen encoding. Invalid sequences become
 * replacement characters (the parse itself stays total); an unknown label
 * throws so the wizard can say the encoding is unavailable. */
export function decodeImportText(bytes: Uint8Array, encoding: string = 'utf-8'): string {
  return new TextDecoder(encoding).decode(bytes);
}

/** Decode a base64 payload (the host file picker's wire shape). */
export function decodeImportBase64(dataBase64: string, encoding: string = 'utf-8'): string {
  const binary = atob(dataBase64);
  const bytes = new Uint8Array(binary.length);
  for (let i = 0; i < binary.length; i++) bytes[i] = binary.charCodeAt(i);
  return decodeImportText(bytes, encoding);
}

/**
 * Backward-compatible single-statement-per-row INSERT generator. The import
 * wizard does **not** use this — it batches through `importExecution` (DBR-01
 * forbids one giant SQL string); this stays for direct SQL preview callers.
 * Values use the dialect-aware escaper, identifiers the dialect-aware quoter.
 */
export function generateInsertSQL(
  tableName: string,
  data: ParsedData,
  databaseType?: string,
): string {
  if (data.rows.length === 0) return '';
  const escape = getEscapeSqlValue(databaseType);
  const colList = data.columns.map((c) => ident(c, databaseType)).join(', ');
  return data.rows
    .map((row) => {
      const values = data.columns.map((col) => escape(row[col])).join(', ');
      return `INSERT INTO ${ident(tableName, databaseType)} (${colList}) VALUES (${values});`;
    })
    .join('\n');
}
