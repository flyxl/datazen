import { useCallback, type ReactNode } from 'react';
import { useI18n } from '../../hooks/useI18n';
import { useConfirmDialog } from '../../hooks/useConfirmDialog';
import { usePanelStore, type ConnectionContext } from '../../stores/panelStore';
import { useSchemaStore } from '../../stores/schemaStore';
import { useConnectionStore } from '../../stores/connectionStore';
import { useTableDataStore } from '../../stores/tableDataStore';
import { DB_REGISTRY, escapeIdent } from '../../lib/databaseTypes';
import { canOpenStructureEditor } from '../../lib/structureEditor/canOpenStructureEditor';
import {
  resolveExportScope,
  supportsAnyExport,
  supportsFullTableExport,
} from '../../lib/exportCapability';
import { invalidateSchemaCache } from '../../lib/schemaCache';
import { fetchRelationDdl, copyToClipboard } from '../../lib/fetchRelationDdl';
import { showNativeContextMenu } from '../../lib/nativeContextMenu';
import { buildSchemaTreeContextMenuItems } from '../../lib/schemaTreeContextMenu';
import { getSqlDialect } from '../../lib/sqlDialects';
import {
  fetchTableSchemaForSqlGeneration,
  generateTableSqlWithFallbacks,
} from '../../lib/tableSchemaForSql';
import { type GeneratedSqlType } from '../../lib/sqlGenerator';
import { openBackupWindow } from '../../lib/windowManager';
import { queryCommands } from '../../commands/query';
import type { SchemaTreeNodeContextMenuPayload } from './schema-tree/SchemaTree';
import type { PanelHandlers } from './usePanelHandlers';

export interface ConnectionContextMenuParams {
  sidebarConnCtx: ConnectionContext | null;
  currentDatabase: string | null;
  initialDatabase: string | undefined;
  handleSelectTableWithSchema: (table: string, schema?: string, database?: string) => void;
  handlers: PanelHandlers;
  openBatchExport: (initialSelected?: string[]) => void;
  safeMode: boolean;
  /** Open the export dialog for a table/view: set the target name and open it. */
  requestExport: (name: string, schema?: string) => void;
  /** Open the import dialog; `isTable` selects the table-target vs database import. */
  requestImport: (isTable: boolean, name: string) => void;
}

export interface ConnectionContextMenuResult {
  /** Exposed via `nodeContextMenuRef` so the external navigator can trigger it. */
  handleNodeContextMenu: (payload: SchemaTreeNodeContextMenuPayload) => void;
  /** Confirm-dialog node rendered alongside the dialogs. */
  confirmActionDialog: ReactNode;
}

/**
 * Builds the schema-tree node context menu handler for `ContentView`.
 *
 * Pure extraction of ContentView's previous inline `handleNodeContextMenu` callback:
 * identical labels/handlers and identical dependencies, no behaviour change.
 */
