/**
 * Statement Gutter CodeMirror extension factory.
 *
 * Shows play markers on the first executable line of each statement.
 * Running statements show a spinner. Clicking a marker generates
 * SqlExecutionTarget(source='gutter').
 *
 * §Track S4-A steps 3, 5, 6, 7, 8:
 * 3. Only shows play on first executable line of each statement.
 * 5. Click generates SqlExecutionTarget(source='gutter').
 * 6. Running spinner matched by documentVersion + targetRange.
 * 7. Platform-aware tooltip (macOS vs others).
 * 8. Uses theme tokens and Web tooltips (no Tauri Menu).
 */
import {
  gutter,
  GutterMarker,
  EditorView,
  type GutterMarker as GutterMarkerType,
} from '@codemirror/view';
import { StateField, StateEffect } from '@codemirror/state';
import { statementIndexField } from '../semantic/statementRanges';
import { executionStateField, matchesRunningTarget } from './executionState';
import { executeShortcutLabel } from './types';
import type { SqlStatementRange } from '../semantic/types';
import type { EditorExecutionState } from './types';

// ── Play Marker ──────────────────────────────────────────────────────────

class PlayMarker extends GutterMarker {
  private _isRunning: boolean;
  private _lineFrom: number;
  private _lineNum: number;

  constructor(isRunning = false, lineFrom = 0, lineNum = 0) {
    super();
    this._isRunning = isRunning;
    this._lineFrom = lineFrom;
    this._lineNum = lineNum;
  }

  toDOM(_view: EditorView): HTMLElement {
    const el = document.createElement('span');
    el.className = this._isRunning
      ? 'sql-gutter-marker sql-gutter-running'
      : 'sql-gutter-marker sql-gutter-idle';
    el.dataset.lineFrom = String(this._lineFrom);
    el.dataset.lineNum = String(this._lineNum);

    if (this._isRunning) {
      el.innerHTML =
        '<svg class="sql-gutter-spinner" viewBox="0 0 16 16" width="14" height="14">' +
        '<circle cx="8" cy="8" r="6" stroke="currentColor" stroke-width="2" fill="none" ' +
        'stroke-dasharray="28" stroke-dashoffset="8" stroke-linecap="round"/>' +
        '</svg>';
    } else {
      el.innerHTML =
        '<svg viewBox="0 0 16 16" width="14" height="14" fill="currentColor">' +
        '<path d="M4 2.5v11l9-5.5z"/>' +
        '</svg>';
    }

    // §Track S4-A step 7: platform-aware tooltip via title attribute
    if (!this._isRunning) {
      el.title = `Execute statement (${executeShortcutLabel()})`;
    } else {
      el.title = 'Executing…';
    }

    return el;
  }

  eq(other: GutterMarkerType): boolean {
    if (!(other instanceof PlayMarker)) return false;
    return this._isRunning === other._isRunning && this._lineFrom === other._lineFrom;
  }
}

// ── Memoized range lookup ────────────────────────────────────────────────

const firstExecLinesCache = new WeakMap<object, Set<number>>();

function computeFirstExecLines(ranges: readonly SqlStatementRange[]): Set<number> {
  const lines = new Set<number>();
  for (const range of ranges) {
    if (range.contentTo > range.contentFrom) {
      // firstExecutableLine is a 1-based document line number
      lines.add(range.firstExecutableLine);
    }
  }
  return lines;
}

function getFirstExecLines(state: import('@codemirror/state').EditorState): Set<number> {
  const field = state.field(statementIndexField(), false);
  if (!field) return new Set();
  let lines = firstExecLinesCache.get(field);
  if (!lines) {
    lines = computeFirstExecLines(field.ranges);
    firstExecLinesCache.set(field, lines);
  }
  return lines;
}

// ── Gutter click handler ─────────────────────────────────────────────────

/** Callback invoked when a gutter marker is clicked. */
export type GutterClickCallback = (target: {
  source: 'gutter';
  sql: string;
  range: { from: number; to: number } | null;
  statementIndex: number | null;
  documentVersion: number;
}) => void;

/** Effect to increment document version for execution matching. */
export const IncrementDocVersionEffect = StateEffect.define<void>();

/**
 * StateField tracking the document version for gutter execution matching.
 * §Track S4-A step 6: spinner matched by documentVersion + targetRange.
 */
export const docVersionField = StateField.define<number>({
  create: () => 0,
  update(value, tr) {
    let next = value;
    for (const effect of tr.effects) {
      if (effect.is(IncrementDocVersionEffect)) {
        next = value + 1;
      }
    }
    if (tr.docChanged) {
      return next + 1;
    }
    return next;
  },
});

// ── Gutter factory ───────────────────────────────────────────────────────

export interface StatementGutterOptions {
  /** Callback when a gutter play marker is clicked. */
  onExecute?: GutterClickCallback;
  /** Maximum line count before frame degrades. Default: 500. */
  maxLineCount?: number;
}

/**
 * Create the statement gutter extension.
 *
 * §Track S4-A: factory contract — independently testable.
 */
