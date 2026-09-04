/**
 * Helper to identify individual statement boundaries in a SQL script,
 * ignoring semicolons in strings and comments, and locate the statement
 * surrounding the given cursor offset.
 */
export function getStatementAtCursor(sql: string, cursorOffset: number): string {
  if (!sql.trim()) return '';

  const ranges: { start: number; end: number; text: string }[] = [];
  let stmtStart = 0;
  let i = 0;
  let inSingleQuote = false;
  let inDoubleQuote = false;
  let inLineComment = false;
  let inBlockComment = false;

  while (i < sql.length) {
    const c = sql[i]!;
    const next = sql[i + 1];

    if (inLineComment) {
      if (c === '\n') inLineComment = false;
      i++;
      continue;
    }
    if (inBlockComment) {
      if (c === '*' && next === '/') {
        inBlockComment = false;
        i += 2;
        continue;
      }
      i++;
      continue;
    }
    if (inSingleQuote) {
      if (c === "'") {
        if (next === "'") {
          i += 2;
          continue;
        }
        inSingleQuote = false;
      }
      i++;
      continue;
    }
    if (inDoubleQuote) {
      if (c === '"') {
        if (next === '"') {
          i += 2;
          continue;
        }
        inDoubleQuote = false;
      }
      i++;
      continue;
    }

    if (c === '-' && next === '-') {
      inLineComment = true;
      i += 2;
      continue;
    }
    if (c === '#') {
      inLineComment = true;
      i++;
      continue;
    }
    if (c === '/' && next === '*') {
      inBlockComment = true;
      i += 2;
      continue;
    }
    if (c === "'") {
      inSingleQuote = true;
      i++;
      continue;
    }
    if (c === '"') {
      inDoubleQuote = true;
      i++;
      continue;
    }

    if (c === ';') {
      ranges.push({ start: stmtStart, end: i + 1, text: sql.slice(stmtStart, i + 1) });
      stmtStart = i + 1;
    }
    i++;
  }

  if (stmtStart < sql.length) {
    ranges.push({ start: stmtStart, end: sql.length, text: sql.slice(stmtStart) });
  }

  const validRanges = ranges.filter((r) => r.text.trim().length > 0);
  if (validRanges.length === 0) return sql.trim();

  for (const r of validRanges) {
    if (cursorOffset >= r.start && cursorOffset <= r.end) {
      return r.text.trim();
    }
  }

  if (cursorOffset < validRanges[0]!.start) return validRanges[0]!.text.trim();
  return validRanges[validRanges.length - 1]!.text.trim();
}
