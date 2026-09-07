import { useCallback } from 'react';
import type { ConnectionContext } from '../../stores/panelStore';
import { usePanelStore } from '../../stores/panelStore';
import { useSchemaStore } from '../../stores/schemaStore';
import type { DataExportCapability } from '../../lib/exportCapability';
import type { ColumnSchema } from '../../types';
import { ExportDialog } from './ExportDialog';
import { BatchExportDialog } from './BatchExportDialog';
import { ImportDialog } from './ImportDialog';
import { ExecuteSqlFileDialog } from './ExecuteSqlFileDialog';
import { CreateDatabaseDialog } from './CreateDatabaseDialog';
import { CreateSchemaDialog } from './CreateSchemaDialog';
import { CreateUserDialog } from './CreateUserDialog';
import { loadBatchExportTableData } from '../../lib/loadBatchExportTable';

export interface ContentViewDialogsProps {
  /** Sidebar/toolbar connection context the dialogs apply to. */
  connCtx: ConnectionContext | null;
  currentDatabase: string | null;
  initialDatabase: string | undefined;
  /** Export capability of the *active* panel (matched to the original wiring). */
  exportCapability: DataExportCapability;

  exportOpen: boolean;
  exportTableName: string | null;
  onCloseExport: () => void;
  tableColumns: ColumnSchema[];
  tableRows: Record<string, unknown>[];
  selectedRows: Set<number>;
  totalRows: number;

  batchExportOpen: boolean;
  batchExportInitialSelected: string[];
  onCloseBatchExport: () => void;
  exportableTableNames: string[];

  importOpen: boolean;
  importTableName: string | null;
  onCloseImport: () => void;
  onImported: () => void;

  sqlFileDialogOpen: boolean;
  onCloseSqlFile: () => void;
  onExecuted: () => void;

  createDbOpen: boolean;
  onCloseCreateDb: () => void;

  createSchemaOpen: boolean;
  onCloseCreateSchema: () => void;

  createUserOpen: boolean;
  onCloseCreateUser: () => void;
}

/**
 * Hosts the multi-modal dialog set reachable from the connection workspace
 * (create database/schema/user, execute SQL file, import/export/batch-export),
 * including the create-* post-success side effects. Pure extraction from
 * `ContentView` — no behaviour change; the open booleans stay owned by
 * `ContentView` so external triggers (toolbar, context menu, `actionsRef`)
 * keep working verbatim.
 */
export function ContentViewDialogs({
  connCtx,
  currentDatabase,
  initialDatabase,
  exportCapability,

  exportOpen,
  exportTableName,
  onCloseExport,
  tableColumns,
  tableRows,
  selectedRows,
  totalRows,

  batchExportOpen,
  batchExportInitialSelected,
  onCloseBatchExport,
  exportableTableNames,

  importOpen,
  importTableName,
  onCloseImport,
  onImported,

  sqlFileDialogOpen,
  onCloseSqlFile,
  onExecuted,

  createDbOpen,
  onCloseCreateDb,

  createSchemaOpen,
  onCloseCreateSchema,

  createUserOpen,
  onCloseCreateUser,
}: ContentViewDialogsProps) {
  const loadForConnection = useSchemaStore((s) => s.loadForConnection);

  const handleDbCreated = useCallback(async () => {
    const ctx = connCtx;
    if (!ctx) return;
    await loadForConnection(ctx.dbSessionId, {
      preferredDatabase: initialDatabase,
      databaseType: ctx.databaseType,
      skipLoadTables: true,
    });
  }, [connCtx, initialDatabase, loadForConnection]);

  const handleSchemaCreated = useCallback(async () => {
    const ctx = connCtx;
    if (!ctx) return;
    const sessionId = ctx.dbSessionId;
    const db = currentDatabase ?? initialDatabase;
    if (db) {
      await useSchemaStore.getState().loadTables(db, sessionId);
    }
    await loadForConnection(sessionId, {
      preferredDatabase: initialDatabase,
      databaseType: ctx.databaseType,
      skipLoadTables: false,
    });
  }, [connCtx, currentDatabase, initialDatabase, loadForConnection]);

  const handleUserCreated = useCallback(() => {
    const ctx = connCtx;
    if (!ctx) return;
    const store = usePanelStore.getState();
    const existingPriv = store.panels.find(
      (p) => p.type === 'privileges' && p.connectionId === ctx.connectionId,
    );
    if (existingPriv) {
      store.removePanel(existingPriv.id);
    }
    const panel = {
      ...ctx,
      type: 'privileges' as const,
      id: `priv-${Date.now()}`,
    };
    store.addPanel(panel);
  }, [connCtx]);

  return (
    <>
      {exportOpen && exportTableName && connCtx && (
        <ExportDialog
          open={exportOpen}
          onClose={onCloseExport}
          tableName={exportTableName}
          columns={tableColumns}
          rows={tableRows}
          selectedRows={selectedRows}
          databaseType={connCtx.databaseType}
          dbSessionId={connCtx.dbSessionId}
          totalRows={totalRows}
          defaultScope="entire_table"
          dataExportCapability={exportCapability}
        />
      )}

      {connCtx && (
        <BatchExportDialog
          open={batchExportOpen}
          onClose={onCloseBatchExport}
          dbSessionId={connCtx.dbSessionId}
          databaseType={connCtx.databaseType}
          database={currentDatabase ?? undefined}
          tables={exportableTableNames}
          initialSelected={batchExportInitialSelected}
          loadTableExportData={loadTableExportData(connCtx)}
          dataExportCapability={exportCapability}
        />
      )}

      {connCtx && (
        <ImportDialog
          open={importOpen}
          onClose={onCloseImport}
          dbSessionId={connCtx.dbSessionId}
          tableName={importTableName}
          onImported={onImported}
          databaseType={connCtx.databaseType}
        />
      )}

      {connCtx && (
        <ExecuteSqlFileDialog
          open={sqlFileDialogOpen}
          onClose={onCloseSqlFile}
          dbSessionId={connCtx.dbSessionId}
          database={currentDatabase ?? initialDatabase ?? null}
          connectionName={connCtx.connectionName}
          onExecuted={onExecuted}
        />
      )}

      {connCtx && (
        <CreateDatabaseDialog
          open={createDbOpen}
          onClose={onCloseCreateDb}
          dbSessionId={connCtx.dbSessionId}
          onCreated={handleDbCreated}
        />
      )}

      {connCtx && (
        <CreateSchemaDialog
          open={createSchemaOpen}
          onClose={onCloseCreateSchema}
          dbSessionId={connCtx.dbSessionId}
          database={currentDatabase ?? initialDatabase ?? null}
          onCreated={handleSchemaCreated}
        />
      )}

      {connCtx && (
        <CreateUserDialog
          open={createUserOpen}
          onClose={onCloseCreateUser}
          dbSessionId={connCtx.dbSessionId}
          onCreated={handleUserCreated}
        />
      )}
    </>
  );
}

/** Builds the `loadBatchExportTableData` closure bound to the given context. */
function loadTableExportData(connCtx: ConnectionContext) {
  return (name: string) =>
    loadBatchExportTableData({
      dbSessionId: connCtx.dbSessionId,
      tableName: name,
      databaseType: connCtx.databaseType,
      includeRows: false,
    });
}
