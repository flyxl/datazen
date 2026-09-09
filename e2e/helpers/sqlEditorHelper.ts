/**
 * SQL editor E2E helpers for DataZen query workspace.
 *
 * Centralises CodeMirror interaction, keyboard shortcuts, gutter markers,
 * and selector conventions for upcoming smart SQL editor journeys (Stage 2+).
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
import { browser, $ } from '@wdio/globals';
import { setEditorContent } from '../helpers.js';

/** Frozen selector map for SQL editor E2E specs. */
export const SQL_EDITOR_SELECTORS = {
  queryPanel: '[data-testid="query-panel"]',
  editorRoot: '[data-testid="query-panel"] .cm-editor',
  editorContent: '[data-testid="query-panel"] .cm-editor .cm-content',
  executeButton: '[data-testid="editor-execute-button"]',
  stopButton: '[data-testid="editor-stop-button"]',
  explainButton: '[data-testid="editor-explain-button"]',
  historyToggle: '[data-testid="editor-history-toggle"]',
  favoritesToggle: '[data-testid="editor-favorites-toggle"]',
  contextSelectors: '[data-testid="query-context-selectors"]',
  errorMessage: '[data-testid="query-error-message"]',
  retryButton: '[data-testid="query-retry"]',
  /** Statement execution gutter (Stage 3+). */
  statementGutter: '.cm-run-statement-gutter',
  statementMarker: '.cm-run-statement-gutter .cm-run-statement-marker',
  statementMarkerExecuting: '.cm-run-statement-marker--executing',
  /** CodeMirror overlays. */
  tooltip: '.cm-tooltip',
  autocomplete: '.cm-tooltip-autocomplete',
  /** Shared confirm / AI surfaces. */
  confirmOk: '[data-testid="confirm-dialog-ok"]',
  aiChatPanel: '[data-testid="ai-chat-panel"]',
  aiInputField: '[data-testid="ai-input-field"]',
  aiNotConfigured: '[data-testid="ai-not-configured"]',
} as const;

/** Minimal CodeMirror EditorView handle exposed on the DOM for E2E. */
interface CmEditorView {
  state: { doc: { toString(): string; line(n: number): { from: number; to: number } } };
  dispatch(spec: { selection?: { anchor: number; head?: number } }): void;
}

function readCmView(): CmEditorView | null {
  const editor = document.querySelector(SQL_EDITOR_SELECTORS.editorRoot);
  if (!editor) return null;
  const view = (editor as HTMLElement & { cmView?: { view: CmEditorView } }).cmView?.view;
  return view ?? null;
}

/** Wait until the query editor CodeMirror surface is visible and focused-ready. */
export async function waitForSqlEditor(timeout = 10000): Promise<void> {
  const editor = await $(SQL_EDITOR_SELECTORS.editorContent);
  await editor.waitForDisplayed({ timeout });
}

/** Replace the full SQL editor document (delegates to shared helper). */
export async function setSqlEditorText(sql: string): Promise<void> {
  await setEditorContent(sql);
}

/** Read the current editor document text. */
export async function getSqlEditorText(): Promise<string> {
  return browser.execute(() => {
    const view = readCmView();
    if (view) return view.state.doc.toString();
    const el = document.querySelector(SQL_EDITOR_SELECTORS.editorContent);
    return el?.textContent ?? '';
  });
}

/** Move the primary cursor to a document offset. */
export async function setSqlEditorCursor(pos: number): Promise<void> {
  await browser.execute((offset) => {
    const view = readCmView();
    if (!view) return;
    const clamped = Math.max(0, Math.min(offset, view.state.doc.toString().length));
    view.dispatch({ selection: { anchor: clamped } });
  }, pos);
  await browser.pause(100);
}

/** Move the primary cursor to a 1-based line number (column 0). */
export async function setSqlEditorCursorLine(lineNumber: number): Promise<void> {
  await browser.execute((line) => {
    const view = readCmView();
    if (!view) return;
    const doc = view.state.doc;
    const totalLines = doc.toString().split('\n').length;
    const target = Math.max(1, Math.min(line, totalLines));
    const lineInfo = doc.line(target);
    view.dispatch({ selection: { anchor: lineInfo.from } });
  }, lineNumber);
  await browser.pause(100);
}

