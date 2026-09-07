/**
 * Pure value parser for Paste-as-IN feature.
 *
 * Parses clipboard text containing delimited values (comma, tab, newline, CRLF)
 * into an array of trimmed string values. Supports quoted delimiters and
 * configurable empty-item strategy.
 *
 * §Track S5-A step 1 + §6.6
 */

/** Maximum source text size in bytes (1 MiB). */
export const MAX_SOURCE_BYTES = 1 * 1024 * 1024;

/** Maximum number of parsed values. */
export const MAX_VALUE_COUNT = 10_000;

export type EmptyItemStrategy = 'skip' | 'keep-empty';

export interface ParseOptions {
  /** Delimiter pattern. Defaults to auto-detect from content. */
  delimiter?: string;
  /** Strategy for empty items between consecutive delimiters. Default: 'skip'. */
  emptyItems?: EmptyItemStrategy;
  /** Trim whitespace from each value. Default: true. */
  trim?: boolean;
}

export interface ParseSuccess {
  ok: true;
  values: string[];
  delimiter: string;
}

export type ParseError =
  | { kind: 'too-large'; maxBytes: number }
  | { kind: 'too-many-values'; maxCount: number }
  | { kind: 'no-values' };

export type ParseResult = ParseSuccess | ParseError;

/** Detect the most likely delimiter from content. */
function detectDelimiter(content: string): string {
  // Count occurrences of candidate delimiters (outside quotes)
  const counts: Record<string, number> = { '\t': 0, ',': 0, '\n': 0, ';': 0 };
  let inSingleQuote = false;
  let inDoubleQuote = false;

  for (let i = 0; i < content.length; i++) {
    const ch = content[i]!;
    if (ch === "'" && !inDoubleQuote) {
      // Check for escaped quote ''
      if (inSingleQuote && content[i + 1] === "'") {
        i++;
        continue;
      }
      inSingleQuote = !inSingleQuote;
    } else if (ch === '"' && !inSingleQuote) {
      inDoubleQuote = !inDoubleQuote;
    } else if (!inSingleQuote && !inDoubleQuote) {
      if (ch in counts) {
        counts[ch]!++;
      }
    }
  }

  // CRLF → treat as newline delimiter (CRLF is normalized)
  const crlfCount = (content.match(/\r\n/g) ?? []).length;
  counts['\n'] = (counts['\n'] ?? 0) - crlfCount;

  // Prefer tab > comma > semicolon > newline
  if (counts['\t']! > 0) return '\t';
  if (counts[',']! > 0) return ',';
  if (counts[';']! > 0) return ';';
  if (counts['\n']! > 0) return '\n';
  // Fallback: if only one value, return comma as default
  return ',';
}

/** Escape a value for SQL single-quote string literal: ' → ''. */
export function escapeSqlString(value: string): string {
  return value.replace(/'/g, "''");
}

/**
 * Classify a trimmed string value into a SQL literal.
 *
 * - "auto-type" mode: numbers stay numeric, NULL stays SQL NULL, others single-quoted.
 * - "all-strings" mode: everything is single-quoted.
 */
export type ValueMode = 'auto-type' | 'all-strings';

export function toSqlLiteral(value: string, mode: ValueMode): string {
  const trimmed = value.trim();
  if (mode === 'all-strings') {
    return `'${escapeSqlString(trimmed)}'`;
  }
  // auto-type
  if (trimmed.toUpperCase() === 'NULL') return 'NULL';
  // Integer (no leading zeros except for '0' itself)
  if (/^-?(0|[1-9]\d*)$/.test(trimmed)) return trimmed;
  // Float (no leading zeros except for '0.xxx')
  if (/^-?(0|[1-9]\d*)\.\d+$/.test(trimmed)) return trimmed;
  // Everything else is a quoted string
  return `'${escapeSqlString(trimmed)}'`;
}

/**
 * Parse delimited values from clipboard text.
 *
 * @param source - Raw text from clipboard
 * @param options - Parsing options
 * @returns Parsed values or an error
 */
export function parseDelimitedValues(source: string, options?: ParseOptions): ParseResult {
  const byteSize = new TextEncoder().encode(source).byteLength;
  if (byteSize > MAX_SOURCE_BYTES) {
    return { kind: 'too-large', maxBytes: MAX_SOURCE_BYTES };
  }

  const trim = options?.trim ?? true;
  const emptyItems = options?.emptyItems ?? 'skip';

  // Normalize CRLF to LF
  const normalized = source.replace(/\r\n/g, '\n').replace(/\r/g, '\n');

  const delimiter = options?.delimiter ?? detectDelimiter(normalized);

  // Split by delimiter
  const raw = normalized.split(delimiter);

  const values: string[] = [];
  for (const item of raw) {
    const processed = trim ? item.trim() : item;
    if (processed === '' && emptyItems === 'skip') continue;
    values.push(processed);
    if (values.length > MAX_VALUE_COUNT) {
      return { kind: 'too-many-values', maxCount: MAX_VALUE_COUNT };
    }
  }

  if (values.length === 0) {
    return { kind: 'no-values' };
  }

  return { ok: true, values, delimiter };
}

/**
 * Format a list of values into SQL IN (...) clause content.
 *
 * @param values - Parsed string values
 * @param mode - Value mode (auto-type or all-strings)
 * @returns SQL fragment like: `'foo', 42, NULL`
 */
export function formatInClause(values: string[], mode: ValueMode): string {
  return values.map((v) => toSqlLiteral(v, mode)).join(', ');
}
