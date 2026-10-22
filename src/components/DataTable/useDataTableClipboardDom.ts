/**
 * The grid's `copy` / `paste` DOM binding (DB-03 §3.6).
 *
 * Extracted from `DataTable.tsx` for the same reason `useDataTableContextMenu` was extracted:
 * the grid is a layout file, and every chapter adds one more gesture it has to bind. This
 * hook owns exactly one decision — **is this gesture ours?** — and nothing else:
 *
 * - no bridge (`clipboard` missing) ⇒ it returns `{}`, so no listener is bound at all and a
 *   grid that did not opt in behaves byte for byte as before;
 * - an open editor or any editable target keeps the native behaviour: pasting into a text
 *   field must paste into that field, not into the grid;
 * - no landing cell ⇒ a `paste` is not intercepted, so plain text still pastes into the page;
 * - the values themselves come from the bridge, never from the DOM.
 */
import { useMemo } from 'react';
import { isEditableTarget } from '../../lib/gridEventGuards';
import {
  normalizeSelectionRange,
  type GridSelection,
} from '../../stores/tableData/gridSelection';
import type { GridClipboardHandlers, GridCopyTarget } from './gridClipboardBridge';

export interface UseDataTableClipboardDomOptions {
  clipboard?: GridClipboardHandlers;
  editingCell: { row: number; col: string } | null;
  /** The cell-selection contract is on for this grid. */
  enabled: boolean;
  columnNames: readonly string[];
  selection: GridSelection;
  selectedRows: ReadonlySet<number>;
}

export interface UseDataTableClipboardDomResult {
  onCopy?: (event: React.ClipboardEvent<HTMLDivElement>) => void;
  onPaste?: (event: React.ClipboardEvent<HTMLDivElement>) => void;
}

export function useDataTableClipboardDom({
  clipboard,
  editingCell,
  enabled,
  columnNames,
  selection,
  selectedRows,
}: UseDataTableClipboardDomOptions): UseDataTableClipboardDomResult {
  /**
   * What a `copy` gesture should copy (§3.7).
   *
   * A `cell` selection copies only the normalized **main rectangle** — never the
   * `extraRanges`, because TSV cannot express a hole and filling one in would put values the
   * user did not select on the clipboard. In row mode the row set is copied verbatim.
   */
  const copyTarget = useMemo<GridCopyTarget | null>(() => {
    if (!enabled) return null;
    if (selection.mode === 'cell') {
      const range = normalizeSelectionRange(selection, columnNames);
      if (!range) return null;
      return {
        mode: 'cell',
        range: {
          topRow: range.start.rowIndex,
          bottomRow: range.end.rowIndex,
          leftColumn: range.start.columnName,
          rightColumn: range.end.columnName,
        },
      };
    }
    if (selection.mode === 'row' && selectedRows.size > 0) {
      return { mode: 'row', rowIndices: Array.from(selectedRows).sort((a, b) => a - b) };
    }
    return null;
  }, [columnNames, enabled, selectedRows, selection]);
  /** Where a paste lands: the **top-left** of the normalized region (00-contracts §4.2). */
  const pasteAnchor = useMemo(() => {
    if (!enabled || selection.mode !== 'cell') return null;
    const range = normalizeSelectionRange(selection, columnNames);
    return range ? { rowIndex: range.start.rowIndex, columnName: range.start.columnName } : null;
  }, [columnNames, enabled, selection]);

  return useMemo(() => {
    if (!clipboard) return {};
    return {
      onCopy: (event: React.ClipboardEvent<HTMLDivElement>) => {
        if (editingCell) return;
        if (isEditableTarget(event.target)) return;
        if (!copyTarget) return;
        const text = clipboard.buildCopyText(copyTarget);
        if (text === null) return;
        event.preventDefault();
        if (text === '') {
          // An empty rectangle is not a successful copy: `copyToClipboard('')` is `false` by
          // contract, and reporting "copied" would be a lie.
          clipboard.onCopied(copyTarget, false);
          return;
        }
        event.clipboardData.setData('text/plain', text);
        clipboard.onCopied(copyTarget, true);
      },
      onPaste: (event: React.ClipboardEvent<HTMLDivElement>) => {
        if (editingCell) return;
        if (isEditableTarget(event.target)) return;
        if (!pasteAnchor) return;
        const text = event.clipboardData.getData('text/plain');
        event.preventDefault();
        clipboard.onPasteText(text, pasteAnchor);
      },
    };
  }, [clipboard, copyTarget, editingCell, pasteAnchor]);
}
