/**
 * Helper to identify individual statement boundaries in a SQL script,
 * ignoring semicolons in strings and comments, and locate the statement
 * surrounding the given cursor offset.
 *
 * Compatibility wrapper over the unified semantic statement range module.
 */
export { getStatementTextAtCursor as getStatementAtCursor } from '../components/sql-editor/semantic/statementRanges';
