/**
 * SQL Completion & Context Utilities for semantic parsing.
 */

/**
 * Detect whether the text tail following a table-clause keyword (FROM, JOIN, INTO, UPDATE)
 * is still in the process of typing a table name, vs. already completed the table name.
 */
export function isTableContext(tail: string): boolean {
  const commaIdx = tail.lastIndexOf(',');
  const segment = commaIdx !== -1 ? tail.slice(commaIdx + 1) : tail;

  const trimmedLeading = segment.trimStart();
  if (trimmedLeading === '') {
    return true;
  }

  const re =
    /(?:["`\[][^"`\]]+["`\]]|\b[A-Za-z0-9_$]+\b)(?:\s*\.\s*(?:["`\[][^"`\]]+["`\]]|\b[A-Za-z0-9_$]+\b))*/g;
  const matches: Array<{ text: string; from: number; to: number }> = [];
  let m: RegExpExecArray | null;
  while ((m = re.exec(segment)) !== null) {
    matches.push({ text: m[0], from: m.index, to: m.index + m[0].length });
  }

  if (matches.length === 0) return true;
  if (matches.length === 1) {
    const firstMatch = matches[0]!;
    const afterFirst = segment.slice(firstMatch.to);
    if (/^\s+/.test(afterFirst)) {
      return false;
    }
    return true;
  }

  return false;
}

/**
 * Extract qualifier parts from text before cursor when the user is typing
 * a dot-qualified name (e.g. `alias.` or `schema.table.`).
 *
 * Returns an array of segment names (unquoted), or an empty array if no
 * qualifier is present.
 *
 * Strictness: the dot must directly precede the identifier being typed at
 * the cursor (modulo whitespace). A dot elsewhere in the statement — e.g.
 * `SELECT a.b FROM t WHERE x` with the cursor on `x` — is NOT a qualifier
 * context and yields `[]`. This check is what keeps `relation` intents from
 * being misread as `qualified_column` during completion.
 */
export function extractQualifierParts(textBeforeCursor: string): string[] {
  const text = textBeforeCursor;
  if (!text) return [];

  let i = text.length - 1;

  // 1. Skip the identifier currently being typed at the cursor (if any)
  if (i >= 0 && (text[i] === '"' || text[i] === '`' || text[i] === ']')) {
    const closeQuote = text[i]!;
    const openQuote = closeQuote === ']' ? '[' : closeQuote;
    i -= 1;
    while (i >= 0 && text[i] !== openQuote) i -= 1;
    if (i >= 0) i -= 1; // skip the open quote
  } else {
    while (i >= 0 && /[A-Za-z0-9_$]/.test(text[i]!)) i -= 1;
  }

  // 2. Skip any whitespace between the dot and the identifier being typed
  while (i >= 0 && (text[i] === ' ' || text[i] === '\t')) i -= 1;

  // 3. The separator MUST be a '.' directly preceding the current identifier
  if (i < 0 || text[i] !== '.') {
    return [];
  }

  // 4. We confirmed this is a dot context. Now collect all qualifier segments before this dot.
  const segments: string[] = [];

  while (i >= 0 && text[i] === '.') {
    i -= 1; // skip the '.'
    // skip whitespace before dot
    while (i >= 0 && (text[i] === ' ' || text[i] === '\t')) i -= 1;
    if (i < 0) break;

    if (text[i] === '"') {
      const end = i;
      i -= 1;
      while (i >= 0 && text[i] !== '"') i -= 1;
      if (i >= 0) {
        segments.unshift(text.slice(i + 1, end).replace(/""/g, '"'));
        i -= 1;
      } else {
        break;
      }
    } else if (text[i] === '`') {
      const end = i;
      i -= 1;
      while (i >= 0 && text[i] !== '`') i -= 1;
      if (i >= 0) {
        segments.unshift(text.slice(i + 1, end));
        i -= 1;
      } else {
        break;
      }
    } else if (text[i] === ']') {
      const end = i;
      i -= 1;
      while (i >= 0 && text[i] !== '[') i -= 1;
      if (i >= 0) {
        segments.unshift(text.slice(i + 1, end).replace(/]]/g, ']'));
        i -= 1;
      } else {
        break;
      }
    } else if (/[A-Za-z0-9_$]/.test(text[i]!)) {
      const end = i + 1;
      while (i >= 0 && /[A-Za-z0-9_$]/.test(text[i]!)) i -= 1;
      segments.unshift(text.slice(i + 1, end));
    } else {
      break;
    }

    // Skip whitespace before this segment to check if there is an outer dot
    while (i >= 0 && (text[i] === ' ' || text[i] === '\t')) i -= 1;
  }

  return segments;
}
