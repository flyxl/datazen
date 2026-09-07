import { useMemo } from 'react';
import { useConnectionStore } from '../../stores/connectionStore';
import { useActiveConnectionStore } from '../../stores/activeConnectionStore';
import { useSchemaStore } from '../../stores/schemaStore';
import { usePanelStore, type ConnectionContext, type Panel } from '../../stores/panelStore';
import { DB_REGISTRY } from '../../lib/databaseTypes';
import { canOpenStructureEditor } from '../../lib/structureEditor/canOpenStructureEditor';
import { resolveExportScope, supportsFullTableExport } from '../../lib/exportCapability';
import { resolveConnectionContext } from './contentViewHelpers';
import type { DatabaseType } from '../../types';

export interface ConnectionWorkspaceMeta {
  connCtx: ConnectionContext | null;
  sidebarConnCtx: ConnectionContext | null;
  initialDatabase: string | undefined;
  databaseType: DatabaseType | undefined;
  dbSessionId: string;
  connectionId: string;
  connectionName: string;
  hasSavedConnections: boolean;
  toolbarDbType: DatabaseType | undefined;
  showStructureEditor: boolean;
  exportScope: ReturnType<typeof resolveExportScope>;
  batchExportSupported: boolean;
  isKvPanel: boolean;
  showNewQuery: boolean;
  showNewTable: boolean;
  showErDiagramToolbar: boolean;
  showObjectsToolbar: boolean;
  connectingEntry: { status: string; connectionId: string; dbSessionId?: string } | null;
  connectingName: string | undefined;
  connectingDbType: DatabaseType | undefined;
  recentPanels: Panel[];
  statusDatabase: string | null;
}

interface ConnectingEntryShape {
  status: string;
  connectionId: string;
}

/**
 * Derives the connection-workspace metadata used by `ContentView` and its
 * drawers/dialogs. Pure relocation of ContentView's previous inline memo/derived
 * state — same inputs, same outputs, no behaviour change.
 */
export function useConnectionWorkspaceMeta(activePanel: Panel | null): ConnectionWorkspaceMeta {
  const savedConnections = useConnectionStore((s) => s.connections);
  const activeConnections = useActiveConnectionStore((s) => s.connections);
  const storeActiveDbSessionId = useSchemaStore((s) => s.activeDbSessionId);
  const currentDatabase = useSchemaStore((s) => s.currentDatabase);
  const allPanels = usePanelStore((s) => s.panels);

  const databaseType = activePanel?.databaseType as DatabaseType | undefined;
  const dbSessionId = activePanel?.dbSessionId ?? '';
  const connectionId = activePanel?.connectionId ?? '';
  const connectionName = activePanel?.connectionName ?? '';

  const connCtx: ConnectionContext | null = useMemo(() => {
    if (!activePanel) return null;
    return {
      connectionId: activePanel.connectionId,
      dbSessionId: activePanel.dbSessionId,
      connectionName: activePanel.connectionName,
      databaseType: activePanel.databaseType,
    };
  }, [
    activePanel?.connectionId,
    activePanel?.dbSessionId,
    activePanel?.connectionName,
    activePanel?.databaseType,
  ]);

  const sidebarConnCtx = useMemo(() => {
    if (!storeActiveDbSessionId) return connCtx;
    return (
      resolveConnectionContext(storeActiveDbSessionId, activeConnections, savedConnections) ??
      connCtx
    );
  }, [storeActiveDbSessionId, activeConnections, savedConnections, connCtx]);

  const initialDatabase = useMemo(() => {
    const ctxConnectionId = sidebarConnCtx?.connectionId ?? connectionId;
    if (!ctxConnectionId) return undefined;
    return savedConnections.find((c) => c.id === ctxConnectionId)?.database;
  }, [savedConnections, sidebarConnCtx?.connectionId, connectionId]);

  const toolbarDbType = databaseType ?? (sidebarConnCtx?.databaseType as DatabaseType | undefined);
  const dbMeta = databaseType ? DB_REGISTRY[databaseType] : undefined;
  const toolbarDbMeta = toolbarDbType ? DB_REGISTRY[toolbarDbType] : undefined;
  const showStructureEditor = canOpenStructureEditor(dbMeta) && dbMeta?.readOnly !== true;
  const exportScope = resolveExportScope(dbMeta);
  const toolbarExportScope = resolveExportScope(toolbarDbMeta);
  const batchExportSupported = supportsFullTableExport(toolbarExportScope);

  // Guard: key-value / document panels (Redis, future MongoDB KV) hide SQL-oriented toolbar items.
  // Uses DB_REGISTRY metadata instead of panel type literal so new KV drivers get the same behaviour.
  const isKvPanel = activePanel?.type === 'redis-db' || toolbarDbMeta?.isKeyValue === true;
  const showNewQuery = !isKvPanel && toolbarDbMeta?.supportsSQL !== false && !!toolbarDbType;
  const showNewTable =
    !isKvPanel &&
    canOpenStructureEditor(toolbarDbMeta) &&
    toolbarDbMeta?.readOnly !== true &&
    !!toolbarDbType;
  const showErDiagramToolbar =
    !isKvPanel && toolbarDbMeta?.supportsErDiagram !== false && !!toolbarDbType;
  const showObjectsToolbar = !isKvPanel && toolbarDbMeta?.readOnly !== true && !!toolbarDbType;

  const connectingEntry = useMemo(() => {
    if (activePanel) return null;
    const entries = Object.values(activeConnections);
    return (entries.find((e) => e.status === 'connecting') ?? null) as ConnectingEntryShape | null;
  }, [activePanel, activeConnections]);

  const connectingName = useMemo(() => {
    if (!connectingEntry) return undefined;
    return savedConnections.find((c) => c.id === connectingEntry.connectionId)?.name;
  }, [connectingEntry, savedConnections]);

  const connectingDbType = useMemo(() => {
    if (!connectingEntry) return undefined;
    return savedConnections.find((c) => c.id === connectingEntry.connectionId)?.databaseType as
      | DatabaseType
      | undefined;
  }, [connectingEntry, savedConnections]);

  const recentPanels = useMemo(() => {
    if (!sidebarConnCtx) return [];
    return allPanels
      .filter((panel) => panel.connectionId === sidebarConnCtx.connectionId)
      .slice(-6)
      .reverse();
  }, [allPanels, sidebarConnCtx]);

  // KV / document panels own their logical database context. The schema store tracks
  // the outer SQL navigator and may still point at db0 after a KV panel was
  // opened on db5, so the status bar must use the panel's immutable target.
  const statusDatabase =
    dbMeta?.isKeyValue === true && activePanel
      ? ((activePanel as { dbName?: string }).dbName ?? currentDatabase)
      : currentDatabase;

  return {
    connCtx,
    sidebarConnCtx,
    initialDatabase,
    databaseType,
    dbSessionId,
    connectionId,
    connectionName,
    hasSavedConnections: savedConnections.length > 0,
    toolbarDbType,
    showStructureEditor,
    exportScope,
    batchExportSupported,
    isKvPanel,
    showNewQuery,
    showNewTable,
    showErDiagramToolbar,
    showObjectsToolbar,
    connectingEntry,
    connectingName,
    connectingDbType,
    recentPanels,
    statusDatabase,
  };
}
