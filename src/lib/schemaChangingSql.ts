import { splitSqlStatements } from './sqlTransactionGuard';

const SCHEMA_CHANGING_DDL =
  /^(CREATE|DROP)\s+(OR\s+REPLACE\s+)?(TEMPORARY\s+)?(DATABASE|SCHEMA|TABLE)\b/i;

const SCHEMA_MUTATING_DDL = /^(CREATE|ALTER|DROP|TRUNCATE|RENAME|COMMENT\s+ON)\b/i;

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

/** True when a statement mutates database schema (DDL like CREATE, ALTER, DROP, TRUNCATE, RENAME). */
export function isSchemaMutatingStatement(stmt: string): boolean {
  const head = normalizeStatementHead(stmt);
  if (!head) return false;
  return SCHEMA_MUTATING_DDL.test(head);
}

/** True when any statement in the script mutates database schema. */
export function sqlMayMutateSchema(sql: string): boolean {
  return splitSqlStatements(sql).some(isSchemaMutatingStatement);
}
