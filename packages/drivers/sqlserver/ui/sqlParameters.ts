import type { SqlParameterStrategy } from '@datazen/driver-sdk';

/** T-SQL local variables and routine parameters are SQL, not editor bind placeholders. */
export const sqlserverSqlParameterStrategy: SqlParameterStrategy = {
  filterOccurrences(sql, occurrences) {
    const declaredAt = findNonRoutineDeclaredAtVars(sql);
    const routineAtVars = findTsqlRoutineSignatureAtVars(sql);
    const routineRanges = findTsqlRoutineRanges(sql);
    return occurrences.filter((occurrence) => {
      if (occurrence.syntax !== 'at') return true;
      const normalizedName = occurrence.name.toLowerCase();
      const isRoutineLocal =
        routineAtVars.has(normalizedName) &&
        routineRanges.some(({ from, to }) => occurrence.from >= from && occurrence.from < to);
      return !declaredAt.has(normalizedName) && !isRoutineLocal;
    });
  },
};

function skipNonCode(sql: string, i: number): number {
  const ch = sql[i];
  if (ch === "'" || ch === '"' || ch === '`') return skipQuote(sql, i);
  if (ch === '-' && sql[i + 1] === '-') {
    const nl = sql.indexOf('\n', i);
    return nl === -1 ? sql.length : nl + 1;
  }
  if (ch === '/' && sql[i + 1] === '*') {
    const end = sql.indexOf('*/', i + 2);
    return end === -1 ? sql.length : end + 2;
  }
  if (ch === '$') return skipDollarQuote(sql, i);
  return i;
}

function skipQuote(sql: string, i: number): number {
  const quote = sql[i];
  let j = i + 1;
  while (j < sql.length) {
    if (sql[j] === quote) {
      if (quote === "'" && sql[j + 1] === "'") {
        j += 2;
        continue;
      }
      return j + 1;
    }
    j += 1;
  }
  return sql.length;
}

function skipDollarQuote(sql: string, i: number): number {
  if (sql[i] !== '$') return i;
  let j = i + 1;
  while (j < sql.length && (isIdentChar(sql[j]) || sql[j] === '_')) j += 1;
  if (j >= sql.length || sql[j] !== '$') return i;
  const tag = sql.slice(i, j + 1);
  const close = sql.indexOf(tag, j + 1);
  return close === -1 ? sql.length : close + tag.length;
}

function isIdentChar(ch: string): boolean {
  const code = ch.charCodeAt(0);
  return (code >= 65 && code <= 90) || (code >= 97 && code <= 122) || (code >= 48 && code <= 57);
}

/** Collect `@name` variables introduced by DECLARE / SET and T-SQL routine signatures. */
export function findDeclaredAtVars(sql: string): Set<string> {
  const declared = new Set<string>();
  for (const stmt of splitStatements(sql)) {
    collectTsqlRoutineSignatureAtVars(stmt, declared);
    collectDeclaredAtVarsInStatement(stmt, declared);
  }
  return declared;
}

function findNonRoutineDeclaredAtVars(sql: string): Set<string> {
  const declared = new Set<string>();
  for (const stmt of splitStatements(sql)) {
    collectDeclaredAtVarsInStatement(stmt, declared);
  }
  return declared;
}

function findTsqlRoutineSignatureAtVars(sql: string): Set<string> {
  const declared = new Set<string>();
  for (const stmt of splitStatements(sql)) {
    collectTsqlRoutineSignatureAtVars(stmt, declared);
  }
  return declared;
}

