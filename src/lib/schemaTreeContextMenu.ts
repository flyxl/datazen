import type { NativeMenuItemDef } from './nativeContextMenu';

export type SchemaTreeNodeKind = 'table' | 'view' | 'database' | 'blank';

/** Caller-supplied labels (typically from `t()`). No hardcoded locale strings here. */
export type SchemaTreeContextMenuLabels = {
  open: string;
  openStructure: string;
  copyName: string;
  copyDdl: string;
  focusEr: string;
  exportData: string;
  importData: string;
  refresh: string;
  newQuery: string;
  copyDatabaseName: string;
  newTable: string;
  /** Batch export (toolbar / database / blank / optional table). */
  batchExport: string;
  truncate: string;
  drop: string;
  dropView: string;
};

export type SchemaTreeContextMenuHandlers = {
  onOpen?: () => void;
  onOpenStructure?: () => void;
  onCopyName?: () => void;
  onCopyDdl?: () => void;
  onFocusEr?: () => void;
  onExport?: () => void;
  onImport?: () => void;
  onRefresh?: () => void;
  onNewQuery?: () => void;
  onCopyDatabaseName?: () => void;
  onNewTable?: () => void;
  /** Open BatchExportDialog (database / blank / optional table). */
  onBatchExport?: () => void;
  onTruncate?: () => void;
  onDrop?: () => void;
};

export type BuildSchemaTreeContextMenuArgs = {
  kind: SchemaTreeNodeKind;
  labels: SchemaTreeContextMenuLabels;
  handlers: SchemaTreeContextMenuHandlers;
  /** Hide import / truncate / drop / new table when true. */
  readOnly?: boolean;
  /** Include open-structure item for tables. */
  showOpenStructure?: boolean;
  /** Include ER focus item for tables. */
  showErFocus?: boolean;
  /**
   * Include the single-table export item. Tables always export when
   * handlers.onExport is set (flag defaults true). Views only include export
   * when this is true. Pass `false` to hide export (e.g. when a driver forbids
   * exporting data).
   */
  showExport?: boolean;
  /**
   * Include the multi-table "batch export" item. Defaults to follow `showExport`
   * when omitted, so existing callers keep their behavior. Pass `false` to hide
   * batch export (e.g. a driver that cannot pull entire tables).
   */
  showBatchExport?: boolean;
  /** Include new-table on database / blank when !readOnly. */
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
 * Table order aligns with TablePlus: Open → Structure → New Query → Copy… → Export… → Truncate/Drop.
 */
export function buildSchemaTreeContextMenuItems(
  args: BuildSchemaTreeContextMenuArgs,
): NativeMenuItemDef[] {
  const {
    kind,
    labels,
    handlers,
    readOnly = false,
    showOpenStructure = false,
    showErFocus = false,
    showExport,
    showBatchExport,
    showNewTable = false,
  } = args;

  /** Default `showBatchExport` to follow `showExport` when not explicitly provided. */
  const batchExportShown = (kind: SchemaTreeNodeKind): boolean =>
    showBatchExport ?? (kind === 'view' ? showExport === true : showExport !== false);

  switch (kind) {
    case 'table': {
      const includeExport = showExport !== false;
      const danger = !readOnly
        ? push(
            item('truncate', labels.truncate, handlers.onTruncate),
            item('drop', labels.drop, handlers.onDrop),
          )
        : [];
      const main = push(
        item('open', labels.open, handlers.onOpen),
        showOpenStructure
          ? item('open-structure', labels.openStructure, handlers.onOpenStructure)
          : null,
        showErFocus ? item('focus-er', labels.focusEr, handlers.onFocusEr) : null,
        item('new-query', labels.newQuery, handlers.onNewQuery),
        item('copy-name', labels.copyName, handlers.onCopyName),
        item('copy-ddl', labels.copyDdl, handlers.onCopyDdl),
        includeExport ? item('export', labels.exportData, handlers.onExport) : null,
        batchExportShown('table')
          ? item('batch-export', labels.batchExport, handlers.onBatchExport)
          : null,
        !readOnly ? item('import', labels.importData, handlers.onImport) : null,
      );
      if (danger.length === 0) return main;
      return [...main, { kind: 'separator' }, ...danger];
    }
    case 'view': {
      const includeExport = showExport === true;
      const main = push(
        item('open', labels.open, handlers.onOpen),
        item('copy-name', labels.copyName, handlers.onCopyName),
        item('copy-ddl', labels.copyDdl, handlers.onCopyDdl),
        includeExport ? item('export', labels.exportData, handlers.onExport) : null,
        batchExportShown('view')
          ? item('batch-export', labels.batchExport, handlers.onBatchExport)
          : null,
      );
      const drop = !readOnly ? item('drop-view', labels.dropView, handlers.onDrop) : null;
      if (!drop) return main;
      return [...main, { kind: 'separator' }, drop];
    }
    case 'database':
      return push(
        item('refresh', labels.refresh, handlers.onRefresh),
        item('new-query', labels.newQuery, handlers.onNewQuery),
        item('copy-database-name', labels.copyDatabaseName, handlers.onCopyDatabaseName),
        batchExportShown('database')
          ? item('batch-export', labels.batchExport, handlers.onBatchExport)
          : null,
        !readOnly ? item('import', labels.importData, handlers.onImport) : null,
        !readOnly && showNewTable ? item('new-table', labels.newTable, handlers.onNewTable) : null,
      );
    case 'blank':
      return push(
        item('refresh', labels.refresh, handlers.onRefresh),
        item('new-query', labels.newQuery, handlers.onNewQuery),
        batchExportShown('blank')
          ? item('batch-export', labels.batchExport, handlers.onBatchExport)
          : null,
        !readOnly ? item('import', labels.importData, handlers.onImport) : null,
        !readOnly && showNewTable ? item('new-table', labels.newTable, handlers.onNewTable) : null,
      );
  }
}
