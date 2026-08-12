import type { NativeMenuItemDef } from './nativeContextMenu';

/** Caller-supplied labels (typically from `t()`). No hardcoded locale strings here. */
export type DataTableContextMenuLabels = {
  copyCell: string;
  copySelectedRows: string;
  export: string;
};

export type DataTableContextMenuHandlers = {
  onCopyCell?: () => void;
  onCopySelectedRows?: () => void;
  onExport?: () => void;
};

export type BuildDataTableContextMenuArgs = {
  labels: DataTableContextMenuLabels;
  handlers: DataTableContextMenuHandlers;
  /** Include copy-cell when non-empty (e.g. window.getSelection or focused cell). */
  cellText?: string | null;
  /** Include copy-selected-rows when non-empty (one string per row, typically TSV). */
  selectedRowTexts?: string[] | null;
  /** Include export when true. */
  exportEnabled?: boolean;
};

function item(
  id: string,
  label: string,
  action: (() => void) | undefined,
): NativeMenuItemDef | null {
  if (!action) return null;
  return { kind: 'item', id, label, action };
}

function push(...defs: Array<NativeMenuItemDef | null>): NativeMenuItemDef[] {
  return defs.filter((d): d is NativeMenuItemDef => d != null);
}

/** Serialize table rows to TSV (tab-separated cells, newline-separated rows). */
export function serializeDataTableRowsAsTsv(rows: unknown[][]): string {
  return rows
    .map((row) => row.map((cell) => (cell == null ? '' : String(cell))).join('\t'))
    .join('\n');
}

/**
 * Build native context-menu items for DataTable (copy cell / copy rows / export).
 */
export function buildDataTableContextMenuItems(
  args: BuildDataTableContextMenuArgs,
): NativeMenuItemDef[] {
  const { labels, handlers, cellText, selectedRowTexts, exportEnabled = false } = args;
  const hasCell = typeof cellText === 'string' && cellText.length > 0;
  const hasRows = Array.isArray(selectedRowTexts) && selectedRowTexts.length > 0;

  return push(
    hasCell ? item('copy-cell', labels.copyCell, handlers.onCopyCell) : null,
    hasRows
      ? item('copy-selected-rows', labels.copySelectedRows, handlers.onCopySelectedRows)
      : null,
    exportEnabled ? item('export', labels.export, handlers.onExport) : null,
  );
}
