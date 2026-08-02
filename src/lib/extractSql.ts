/**
 * Extract pure SQL from an LLM response that may contain reasoning,
 * markdown fences, or explanatory text.
 *
 * Priority:
 *  1. SQL inside a ```sql ... ``` fence
 *  2. SQL inside a bare ``` ... ``` fence (if it looks like SQL)
 *  3. Lines that look like SQL statements (SELECT/INSERT/UPDATE/DELETE/WITH/CREATE/ALTER/DROP/EXPLAIN/SHOW/DESCRIBE/PRAGMA/TRUNCATE/GRANT/REVOKE/BEGIN/COMMIT/ROLLBACK/SET/USE/VACUUM)
 *  4. Fallback: return trimmed input as-is
 */
export function extractSqlFromResponse(raw: string): string {
  const trimmed = raw.trim();
  if (!trimmed) return '';

  // 1. Try ```sql ... ``` fence (case-insensitive language tag)
  const sqlFenceRe = /```(?:sql|SQL|mysql|postgresql|postgres|sqlite)\s*\n([\s\S]*?)```/;
  const sqlMatch = sqlFenceRe.exec(trimmed);
  if (sqlMatch) {
    return sqlMatch[1].trim();
  }

  // 2. Try bare ``` ... ``` fence
  const bareFenceRe = /```\s*\n([\s\S]*?)```/;
  const bareMatch = bareFenceRe.exec(trimmed);
  if (bareMatch) {
    const inner = bareMatch[1].trim();
    if (looksLikeSql(inner)) {
      return inner;
    }
  }

  // 3. If the entire response looks like pure SQL, return as-is
  if (looksLikeSql(trimmed)) {
    return trimmed;
  }

  // 4. Extract SQL-looking lines from mixed content
  const lines = trimmed.split('\n');
  const sqlLines: string[] = [];
  let inSqlBlock = false;

  for (const line of lines) {
    const stripped = line.trim();
    if (!stripped) {
      if (inSqlBlock) sqlLines.push('');
      continue;
    }
    if (isSqlLine(stripped)) {
      sqlLines.push(line);
      inSqlBlock = true;
    } else if (inSqlBlock && isSqlContinuation(stripped)) {
      sqlLines.push(line);
    } else {
      inSqlBlock = false;
    }
  }

  const extracted = sqlLines.join('\n').trim();
  if (extracted) return extracted;

  return trimmed;
}

const SQL_KEYWORDS_RE =
  /^\s*(SELECT|INSERT|UPDATE|DELETE|WITH|CREATE|ALTER|DROP|EXPLAIN|SHOW|DESCRIBE|PRAGMA|TRUNCATE|GRANT|REVOKE|BEGIN|COMMIT|ROLLBACK|SET|USE|VACUUM)\b/i;

function looksLikeSql(text: string): boolean {
  const firstLine = text.split('\n')[0].trim();
  return SQL_KEYWORDS_RE.test(firstLine);
}

function isSqlLine(line: string): boolean {
  return SQL_KEYWORDS_RE.test(line);
}

function isSqlContinuation(line: string): boolean {
  const upper = line.toUpperCase().trimStart();
  const continuationKeywords = [
    'FROM', 'WHERE', 'JOIN', 'LEFT', 'RIGHT', 'INNER', 'OUTER', 'CROSS',
    'ON', 'AND', 'OR', 'NOT', 'IN', 'EXISTS', 'BETWEEN', 'LIKE', 'ILIKE',
    'ORDER', 'GROUP', 'HAVING', 'LIMIT', 'OFFSET', 'UNION', 'INTERSECT',
    'EXCEPT', 'AS', 'CASE', 'WHEN', 'THEN', 'ELSE', 'END', 'INTO',
    'VALUES', 'SET', 'RETURNING', 'FETCH', 'FOR', 'WINDOW', 'PARTITION',
    'OVER', 'ROWS', 'RANGE', 'LATERAL', 'NATURAL', 'USING', 'FILTER',
    'WITHIN', 'RECURSIVE', 'MATERIALIZED',
  ];
  if (continuationKeywords.some((kw) => upper.startsWith(kw + ' ') || upper.startsWith(kw + '\t') || upper === kw)) {
    return true;
  }
  // Lines starting with operators, parentheses, or ending with comma/semicolon
  if (/^[),;]/.test(line.trim()) || /^[(\-+*/%|&]/.test(line.trim())) {
    return true;
  }
  // Indented content (likely part of a SQL statement)
  if (/^\s{2,}\S/.test(line) && !/^[a-z].*[:：]/i.test(line.trim())) {
    return true;
  }
  return false;
}
