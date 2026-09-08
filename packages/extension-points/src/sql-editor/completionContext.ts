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
 * Extract qualifier segments preceding a dot at the cursor.
 */
export function extractQualifierParts(textBeforeCursor: string): string[] {
  const dotIndex = textBeforeCursor.lastIndexOf('.');
  if (dotIndex === -1) return [];

  const segments: string[] = [];
  let i = dotIndex;

  while (i >= 0 && textBeforeCursor[i] === '.') {
    i -= 1;
    while (i >= 0 && (textBeforeCursor[i] === ' ' || textBeforeCursor[i] === '\t')) i -= 1;
    if (i < 0) break;

    const closeQuote = textBeforeCursor[i]!;
    if (closeQuote === '"' || closeQuote === '`' || closeQuote === ']') {
      const openQuote = closeQuote === ']' ? '[' : closeQuote;
      const startQuote = textBeforeCursor.lastIndexOf(openQuote, i - 1);
      if (startQuote !== -1) {
        segments.unshift(textBeforeCursor.slice(startQuote + 1, i));
        i = startQuote - 1;
      } else {
        break;
      }
    } else {
      const match = /[A-Za-z0-9_$]+$/.exec(textBeforeCursor.slice(0, i + 1));
      if (match) {
        segments.unshift(match[0]);
        i -= match[0].length;
      } else {
        break;
      }
    }

    while (i >= 0 && (textBeforeCursor[i] === ' ' || textBeforeCursor[i] === '\t')) i -= 1;
  }

  return segments;
}
