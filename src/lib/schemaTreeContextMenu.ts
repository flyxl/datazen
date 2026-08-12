import type { NativeMenuItemDef } from './nativeContextMenu';

export type SchemaTreeNodeKind = 'table' | 'view' | 'database' | 'blank';

/** Caller-supplied labels (typically from `t()`). No hardcoded locale strings here. */
export type SchemaTreeContextMenuLabels = {
  open: string;
  copyName: string;
  editStructure: string;
  focusEr: string;
  exportData: string;
  importData: string;
  refresh: string;
  newQuery: string;
  copyDatabaseName: string;
  newTable: string;
};

export type SchemaTreeContextMenuHandlers = {
  onOpen?: () => void;
  onCopyName?: () => void;
  onEditStructure?: () => void;
  onFocusEr?: () => void;
  onExport?: () => void;
  onImport?: () => void;
  onRefresh?: () => void;
  onNewQuery?: () => void;
  onCopyDatabaseName?: () => void;
  onNewTable?: () => void;
};

export type BuildSchemaTreeContextMenuArgs = {
  kind: SchemaTreeNodeKind;
  labels: SchemaTreeContextMenuLabels;
  handlers: SchemaTreeContextMenuHandlers;
  /** Hide import (table) and new table (blank) when true. */
  readOnly?: boolean;
  /** Include edit-structure item for tables. */
  showEditStructure?: boolean;
  /** Include ER focus item for tables. */
  showErFocus?: boolean;
  /**
   * Include export. Tables always export when handlers.onExport is set (flag defaults true).
   * Views only include export when this is true.
   */
  showExport?: boolean;
  /** Include new-table on blank when !readOnly. */
  showNewTable?: boolean;
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

/**
 * Build native context-menu items for Schema tree nodes (table / view / database / blank).
 */
export function buildSchemaTreeContextMenuItems(
  args: BuildSchemaTreeContextMenuArgs,
): NativeMenuItemDef[] {
  const {
    kind,
    labels,
    handlers,
    readOnly = false,
    showEditStructure = false,
    showErFocus = false,
    showExport,
    showNewTable = false,
  } = args;

  switch (kind) {
    case 'table': {
      const includeExport = showExport !== false;
      return push(
        item('open', labels.open, handlers.onOpen),
        item('copy-name', labels.copyName, handlers.onCopyName),
        showEditStructure
          ? item('edit-structure', labels.editStructure, handlers.onEditStructure)
          : null,
        showErFocus ? item('focus-er', labels.focusEr, handlers.onFocusEr) : null,
        includeExport ? item('export', labels.exportData, handlers.onExport) : null,
        !readOnly ? item('import', labels.importData, handlers.onImport) : null,
      );
    }
    case 'view': {
      const includeExport = showExport === true;
      return push(
        item('open', labels.open, handlers.onOpen),
        item('copy-name', labels.copyName, handlers.onCopyName),
        includeExport ? item('export', labels.exportData, handlers.onExport) : null,
      );
    }
    case 'database':
      return push(
        item('refresh', labels.refresh, handlers.onRefresh),
        item('new-query', labels.newQuery, handlers.onNewQuery),
        item('copy-database-name', labels.copyDatabaseName, handlers.onCopyDatabaseName),
      );
    case 'blank':
      return push(
        item('refresh', labels.refresh, handlers.onRefresh),
        item('new-query', labels.newQuery, handlers.onNewQuery),
        !readOnly && showNewTable ? item('new-table', labels.newTable, handlers.onNewTable) : null,
      );
  }
}