function findTsqlRoutineRanges(sql: string): Array<{ from: number; to: number }> {
  const ranges: Array<{ from: number; to: number }> = [];
  for (const { statement, from } of splitStatementsWithOffsets(sql)) {
    const prefix = maskNonCodeForTsql(statement);
    const definition =
      /^\s*(?:CREATE|ALTER)\s+(?:OR\s+ALTER\s+)?(?:FUNCTION|PROC(?:EDURE)?)\b/i.exec(prefix);
    if (!definition) continue;

    const remaining = maskNonCodeForTsql(sql.slice(from));
    const header = remaining.slice(definition[0].length);
    const bodyMarker = /\bAS\b/i.exec(header);
    if (!bodyMarker) continue;

    const bodyStart = definition[0].length + bodyMarker.index + bodyMarker[0].length;
    const body = remaining.slice(bodyStart);
    const begin = /\bBEGIN\b/i.exec(body);
    if (!begin) {
      const semicolon = remaining.indexOf(';', bodyStart);
      ranges.push({ from, to: semicolon === -1 ? sql.length : from + semicolon });
      continue;
    }

    const blockStart = bodyStart + begin.index;
    const keywords = /\b(BEGIN|CASE|END)\b/gi;
    keywords.lastIndex = blockStart;
    let depth = 0;
    let end = sql.length;
    let match: RegExpExecArray | null;
    while ((match = keywords.exec(remaining))) {
      const keyword = match[1].toUpperCase();
      if (keyword === 'BEGIN') {
        const next = /^\s+(?:TRAN(?:SACTION)?|DIALOG|CONVERSATION|DISTRIBUTED)\b/i.exec(
          remaining.slice(keywords.lastIndex),
        );
        if (!next) depth += 1;
      } else if (keyword === 'CASE') {
        depth += 1;
      } else {
        depth -= 1;
        if (depth === 0) {
          end = from + keywords.lastIndex;
          const terminator = /^\s*;/.exec(sql.slice(end));
          if (terminator) end += terminator[0].length;
          break;
        }
      }
    }
    ranges.push({ from, to: end });
  }
  return ranges;
}

function splitStatementsWithOffsets(sql: string): Array<{ statement: string; from: number }> {
  const statements: Array<{ statement: string; from: number }> = [];
  let start = 0;
  let i = 0;
  while (i < sql.length) {
    const skipped = skipNonCode(sql, i);
    if (skipped !== i) {
      i = skipped;
      continue;
    }
    if (sql[i] === ';') {
      statements.push({ statement: sql.slice(start, i), from: start });
      start = i + 1;
    }
    i += 1;
  }
  statements.push({ statement: sql.slice(start), from: start });
  return statements;
}

/** A routine parameter belongs to the T-SQL body, not to the editor's bind payload. */
function collectTsqlRoutineSignatureAtVars(stmt: string, declared: Set<string>) {
  const code = maskNonCodeForTsql(stmt);
  const definition = /^\s*(?:CREATE|ALTER)\s+(?:OR\s+ALTER\s+)?(?:FUNCTION|PROC(?:EDURE)?)\b/i.exec(
    code,
  );
  if (!definition) return;

  const header = code.slice(definition[0].length);
  const bodyMarker = /\bAS\b/i.exec(header);
  if (!bodyMarker) return;

  const signature = header.slice(0, bodyMarker.index);
  for (const match of signature.matchAll(/@([A-Za-z_][A-Za-z0-9_]*)/g)) {
    declared.add(match[1].toLowerCase());
  }
}

/** Keep offsets stable while hiding comments, literals, and bracketed identifiers. */
function maskNonCodeForTsql(sql: string): string {
  const masked = sql.split('');
  for (let i = 0; i < sql.length; ) {
    const end = sql[i] === '[' ? skipBracketIdentifier(sql, i) : skipNonCode(sql, i);
    if (end === i) {
      i += 1;
      continue;
    }
    for (let j = i; j < end; j += 1) {
      if (sql[j] !== '\n' && sql[j] !== '\r') masked[j] = ' ';
    }
    i = end;
  }
  return masked.join('');
}

function skipBracketIdentifier(sql: string, i: number): number {
  let j = i + 1;
  while (j < sql.length) {
    if (sql[j] === ']') {
      if (sql[j + 1] === ']') {
        j += 2;
        continue;
      }
      return j + 1;
    }
    j += 1;
  }
  return sql.length;
}

function collectDeclaredAtVarsInStatement(stmt: string, declared: Set<string>) {
  const trimmed = stmt.trimStart();
  if (!trimmed) return;

  const upper = trimmed.toUpperCase();
  if (upper.startsWith('DECLARE')) {
    const body = trimmed.slice('DECLARE'.length);
    for (const m of body.matchAll(/@([A-Za-z_][A-Za-z0-9_]*)/g)) {
      declared.add(m[1].toLowerCase());
    }
    return;
  }

  if (upper.startsWith('SET')) {
    const m = /^SET\s+@([A-Za-z_][A-Za-z0-9_]*)\s*=/i.exec(trimmed);
    if (m) {
      declared.add(m[1].toLowerCase());
    }
  }
}

function splitStatements(sql: string): string[] {
  return splitStatementsWithOffsets(sql).map(({ statement }) => statement);
}
