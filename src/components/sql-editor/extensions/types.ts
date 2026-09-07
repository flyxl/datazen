/**
 * Shared types for statement frame, gutter, and execution state extensions.
 *
 * These extensions consume the unified active statement range from S2-A
 * (statementRanges.ts) and expose the same SqlExecutionTarget contract.
 */
import type { SqlStatementRange, SqlTextRange, SqlExecutionTarget } from '../semantic/types';

// Re-export consumers need
export type { SqlStatementRange, SqlTextRange, SqlExecutionTarget };

/** Execution status of the editor (mirrors §4.5 SqlEditorExecutionState). */
export type ExecutionStatus = 'idle' | 'running' | 'cancelling';

/** Immutable snapshot of the running execution state. */
export type EditorExecutionState = {
  status: ExecutionStatus;
  targetRange: SqlTextRange | null;
  documentVersion: number | null;
};

/**
 * Pure function: determines whether a document should degrade the frame.
 *
 * Extracted for testability — no DOM / CodeMirror dependency.
 * Default threshold is 500 lines (per §Track S4-A step 2).
 */
export function isDocumentDegraded(lineCount: number, maxLineCount = 500): boolean {
  return lineCount > maxLineCount;
}

/**
 * Pure function: determines whether the frame should be hidden.
 *
 * Frame is hidden when:
 * 1. Document is degraded (lineCount > maxLineCount)
 * 2. There is a multi-line, non-empty selection
 */
export function shouldHideFrame(opts: {
  degraded: boolean;
  selectionEmpty: boolean;
  selectionFrom: number;
  selectionTo: number;
  docLineCount: number;
  docText: string;
}): boolean {
  if (opts.degraded) return true;
  if (!opts.selectionEmpty && opts.selectionFrom !== opts.selectionTo) {
    // Check if the selection spans more than one line by counting newlines
    const text = opts.docText.slice(
      Math.min(opts.selectionFrom, opts.selectionTo),
      Math.max(opts.selectionFrom, opts.selectionTo),
    );
    if (text.includes('\n')) return true;
  }
  return false;
}

/** Platform detection — consistent with existing src/lib/sqlEditorContextMenu.ts */
export function isMacOS(): boolean {
  return typeof navigator !== 'undefined' && /Mac/i.test(navigator.platform || navigator.userAgent);
}

/** Returns the platform-aware shortcut label for execute action. */
export function executeShortcutLabel(): string {
  return isMacOS() ? '⌘ Enter' : 'Ctrl+Enter';
}

// ── Internal helpers ─────────────────────────────────────────────────────
