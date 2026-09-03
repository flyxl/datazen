import type { NativeMenuItemDef } from './nativeContextMenu';
import { isProductFeatureEnabled } from './productFeatures';

export type SchemaTreeNodeKind =
  | 'table'
  | 'view'
  | 'database'
  | 'schema'
  | 'category'
  | 'blank'
  | 'function'
  | 'procedure'
  | 'trigger'
  | 'sequence'
  | 'type';

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
  dropDatabase: string;
  viewErDiagram: string;
  newSchema: string;
  createSchema: string;
  dropSchema: string;
  executeSqlFile: string;
  queryHistory: string;
  dataTransfer: string;
  compareSchema: string;
  compareData: string;
  backup: string;
  restore: string;
  generateSql?: string;
  generateSelect?: string;
  generateInsert?: string;
  generateUpdate?: string;
  generateDelete?: string;
  generateDdl?: string;
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
  onGenerateSelect?: () => void;
  onGenerateInsert?: () => void;
  onGenerateUpdate?: () => void;
  onGenerateDelete?: () => void;
  onGenerateDdl?: () => void;
  onCopyDatabaseName?: () => void;
  onNewTable?: () => void;
  /** Open BatchExportDialog (database / blank / optional table). */
  onBatchExport?: () => void;
  onTruncate?: () => void;
  onDrop?: () => void;
  onDropDatabase?: () => void;
  onViewErDiagram?: () => void;
  onNewSchema?: () => void;
  onCreateSchema?: () => void;
  onDropSchema?: () => void;
  onExecuteSqlFile?: () => void;
  onQueryHistory?: () => void;
  onDataTransfer?: () => void;
  onCompareSchema?: () => void;
  onCompareData?: () => void;
  onBackup?: () => void;
  onRestore?: () => void;
};

export type BuildSchemaTreeContextMenuArgs = {
  kind: SchemaTreeNodeKind;
  labels: SchemaTreeContextMenuLabels;
  handlers: SchemaTreeContextMenuHandlers;
  /** Hide import / truncate / drop / new table when true. */
  readOnly?: boolean;
  /** Hide execute-sql-file and other mutating entries when safe mode disables them. */
  safeMode?: boolean;
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
  /** Category id for `kind: 'category'` (e.g. `'tables'`). */
  categoryId?: string;
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

function migrationToolItems(
  labels: SchemaTreeContextMenuLabels,
  handlers: SchemaTreeContextMenuHandlers,
): NativeMenuItemDef[] {
  return push(
    isProductFeatureEnabled('dataTransfer')
      ? item('data-transfer', labels.dataTransfer, handlers.onDataTransfer)
      : null,
    isProductFeatureEnabled('schemaDiff')
      ? item('compare-schema', labels.compareSchema, handlers.onCompareSchema)
      : null,
    isProductFeatureEnabled('dataSync')
      ? item('compare-data', labels.compareData, handlers.onCompareData)
      : null,
  );
}

function generateSqlSubmenu(
  labels: SchemaTreeContextMenuLabels,
  handlers: SchemaTreeContextMenuHandlers,
): NativeMenuItemDef | null {
  const hasAny =
    handlers.onGenerateSelect ||
    handlers.onGenerateInsert ||
    handlers.onGenerateUpdate ||
    handlers.onGenerateDelete ||
    handlers.onGenerateDdl;
  if (!hasAny) return null;

  const subItems = push(
    item('generate-select', labels.generateSelect ?? 'SELECT', handlers.onGenerateSelect),
    item('generate-insert', labels.generateInsert ?? 'INSERT', handlers.onGenerateInsert),
    item('generate-update', labels.generateUpdate ?? 'UPDATE', handlers.onGenerateUpdate),
    item('generate-delete', labels.generateDelete ?? 'DELETE', handlers.onGenerateDelete),
    item('generate-ddl', labels.generateDdl ?? 'DDL', handlers.onGenerateDdl),
  );
  if (subItems.length === 0) return null;
  return {
    kind: 'submenu',
    id: 'generate-sql',
    label: labels.generateSql ?? 'Generate SQL',
    items: subItems,
  };
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
    safeMode = false,
    showOpenStructure = false,
    showErFocus = false,
    showExport,
    showBatchExport,
    showNewTable = false,
    categoryId,
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
        generateSqlSubmenu(labels, handlers),
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
    case 'database': {
      const dbMain = push(
        item('refresh', labels.refresh, handlers.onRefresh),
        item('new-query', labels.newQuery, handlers.onNewQuery),
        item('query-history', labels.queryHistory, handlers.onQueryHistory),
        !readOnly && !safeMode
          ? item('execute-sql-file', labels.executeSqlFile, handlers.onExecuteSqlFile)
          : null,
        item('copy-database-name', labels.copyDatabaseName, handlers.onCopyDatabaseName),
        item('view-er-diagram', labels.viewErDiagram, handlers.onViewErDiagram),
        batchExportShown('database')
          ? item('batch-export', labels.batchExport, handlers.onBatchExport)
          : null,
        !readOnly ? item('import', labels.importData, handlers.onImport) : null,
        !readOnly && showNewTable ? item('new-table', labels.newTable, handlers.onNewTable) : null,
        item('create-schema', labels.createSchema, handlers.onCreateSchema),
      );
      const syncItems = migrationToolItems(labels, handlers);
      const backupItems = push(
        item('backup', labels.backup, handlers.onBackup),
        item('restore', labels.restore, handlers.onRestore),
      );
      const dropDb = !readOnly
        ? item('drop-database', labels.dropDatabase, handlers.onDropDatabase)
        : null;
      const parts = [...dbMain];
      if (syncItems.length > 0) parts.push({ kind: 'separator' as const }, ...syncItems);
      if (backupItems.length > 0) parts.push({ kind: 'separator' as const }, ...backupItems);
      if (dropDb) return [...parts, { kind: 'separator' as const }, dropDb];
      return parts;
    }
    case 'schema': {
      const schemaMain = push(
        item('refresh', labels.refresh, handlers.onRefresh),
        item('new-query', labels.newQuery, handlers.onNewQuery),
        item('query-history', labels.queryHistory, handlers.onQueryHistory),
        !readOnly && !safeMode
          ? item('execute-sql-file', labels.executeSqlFile, handlers.onExecuteSqlFile)
          : null,
        item('copy-schema-name', labels.copyName, handlers.onCopyName),
        item('view-er-diagram', labels.viewErDiagram, handlers.onViewErDiagram),
        batchExportShown('schema')
          ? item('batch-export', labels.batchExport, handlers.onBatchExport)
          : null,
        !readOnly ? item('import', labels.importData, handlers.onImport) : null,
        !readOnly ? item('new-table', labels.newTable, handlers.onNewTable) : null,
      );
      const schemaSyncItems = migrationToolItems(labels, handlers);
      const dropSch = !readOnly
        ? item('drop-schema', labels.dropSchema, handlers.onDropSchema)
        : null;
      const parts =
        schemaSyncItems.length > 0
          ? [...schemaMain, { kind: 'separator' as const }, ...schemaSyncItems]
          : schemaMain;
      if (dropSch) return [...parts, { kind: 'separator' as const }, dropSch];
      return parts;
    }
    case 'category':
      return push(
        item('refresh', labels.refresh, handlers.onRefresh),
        !readOnly && categoryId === 'tables'
          ? item('new-table', labels.newTable, handlers.onNewTable)
          : null,
        !readOnly ? item('import', labels.importData, handlers.onImport) : null,
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
    default:
      return push(item('copy-name', labels.copyName, handlers.onCopyName));
  }
}
