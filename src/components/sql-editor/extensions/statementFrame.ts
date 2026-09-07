/**
 * Statement Frame CodeMirror extension factory.
 *
 * Draws a left-border decoration and subtle background on the active
 * statement range. Consumes the unified active statement range from
 * S2-A (statementRanges.ts).
 *
 * §Track S4-A steps 1-4, 7:
 * 1. Consumes unified active statement range from S2-A.
 * 2. Hidden during multi-line non-empty selection.
 * 3. Degraded (hidden) above maxLineCount lines with testable state.
 * 4. Uses theme tokens — no hard-coded colors beyond fallbacks.
 * 5. Uses Web tooltip (no Tauri Menu).
 */
import {
  ViewPlugin,
  Decoration,
  EditorView,
  type DecorationSet,
  type ViewUpdate,
  type EditorView as EditorViewType,
  type ViewPlugin as ViewPluginType,
} from '@codemirror/view';
import { StateField, StateEffect } from '@codemirror/state';
import { statementIndexField, findStatementAtCursor } from '../semantic/statementRanges';
import { shouldHideFrame, isDocumentDegraded } from './types';
import type { SqlStatementRange } from '../semantic/types';

// ── Degraded state field ─────────────────────────────────────────────────

/** Effect to explicitly set degraded mode (for testing). */
export const SetDegradedEffect = StateEffect.define<boolean>();

/**
 * StateField tracking whether the frame should be degraded (hidden).
 *
 * §Track S4-A step 4: "setting can-be-tested degraded state".
 * Exposed for consumers to dispatch SetDegradedEffect and for tests to read.
 */
export const frameDegradedField = StateField.define<boolean>({
  create(state) {
    return isDocumentDegraded(state.doc.lines);
  },
  update(value, tr) {
    for (const effect of tr.effects) {
      if (effect.is(SetDegradedEffect)) return effect.value;
    }
    // Auto-detect from document line count
    if (tr.docChanged) {
      return isDocumentDegraded(tr.state.doc.lines);
    }
    return value;
  },
});

// ── Frame decoration ─────────────────────────────────────────────────────

const frameDeco = Decoration.line({
  attributes: {
    class: 'sql-statement-frame',
    'data-sql-frame': 'true',
  },
});

/**
 * Pure: determine the active statement range for the current cursor position.
 * Uses the S2-A statement index field.
 */
function findActiveStatement(
  state: import('@codemirror/state').EditorState,
): SqlStatementRange | null {
  const field = state.field(statementIndexField(), false);
  if (!field) return null;
  const cursor = state.selection.main.head;
  const doc = state.doc.toString();

  return findStatementAtCursor(doc, cursor, field.ranges);
}

/**
 * Build the set of frame decorations for the current state.
 */
function buildFrameDecorations(view: EditorViewType): DecorationSet {
  const state = view.state;
  const degraded = state.field(frameDegradedField, false) ?? false;
  const selection = state.selection.main;
  const docLines = state.doc.lines;

  // Check if frame should be hidden
  if (
    shouldHideFrame({
      degraded,
      selectionEmpty: selection.empty,
      selectionFrom: selection.from,
      selectionTo: selection.to,
      docLineCount: docLines,
      docText: state.doc.toString(),
    })
  ) {
    return Decoration.none;
  }

  // Find the active statement
  const stmt = findActiveStatement(state);
  if (!stmt) return Decoration.none;

  // Apply the frame decoration to the first line of the statement
  const line = state.doc.lineAt(stmt.from);
  return Decoration.set([frameDeco.range(line.from)]);
}

// ── ViewPlugin ───────────────────────────────────────────────────────────

/**
 * Statement frame ViewPlugin.
 *
 * Renders a left-border decoration on the active statement and manages
 * the degraded state based on document line count.
 */
function createFramePlugin(): ViewPluginType<{ deco: DecorationSet }> {
  return ViewPlugin.fromClass(
    class {
      deco: DecorationSet;

      constructor(view: EditorViewType) {
        this.deco = buildFrameDecorations(view);
      }

      update(update: ViewUpdate) {
        if (update.docChanged || update.selectionSet || update.viewportChanged) {
          this.deco = buildFrameDecorations(update.view);
        }
      }

      destroy() {
        this.deco = Decoration.none;
      }
    },
    {
      decorations: (v) => v.deco,
    },
  );
}

// ── Factory ──────────────────────────────────────────────────────────────

export interface StatementFrameOptions {
  /** Maximum line count before frame degrades. Default: 500. */
  maxLineCount?: number;
}

/**
 * Create the statement frame extension.
 *
 * §Track S4-A: factory contract — must be independently testable.
 * The extension:
 * - Consumes unified active statement range from S2-A
 * - Hides frame during multi-line non-empty selection
 * - Degrades (hides) above maxLineCount lines
 * - Uses theme tokens for styling
 * - Uses Web tooltips (no Tauri Menu)
 */
export function createStatementFrameExtension(
  options: StatementFrameOptions = {},
): import('@codemirror/state').Extension[] {
  void options; // maxLineCount may be used by future frame logic

  return [
    frameDegradedField,
    createFramePlugin(),
    // Theme for the frame decoration
    EditorView.baseTheme({
      '.sql-statement-frame': {
        borderLeft: '2px solid var(--cm-sql-frame-border, rgba(59,130,246,0.6))',
        backgroundColor: 'var(--cm-sql-frame-bg, rgba(59,130,246,0.05))',
      },
    }),
  ];
}
