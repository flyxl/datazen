/**
 * Selection-aware SQL beautifier command (§4.2).
 *
 * Lives next to the editor rather than in `src/lib/` because it depends on
 * `EditorView`; `src/lib/sqlFormat.ts` stays free of CodeMirror so non-editor
 * callers (export, workflow forms) can keep importing it.
 */
import type { EditorView } from '@codemirror/view';
import { formatSql, DEFAULT_SQL_FORMAT_OPTIONS } from '../../../lib/sqlFormat';
import { buildStatementRanges, findStatementAtCursor } from '../semantic/statementRanges';
import type { SqlFormatOptions } from '../../../types';

export interface FormatEditorDocumentOptions {
  options?: Partial<SqlFormatOptions>;
  databaseType?: string;
}

/**
 * Formats the selection when one exists, otherwise the whole document.
 *
 * Returns false when there is nothing to format or the formatter rejects the
 * input (incomplete SQL is common mid-edit and must not clobber the buffer).
 */
export function formatEditorDocument(
  view: EditorView,
  { options, databaseType }: FormatEditorDocumentOptions = {},
): boolean {
  const resolved = { ...DEFAULT_SQL_FORMAT_OPTIONS, ...options };
  const selection = view.state.selection.main;

  return selection.empty
    ? formatWholeDocument(view, resolved, databaseType)
    : formatSelection(view, resolved, databaseType, selection.from, selection.to);
}

function formatSelection(
  view: EditorView,
  options: SqlFormatOptions,
  databaseType: string | undefined,
  from: number,
  to: number,
): boolean {
  const original = view.state.sliceDoc(from, to);
  if (!original.trim()) return false;

  const formatted = safeFormat(original, databaseType, options);
  if (formatted == null || formatted === original) return false;

  // Re-apply the selection's own leading indentation so a formatted block keeps
  // sitting at the depth the user selected it at.
  const indent = leadingIndentOf(view, from);
  const reindented = indent ? reindent(formatted, indent) : formatted;

  view.dispatch({
    changes: { from, to, insert: reindented },
    // Keep the block highlighted — the user's next action is usually to re-run it.
    selection: { anchor: from, head: from + reindented.length },
    scrollIntoView: true,
  });
  return true;
}

function formatWholeDocument(
  view: EditorView,
  options: SqlFormatOptions,
  databaseType: string | undefined,
): boolean {
  const original = view.state.doc.toString();
  if (!original.trim()) return false;

  const formatted = safeFormat(original, databaseType, options);
  if (formatted == null || formatted === original) return false;

  const cursor = view.state.selection.main.head;
  const anchor = mapCursorByStatement(original, formatted, cursor);

  view.dispatch({
    changes: { from: 0, to: original.length, insert: formatted },
    selection: { anchor },
    scrollIntoView: true,
  });
  return true;
}

/**
 * Reformatting rewrites every offset, so a raw offset would drop the cursor in
 * an unrelated statement and make the viewport jump. Anchor on the statement
 * the cursor was in instead, and fall back to a clamped offset when the
 * statement count changes.
 */
function mapCursorByStatement(original: string, formatted: string, cursor: number): number {
  const before = findStatementAtCursor(original, cursor);
  if (before == null) return Math.min(cursor, formatted.length);

  const after = buildStatementRanges(formatted).find((r) => r.index === before.index);
  if (after == null) return Math.min(cursor, formatted.length);

  return Math.min(after.contentFrom, formatted.length);
}

function safeFormat(
  sql: string,
  databaseType: string | undefined,
  options: SqlFormatOptions,
): string | null {
  try {
    return formatSql(sql, databaseType, options);
  } catch {
    // sql-formatter throws on syntax it cannot parse; leaving the buffer
    // untouched is strictly better than corrupting in-progress SQL.
    return null;
  }
}

function leadingIndentOf(view: EditorView, from: number): string {
  const line = view.state.doc.lineAt(from);
  // Only treat it as a block indent when the selection starts the line.
  const prefix = view.state.sliceDoc(line.from, from);
  return /^[ \t]*$/.test(prefix) ? prefix : '';
}

function reindent(text: string, indent: string): string {
  return text
    .split('\n')
    .map((line, i) => (i === 0 || line.length === 0 ? line : `${indent}${line}`))
    .join('\n');
}
