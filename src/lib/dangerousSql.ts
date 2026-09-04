import { splitSqlStatements } from './sqlTransactionGuard';

const DANGEROUS_HEAD =
  /^(WITH\b[\s\S]*?\)\s*)?(DROP|TRUNCATE)\b/i;

function stripLeadingComments(stmt: string): string {
  let s = stmt.trimStart();
  for (;;) {
    if (s.startsWith('--')) {
      const nl = s.indexOf('\n');
      if (nl < 0) return '';
      s = s.slice(nl + 1).trimStart();
      continue;
    }
    const block = /^\/\*[\s\S]*?\*\//.exec(s);
    if (block) {
      s = s.slice(block[0].length).trimStart();
      continue;
    }
    break;
  }
  return s;
}

/** True when a statement's main verb is DROP or TRUNCATE (heuristic, not a full parser). */
export function isDangerousWriteStatement(stmt: string): boolean {
  const head = stripLeadingComments(stmt).replace(/\s+/g, ' ');
  if (!head) return false;
  return DANGEROUS_HEAD.test(head);
}

/** True when any statement in the script is DROP or TRUNCATE. */
export function sqlContainsDangerousWrite(sql: string): boolean {
  return splitSqlStatements(sql).some(isDangerousWriteStatement);
}