export function createStatementGutterExtension(
  options: StatementGutterOptions = {},
): import('@codemirror/state').Extension[] {
  const { onExecute } = options;

  return [
    docVersionField,
    gutter({
      class: 'sql-statement-gutter',
      lineMarker(view, line) {
        const state = view.state;
        // In degraded mode, still show gutter but without frame
        // §Track S4-A step 2: "only keep gutter"
        const firstExecLines = getFirstExecLines(state);
        const lineNum = state.doc.lineAt(line.from).number;

        if (!firstExecLines.has(lineNum)) return null;

        // Check if this statement is currently executing
        const execState: EditorExecutionState = state.field(executionStateField);
        const docVersion = state.field(docVersionField);
        const stmtRange = findStatementForLine(state, line.from);

        const isRunning =
          execState.status === 'running' &&
          stmtRange !== null &&
          matchesRunningTarget(execState, docVersion, stmtRange);

        return new PlayMarker(isRunning, line.from, lineNum);
      },
      domEventHandlers: {
        click(view, line, event) {
          const target = event.target as HTMLElement;
          const marker = target.closest('.sql-gutter-idle') as HTMLElement | null;
          if (!marker) return false;

          event.preventDefault();
          event.stopPropagation();

          const lineFrom =
            marker.dataset.lineFrom !== undefined ? Number(marker.dataset.lineFrom) : line.from;
          const stmt = findStatementForLine(view.state, lineFrom);
          if (!stmt) return false;

          const docVersion = view.state.field(docVersionField);
          const sql = view.state.doc.sliceString(stmt.from, stmt.to).trim();

          onExecute?.({
            source: 'gutter',
            sql,
            range: { from: stmt.contentFrom, to: stmt.contentTo },
            statementIndex: stmt.index,
            documentVersion: docVersion,
          });

          return true;
        },
      },
      lineMarkerChange(update) {
        return (
          update.docChanged ||
          update.selectionSet ||
          update.transactions.some((tr) =>
            tr.effects.some((e) => e.is(IncrementDocVersionEffect) || e.is(SetDegradedEffect)),
          )
        );
      },
    }),
    // Event handler for gutter marker clicks (fallback)
    EditorView.domEventHandlers({
      click(e, view) {
        const target = e.target as HTMLElement;
        const marker = target.closest('.sql-gutter-idle') as HTMLElement | null;
        if (!marker) return false;

        e.preventDefault();
        e.stopPropagation();

        let lineFrom: number | null = null;
        if (marker.dataset.lineFrom !== undefined) {
          lineFrom = Number(marker.dataset.lineFrom);
        } else {
          const gutterElement = target.closest('.cm-gutterElement');
          if (gutterElement) {
            const rect = gutterElement.getBoundingClientRect();
            const y = (rect.top + rect.bottom) / 2 - view.documentTop;
            lineFrom = view.lineBlockAtHeight(y).from;
          }
        }

        if (lineFrom === null) {
          const rect = target.getBoundingClientRect();
          const y = (rect.top + rect.bottom) / 2 - view.documentTop;
          lineFrom = view.lineBlockAtHeight(y).from;
        }

        const stmt = findStatementForLine(view.state, lineFrom);
        if (!stmt) return false;

        const docVersion = view.state.field(docVersionField);
        const sql = view.state.doc.sliceString(stmt.from, stmt.to).trim();

        onExecute?.({
          source: 'gutter',
          sql,
          range: { from: stmt.contentFrom, to: stmt.contentTo },
          statementIndex: stmt.index,
          documentVersion: docVersion,
        });

        return true;
      },
    }),
    // Theme for gutter markers
    EditorView.baseTheme({
      '.sql-statement-gutter': {
        width: '28px',
        backgroundColor: 'var(--cm-gutter-background, inherit)',
      },
      '.sql-gutter-marker': {
        cursor: 'pointer',
        display: 'flex',
        alignItems: 'center',
        justifyContent: 'center',
        width: '100%',
        height: '100%',
        color: 'var(--cm-sql-gutter-color, #3b82f6)',
        opacity: '0.6',
        transition: 'opacity 0.15s',
        '&:hover': {
          opacity: '1',
        },
      },
      '.sql-gutter-running': {
        opacity: '1',
        color: 'var(--cm-sql-gutter-running-color, #f59e0b)',
      },
      '.sql-gutter-spinner': {
        animation: 'sql-gutter-spin 0.8s linear infinite',
      },
      '@keyframes sql-gutter-spin': {
        from: { transform: 'rotate(0deg)' },
        to: { transform: 'rotate(360deg)' },
      },
    }),
  ];
}

// ── Internal helpers ─────────────────────────────────────────────────────

/** Re-export degraded field for tests to import from gutter module. */
import { SetDegradedEffect } from './statementFrame';

/**
 * Find the statement range containing the given line.
 */
function findStatementForLine(
  state: import('@codemirror/state').EditorState,
  lineFrom: number,
): SqlStatementRange | null {
  const field = state.field(statementIndexField(), false);
  if (!field) return null;
  const lineNum = state.doc.lineAt(lineFrom).number;
  // First match directly by firstExecutableLine
  const byExecLine = field.ranges.find((r) => r.firstExecutableLine === lineNum);
  if (byExecLine) return byExecLine;

  for (const range of field.ranges) {
    if (lineFrom >= range.from && lineFrom <= range.to) {
      return range;
    }
  }
  return null;
}