/** Set a non-empty text selection by document offsets. */
export async function setSqlEditorSelection(from: number, to: number): Promise<void> {
  await browser.execute(
    (start, end) => {
      const view = readCmView();
      if (!view) return;
      const text = view.state.doc.toString();
      const anchor = Math.max(0, Math.min(start, text.length));
      const head = Math.max(0, Math.min(end, text.length));
      view.dispatch({ selection: { anchor, head } });
    },
    from,
    to,
  );
  await browser.pause(100);
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

async function dispatchModifierEnter(modifier: 'execute-current' | 'execute-all'): Promise<void> {
  const isMac = await browser.execute(() => /Mac|iPhone|iPad/.test(navigator.platform));
  const modKey = isMac ? 'Meta' : 'Control';
  const keys = modifier === 'execute-all' ? [modKey, 'Shift', 'Enter'] : [modKey, 'Enter'];
  await browser.keys(keys);
  await browser.pause(200);
}

/** Dispatch Mod+Enter (execute selection or current statement). */
export async function dispatchExecuteCurrentStatement(): Promise<void> {
  await dispatchModifierEnter('execute-current');
}

/** Dispatch Mod+Shift+Enter (execute full script). */
export async function dispatchExecuteAllStatements(): Promise<void> {
  await dispatchModifierEnter('execute-all');
}

export interface GutterMarkerInfo {
  index: number;
  lineNumber: number | null;
  className: string;
  isExecuting: boolean;
  isActive: boolean;
}

/** List statement gutter play markers (empty until Stage 3 gutter lands). */
export async function getGutterMarkers(): Promise<GutterMarkerInfo[]> {
  return browser.execute((selectors) => {
    const gutter = document.querySelector(selectors.statementGutter);
    if (!gutter) return [];
    const markers = Array.from(gutter.querySelectorAll<HTMLElement>(selectors.statementMarker));
    return markers.map((el, index) => {
      const lineAttr = el.closest('.cm-gutterElement')?.getAttribute('data-line-number');
      return {
        index,
        lineNumber: lineAttr ? Number(lineAttr) : null,
        className: el.className,
        isExecuting: el.classList.contains('cm-run-statement-marker--executing'),
        isActive: el.classList.contains('cm-run-statement-marker--active'),
      };
    });
  }, SQL_EDITOR_SELECTORS);
}

/** Click the gutter play marker at the given index. */
export async function clickGutterMarker(index: number): Promise<void> {
  await browser.execute(
    (selectors, markerIndex) => {
      const gutter = document.querySelector(selectors.statementGutter);
      if (!gutter) throw new Error('statement gutter not found');
      const markers = gutter.querySelectorAll<HTMLElement>(selectors.statementMarker);
      const target = markers[markerIndex];
      if (!target) throw new Error(`gutter marker ${markerIndex} not found`);
      target.click();
    },
    SQL_EDITOR_SELECTORS,
    index,
  );
  await browser.pause(200);
}

/** Wait until query execution sequence increments on the query panel. */
export async function waitForQueryExecution(timeout = 15000): Promise<void> {
  const queryPanel = await $(SQL_EDITOR_SELECTORS.queryPanel);
  const previousSeq = Number((await queryPanel.getAttribute('data-execution-seq')) ?? '0');
  await browser.waitUntil(
    async () => {
      const seq = Number((await queryPanel.getAttribute('data-execution-seq')) ?? '0');
      return seq > previousSeq;
    },
    { timeout, timeoutMsg: 'Timed out waiting for query execution to start/finish' },
  );
}

/** Return true when a CodeMirror tooltip is visible. */
export async function isEditorTooltipVisible(): Promise<boolean> {
  const tooltip = await $(SQL_EDITOR_SELECTORS.tooltip);
  return tooltip.isDisplayed().catch(() => false);
}

/** Return true when the AI chat panel is open. */
export async function isAiChatPanelOpen(): Promise<boolean> {
  const panel = await $(SQL_EDITOR_SELECTORS.aiChatPanel);
  if (await panel.isExisting()) {
    return panel.isDisplayed().catch(() => false);
  }
  const fallback = await $(SQL_EDITOR_SELECTORS.aiNotConfigured);
  return fallback.isDisplayed().catch(() => false);
}
