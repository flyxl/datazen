/**
 * SQL editor E2E helpers for DataZen query workspace.
 *
 * Selector conventions (frozen for downstream tracks):
 * - Query shell: `[data-testid="query-panel"]`
 * - Editor surface: `[data-testid="query-panel"] .cm-editor .cm-content`
 * - Statement gutter lane: `.cm-run-statement-gutter`
 * - Statement play marker: `.cm-run-statement-gutter .cm-run-statement-marker`
 * - Running marker: `.cm-run-statement-marker--executing`
 * - CodeMirror tooltip: `.cm-tooltip`, autocomplete: `.cm-tooltip-autocomplete`
 * - Confirm dialog: `[data-testid="confirm-dialog-ok"]`
 * - AI chat panel: `[data-testid="ai-chat-panel"]`, input: `[data-testid="ai-input-field"]`
 */
import { browser } from '@wdio/globals';

/** Minimal CodeMirror EditorView handle exposed on the DOM for E2E. */
interface CmEditorView {
  state: { doc: { toString(): string; line(n: number): { from: number; to: number } } };
  dispatch(spec: { selection?: { anchor: number; head?: number } }): void;
}

/** Select a substring by literal match (first occurrence). */
export async function selectSqlEditorSubstring(literal: string): Promise<boolean> {
  return browser.execute((needle) => {
    const editor = document.querySelector('[data-testid="sql-editor"]');
    const view = (editor as (HTMLElement & { cmView?: { view: CmEditorView } }) | null)?.cmView
      ?.view;
    if (!view) return false;
    const doc = view.state.doc.toString();
    const start = doc.indexOf(needle);
    if (start < 0) return false;
    view.dispatch({ selection: { anchor: start, head: start + needle.length } });
    return true;
  }, literal);
}