export function useConnectionContextMenu({
  sidebarConnCtx,
  currentDatabase,
  initialDatabase,
  handleSelectTableWithSchema,
  handlers,
  openBatchExport,
  safeMode,
  requestExport,
  requestImport,
}: ConnectionContextMenuParams): ConnectionContextMenuResult {
  const { t } = useI18n();
  const [confirmAction, confirmActionDialog] = useConfirmDialog();
  const removePanel = usePanelStore((s) => s.removePanel);
  const removeRelation = useSchemaStore((s) => s.removeRelation);
  const savedConnections = useConnectionStore((s) => s.connections);

  const handleNodeContextMenu = useCallback(
    (payload: SchemaTreeNodeContextMenuPayload) => {
      const ctx = sidebarConnCtx;
      if (!ctx) return;
      const { kind, name, schema } = payload;
      const ctxDbType = ctx.databaseType;
      const ctxDbMeta = DB_REGISTRY[ctxDbType];
      const saved = savedConnections.find((c) => c.id === ctx.connectionId);
      const ctxIsReadOnly = ctxDbMeta?.readOnly === true || saved?.readOnly === true;
      const ctxShowStructureEditor = canOpenStructureEditor(ctxDbMeta) && !ctxIsReadOnly;
      const ctxSupportsErDiagram = ctxDbMeta?.supportsErDiagram !== false;
      const ctxExportScope = resolveExportScope(ctxDbMeta);
      const ctxExportDataSupported = supportsAnyExport(ctxExportScope);
      const ctxBatchExportSupported = supportsFullTableExport(ctxExportScope);
      const scopedPanels = usePanelStore
        .getState()
        .panels.filter((p) => p.connectionId === ctx.connectionId);

      const copyText = (text: string) => {
        void copyToClipboard(text);
      };
      const quoted = escapeIdent(name, ctxDbType);

      const copyDdl = () => {
        void (async () => {
          try {
            const ddl = await fetchRelationDdl(
              ctx.dbSessionId,
              name,
              ctxDbType,
              kind === 'view',
              schema,
            );
            if (ddl) {
              await copyToClipboard(ddl);
            }
          } catch (e) {
            console.warn('Failed to copy DDL:', e);
          }
        })();
      };

      const confirmAndRun = async (
        message: string,
        title: string,
        sql: string,
        afterSuccess?: () => void,
      ) => {
        const confirmed = await confirmAction({ title, message, kind: 'warning' });
        if (!confirmed) return;
        const database = currentDatabase ?? initialDatabase ?? null;
        try {
          await queryCommands.executeQuery(
            ctx.dbSessionId,
            sql,
            undefined,
            database,
            schema ?? null,
          );
          afterSuccess?.();
        } catch (e) {
          console.warn(e);
        }
      };

      const closePanelsForTable = (table: string) => {
        const toClose = scopedPanels.filter((p) => p.type === 'table' && p.tableName === table);
        for (const p of toClose) removePanel(p.id);
      };

      const database = currentDatabase ?? initialDatabase ?? undefined;

      const handleGenerateTableSql = (type: GeneratedSqlType) => {
        void (async () => {
          const schemaState = useSchemaStore.getState().schemas.get(ctx.dbSessionId);
          const tableSchema = await fetchTableSchemaForSqlGeneration({
            dbSessionId: ctx.dbSessionId,
            tableName: name,
            schema,
            database,
            databaseType: ctxDbType,
            columnMap: schemaState?.columnMap,
          });
          const tableRef = schema ? `${schema}.${name}` : name;
          const sql = generateTableSqlWithFallbacks(tableSchema, type, ctxDbType, {
            schemaPrefix: schema,
            tableName: name,
            tableRefLabel: tableRef,
          });
          handlers.handleNewQuery(sql, { database, schema });
        })();
      };

      void showNativeContextMenu(
        buildSchemaTreeContextMenuItems({
          kind,
          labels: {
            open: kind === 'view' ? t('schemaTree.open') : t('schemaTree.openTable'),
            openStructure: t('schemaTree.openStructure'),
            copyName: t('common.copyName'),
            copyDdl: t('common.copyDdl'),
            focusEr: t('erDiagram.focusTable'),
            exportData: t('common.exportData'),
            importData: t('common.importData'),
            refresh: t('connWin.refresh'),
            newQuery: t('common.newQuery'),
            queryHistory: t('main.ctx.queryHistory'),
            copyDatabaseName: t('schemaTree.copyDatabaseName'),
            newTable: t('common.newTable'),
            batchExport: `${t('batchExport.title')}…`,
            truncate: t('schemaTree.truncate'),
            drop: t('schemaTree.drop'),
            dropView: t('schemaTree.dropView'),
            dropDatabase: t('schemaTree.dropDatabase'),
            dropSchema: t('schemaTree.dropSchema'),
            viewErDiagram: t('schemaTree.viewErDiagram'),
            newSchema: t('schemaTree.newSchema'),
            createSchema: t('common.createSchema'),
            executeSqlFile: t('common.executeSqlFile'),
            dataTransfer: t('common.dataTransfer'),
            compareSchema: t('schemaTree.compareSchema'),
            compareData: t('schemaTree.compareData'),
            backup: t('common.backupDatabase'),
            restore: t('common.restoreDatabase'),
            generateSql: t('schemaTree.generateSql'),
            generateSelect: t('schemaTree.generateSelect'),
            generateInsert: t('schemaTree.generateInsert'),
            generateUpdate: t('schemaTree.generateUpdate'),
            generateDelete: t('schemaTree.generateDelete'),
            generateDdl: t('schemaTree.generateDdl'),
          },
          handlers: {
            onOpen:
              kind === 'table' || kind === 'view'
                ? () => handleSelectTableWithSchema(name, schema)
                : undefined,
            onGenerateSelect: kind === 'table' ? () => handleGenerateTableSql('select') : undefined,
            onGenerateInsert: kind === 'table' ? () => handleGenerateTableSql('insert') : undefined,
            onGenerateUpdate: kind === 'table' ? () => handleGenerateTableSql('update') : undefined,
            onGenerateDelete: kind === 'table' ? () => handleGenerateTableSql('delete') : undefined,
            onOpenStructure:
              kind === 'table' ? () => handlers.handleOpenStructure(name) : undefined,
            onCopyName: kind === 'table' || kind === 'view' ? () => copyText(name) : undefined,
            onCopyDdl: kind === 'table' || kind === 'view' ? () => copyDdl() : undefined,
            onFocusEr: kind === 'table' ? () => handlers.handleOpenErDiagram(name) : undefined,
            onExport:
              kind === 'table' || kind === 'view' ? () => requestExport(name, schema) : undefined,
            onBatchExport: () => {
              if (kind === 'table' || kind === 'view') {
                openBatchExport([name]);
              } else {
                openBatchExport([]);
              }
            },
            onImport:
              !ctxIsReadOnly && (kind === 'table' || kind === 'database' || kind === 'blank')
                ? () => requestImport(kind === 'table', name)
                : undefined,
            onRefresh: handlers.handleRefresh,
            onNewQuery: () => {
              if (kind === 'table') {
                handlers.handleOpenTableAction(
                  {
                    connectionId: ctx.connectionId,
                    dbSessionId: ctx.dbSessionId,
                    databaseType: ctx.databaseType,
                    database: currentDatabase ?? initialDatabase,
                    schema,
                    tableName: name,
                  },
                  'select',
                );
              } else {
                handlers.handleNewQuery();
              }
            },
            onQueryHistory:
              kind === 'database' || kind === 'schema'
                ? () => handlers.handleOpenQueryHistory()
                : undefined,
            onCopyDatabaseName: kind === 'database' ? () => copyText(name) : undefined,
            onBackup:
              kind === 'database' && ctxDbMeta?.supportsBackup
                ? () =>
                    openBackupWindow('backup', { connectionId: ctx.connectionId, database: name })
                : undefined,
            onRestore:
              kind === 'database' && ctxDbMeta?.supportsBackup
                ? () =>
                    openBackupWindow('restore', { connectionId: ctx.connectionId, database: name })
                : undefined,
            onNewTable: handlers.handleCreateTable,
            onTruncate:
              kind === 'table' && !ctxIsReadOnly && !safeMode
                ? () => {
                    const dialect = getSqlDialect(ctxDbType);
                    const sql = dialect?.getTruncateTableSql
                      ? dialect.getTruncateTableSql(quoted)
                      : `TRUNCATE TABLE ${quoted}`;
                    void confirmAndRun(
                      t('schemaTree.confirmTruncate', { name }),
                      t('schemaTree.truncate'),
                      sql,
                      () => {
                        const store = useTableDataStore.getState();
                        if (store.activeTable === name) {
                          void store.loadTableData({
                            dbSessionId: ctx.dbSessionId,
                            table: name,
                            connectionId: ctx.connectionId,
                            driverType: ctx.databaseType,
                            database: currentDatabase,
                            schema,
                          });
                        }
                      },
                    );
                  }
                : undefined,
            onDrop:
              (kind === 'table' || kind === 'view') && !ctxIsReadOnly && !safeMode
                ? () => {
                    const isView = kind === 'view';
                    const sql = isView ? `DROP VIEW ${quoted}` : `DROP TABLE ${quoted}`;
                    void confirmAndRun(
                      t(isView ? 'schemaTree.confirmDropView' : 'schemaTree.confirmDrop', {
                        name,
                      }),
                      t(isView ? 'schemaTree.dropView' : 'schemaTree.drop'),
                      sql,
                      () => {
                        invalidateSchemaCache(ctx.dbSessionId, name);
                        removeRelation(name);
                        handlers.handleRefresh();
                        closePanelsForTable(name);
                      },
                    );
                  }
                : undefined,
          },
          readOnly: ctxIsReadOnly,
          safeMode,
          showOpenStructure: true,
          showErFocus: ctxSupportsErDiagram,
          showExport: kind === 'table' ? false : ctxExportDataSupported,
          showBatchExport: kind === 'table' ? false : ctxBatchExportSupported,
          showNewTable: ctxShowStructureEditor,
        }),
        { x: payload.x, y: payload.y },
      );
    },
    [
      sidebarConnCtx,
      currentDatabase,
      initialDatabase,
      t,
      handleSelectTableWithSchema,
      handlers,
      removeRelation,
      openBatchExport,
      safeMode,
      removePanel,
      confirmAction,
      savedConnections,
      requestExport,
      requestImport,
    ],
  );

  return { handleNodeContextMenu, confirmActionDialog };
}
