/**
 * Minimal SQL tokenizer for the read-only query preview.
 *
 * Deliberately small and total: it walks the string once, emits every character
 * (gaps become `plain`), and never throws — a preview must render whatever the
 * generator produced, including a half-typed statement.
 */

export type SqlTokenKind =
  | 'keyword'
  | 'string'
  | 'number'
  | 'identifier'
  | 'comment'
  | 'punct'
  | 'plain';

export interface SqlToken {
  kind: SqlTokenKind;
  text: string;
}

/** Keywords highlighted in the preview (case-insensitive). */
export const SQL_KEYWORDS: ReadonlySet<string> = new Set([
  'ALL',
  'AND',
  'AS',
  'ASC',
  'BETWEEN',
  'BY',
  'CASE',
  'CAST',
  'CROSS',
  'DESC',
  'DISTINCT',
  'ELSE',
  'END',
  'EXISTS',
  'FALSE',
  'FETCH',
  'FOR',
  'FROM',
  'FULL',
  'GROUP',
  'HAVING',
  'ILIKE',
  'IN',
  'INNER',
  'INTERSECT',
  'IS',
  'JOIN',
  'LEFT',
  'LIKE',
  'LIMIT',
  'NATURAL',
  'NOT',
  'NULL',
  'NULLS',
  'OFFSET',
  'ON',
  'OR',
  'ORDER',
  'OUTER',
  'RIGHT',
  'SELECT',
  'THEN',
  'TOP',
  'TRUE',
  'UNION',
  'WHEN',
  'WHERE',
  'WITH',
]);

/**
 * One pass over the statement. Order matters: comments and quoted literals win
 * over keywords, so `'select'` stays a string.
 */
const TOKEN_PATTERN =
  /(--[^\n]*|\/\*[\s\S]*?\*\/)|('(?:[^']|'')*')|("(?:[^"]|"")*"|`(?:[^`]|``)*`|\[[^\]]*\])|(\d+(?:\.\d+)?)|([A-Za-z_][A-Za-z0-9_$]*)|([(),.;*=<>!+\-/|]+)|(\s+)/g;

export function tokenizeSql(sql: string): SqlToken[] {
  const tokens: SqlToken[] = [];
  let lastIndex = 0;

  const pushPlain = (text: string) => {
    if (text) tokens.push({ kind: 'plain', text });
  };

  TOKEN_PATTERN.lastIndex = 0;
  let match: RegExpExecArray | null;
  while ((match = TOKEN_PATTERN.exec(sql)) !== null) {
    // Anything the pattern skipped (an unterminated quote, say) is plain text —
    // the preview must still show every character the generator emitted.
    pushPlain(sql.slice(lastIndex, match.index));
    const [text, comment, str, quotedIdentifier, number, word, punct, space] = match;
    if (comment) tokens.push({ kind: 'comment', text: comment });
    else if (str) tokens.push({ kind: 'string', text: str });
    else if (quotedIdentifier) tokens.push({ kind: 'identifier', text: quotedIdentifier });
    else if (number) tokens.push({ kind: 'number', text: number });
    else if (word) {
      tokens.push({
        kind: SQL_KEYWORDS.has(word.toUpperCase()) ? 'keyword' : 'identifier',
        text: word,
      });
    } else if (punct) tokens.push({ kind: 'punct', text: punct });
    else if (space) pushPlain(space);
    else pushPlain(text!);
    lastIndex = match.index + text!.length;
  }
  pushPlain(sql.slice(lastIndex));
  return tokens;
}

/** Tailwind classes per token kind (they map onto the semantic tokens). */
export const SQL_TOKEN_CLASS: Record<SqlTokenKind, string> = {
  keyword: 'text-accent font-medium',
  string: 'text-success',
  number: 'text-warning',
  identifier: 'text-fg',
  comment: 'text-fg-muted italic',
  punct: 'text-fg-muted',
  plain: 'text-fg',
};
