import { getCachedDDL } from './schemaCache';
import { getSqlDialect } from './sqlDialects';
import { databaseCommands } from '../commands/database';
import type { DatabaseType } from '../types';

/**
 * Fetch relation DDL using the active database dialect query and cached execution.
 * Aligns 100% with TablePanel's DDLView sub-tab behavior.
 */
export async function fetchRelationDdl(
  dbSessionId: string,
  tableName: string,
  database: string,
  databaseType?: string,
  isView?: boolean,
  schema?: string | null,
): Promise<string> {
  if (!dbSessionId || !tableName || !database) return '';

  const dialect = databaseType ? getSqlDialect(databaseType as DatabaseType) : null;
  if (dialect?.ddl) {
    const { sql, extractColumnIndex } =
      isView && dialect.ddl.getViewDdlQuery
        ? dialect.ddl.getViewDdlQuery(tableName, schema)
        : dialect.ddl.getTableDdlQuery(tableName, schema);

    try {
      const ddl = await getCachedDDL(
        dbSessionId,
        tableName,
        sql,
        (rows) => {
          const row = rows[0];
          const val = row?.[extractColumnIndex];
          return typeof val === 'string' ? val : val != null ? String(val) : '';
        },
        database,
        { namespacePath: [database] },
      );
      if (ddl && ddl.trim()) return ddl;
    } catch {
      // Fall through to getObjectDdl
    }
  }

  try {
    const kind = isView ? 'view' : 'table';
    const effectiveSchema = schema ?? database ?? null;
    const ddl = await databaseCommands.getObjectDdl(
      dbSessionId,
      kind,
      tableName,
      effectiveSchema,
      null,
      null,
      null,
      database,
    );
    if (ddl && ddl.trim()) return ddl;
  } catch {
    // ignore
  }

  return '';
}

/**
 * Robust clipboard copy supporting modern Clipboard API, Tauri native backend, and execCommand fallback.
 */
export async function copyToClipboard(text: string): Promise<boolean> {
  if (!text) return false;

  // 1. Try modern Clipboard API first (works when within synchronous user gesture)
  try {
    if (typeof navigator !== 'undefined' && navigator.clipboard?.writeText) {
      await navigator.clipboard.writeText(text);
      return true;
    }
  } catch {
    // If Clipboard API throws (e.g. NotAllowedError in WebKit when async), fall through
  }

  // 2. Try Tauri native backend IPC (bypasses browser user-gesture restrictions completely)
  try {
    const { invoke } = await import('@tauri-apps/api/core');
    await invoke('write_clipboard', { text });
    return true;
  } catch {
    // Fall back to DOM execCommand
  }

  // 3. Fallback to execCommand
  try {
    if (typeof document !== 'undefined') {
      const el = document.createElement('textarea');
      el.value = text;
      el.style.position = 'fixed';
      el.style.opacity = '0';
      el.style.pointerEvents = 'none';
      document.body.appendChild(el);
      el.select();
      const ok = document.execCommand('copy');
      document.body.removeChild(el);
      return ok;
    }
  } catch {
    // ignore
  }

  return false;
}

/**
 * Rich-text clipboard copy (DB-10): a `text/html` flavor plus a plain-text
 * fallback, so "Copy as HTML Table" pastes a real table into Notion / Excel /
 * Google Sheets, and a plain-text target still gets a tab-separated table.
 *
 * The chain mirrors `copyToClipboard`'s three steps, because the reason that
 * chain exists (an async context-menu callback has lost the user gesture on
 * WKWebView) applies to the ClipboardItem path just as much as to writeText:
 *
 * 1. `navigator.clipboard.write` with a `ClipboardItem` of both flavors.
 * 2. Tauri's `write_clipboard_html` (writes text/html where the OS has one).
 * 3. The plain-text fallback of `copyToClipboard` — a table as text beats
 *    markup source as text.
 */
export async function copyRichTextToClipboard(html: string, text: string): Promise<boolean> {
  if (!html) return copyToClipboard(text);

  // 1. Modern Clipboard API with both flavors
  try {
    if (
      typeof navigator !== 'undefined' &&
      typeof ClipboardItem !== 'undefined' &&
      navigator.clipboard?.write
    ) {
      const item = new ClipboardItem({
        'text/html': new Blob([html], { type: 'text/html' }),
        'text/plain': new Blob([text], { type: 'text/plain' }),
      });
      await navigator.clipboard.write([item]);
      return true;
    }
  } catch {
    // NotAllowedError (no user gesture) or unsupported flavors — fall through
  }

  // 2. Tauri native backend IPC
  try {
    const { invoke } = await import('@tauri-apps/api/core');
    await invoke('write_clipboard_html', { html, altText: text });
    return true;
  } catch {
    // Fall back to plain text
  }

  // 3. Plain text, the same way every other copy lands
  return copyToClipboard(text);
}
