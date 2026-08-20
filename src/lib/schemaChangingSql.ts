import { splitSqlStatements } from './sqlTransactionGuard';

const SCHEMA_CHANGING_DDL =
  /^(CREATE|DROP)\s+(OR\s+REPLACE\s+)?(TEMPORARY\s+)?(DATABASE|SCHEMA|TABLE)\b/i;

function normalizeStatementHead(stmt: string): string {
  return stmt.trim().replace(/\s+/g, ' ');
}

/** True when a statement creates/drops a database, schema, or table. */
export function isSchemaChangingStatement(stmt: string): boolean {
  const head = normalizeStatementHead(stmt);
  if (!head) return false;
  return SCHEMA_CHANGING_DDL.test(head);
}

/** True when any statement in the script is schema-changing DDL. */
export function sqlContainsSchemaChangingDdl(sql: string): boolean {
  return splitSqlStatements(sql).some(isSchemaChangingStatement);
}
