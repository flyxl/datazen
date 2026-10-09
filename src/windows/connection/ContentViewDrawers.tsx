import { useCallback, useMemo } from 'react';
import { useResizable } from '../../hooks/useResizable';
import { usePanelStore, type Panel } from '../../stores/panelStore';
import { useTableDataStore } from '../../stores/tableDataStore';
import { DB_REGISTRY } from '../../lib/databaseTypes';
import { rowToRecord } from '../../lib/rowToRecord';
import type { ColumnDef } from '../../components/DataTable/TableHeader';
import { DetailPanel } from '../../components/DataTable/DetailPanel';
import { AiChatPanel } from '../../components/ai/AiChatPanel';
import { buildKvAiContext } from '../../lib/kvAiContext';
import { useKvSlotSelectedKey } from '../../hooks/useKvSlotSelectedKey';
import type { ColumnSchema, DatabaseType } from '../../types';
import type { KvSlotState } from '@datazen/driver-sdk';
import type { AiChatDraftRequest } from './query/aiDraftBridge';
import type { KvKeyPropsSidebarBinding } from './useKvWorkspaceSlots';

const NO_COLUMNS: ColumnSchema[] = [];
const NO_ROWS: Record<string, unknown>[] = [];
const NO_SELECTED: Set<number> = new Set();

export interface ContentViewDrawersProps {
  activePanel: Panel | null;
  detailOpen: boolean;
  aiChatOpen: boolean;
  detailPanelApplicable: boolean;
  dbSessionId: string;
  connectionName: string;
  currentDatabase: string | null;
  databaseType: DatabaseType | undefined;
  /**
   * Relay of the active KV panel (`undefined` on a relational panel). Read here
   * only through a leaf subscription, to build the assistant's KV context;
   * the drawer itself keeps rendering the same way either way.
   */
  kvPanelState?: KvSlotState;
  /** Collapse request wired to the driver's own close control. */
  onCloseDetail: () => void;
  /**
   * Driver-contributed key-props sidebar for a KV panel's detail drawer. Present ⇒
   * the drawer renders it instead of the row-detail table, which on a KV panel was
   * an empty grid behind a working-looking toggle. Absent ⇒ the row table,
   * unchanged.
   */
  keyPropsSidebarSlot?: KvKeyPropsSidebarBinding;
  /** Pending AI draft request from ContentView coordinator. */
  pendingDraftRequest: AiChatDraftRequest | null;
  /** Called after AiChatPanel writes the draft to its textarea. */
  onDraftConsumed: (requestId: string) => void;
}

/**
 * Right-hand drawers for the connection workspace: the data-detail drawer
 * (`detailRow`) and the AI assistant sidebar (`aiChatOpen`), including the
 * resizable split handle. Pure render + state extraction from `ContentView` —
 * no behaviour change; the open state stays owned by `ContentView`.
 */
export function ContentViewDrawers({
  activePanel,
  detailOpen,
  aiChatOpen,
  detailPanelApplicable,
  dbSessionId,
  connectionName,
  currentDatabase,
  databaseType,
  kvPanelState,
  onCloseDetail,
  keyPropsSidebarSlot,
  pendingDraftRequest,
  onDraftConsumed,
}: ContentViewDrawersProps) {
  const { size: aiSidebarWidth, handleRef: aiHandleRef } = useResizable({
    direction: 'horizontal',
    initialSize: 320,
    minSize: 240,
    maxSize: 600,
    reverse: true,
    storageKey: 'connection.aiSidebar',
  });

  // The assistant gets whatever the host really owns about this panel.
  // Subscribing to the one scalar here — instead of the workspace passing a key
  // down — keeps a key-tree selection from re-rendering the whole content column.
  const kvSelectedKey = useKvSlotSelectedKey(kvPanelState);
  const kvAiContext = useMemo(
    () =>
      buildKvAiContext({
        connectionName,
        dbSessionId,
        database: currentDatabase,
        selectedKey: kvSelectedKey,
      }),
    [connectionName, dbSessionId, currentDatabase, kvSelectedKey],
  );
  const detailPanelId =
    activePanel && (activePanel.type === 'table' || activePanel.type === 'view')
      ? activePanel.id
      : null;
  const tableSlice = useTableDataStore((s) =>
    detailPanelId ? s.byPanel.get(detailPanelId) : undefined,
  );
  const tableColumns = tableSlice?.columns ?? NO_COLUMNS;
  const tableRows = tableSlice?.rows ?? NO_ROWS;
  const selectedRows = tableSlice?.selectedRows ?? NO_SELECTED;
  const detailRowIndex = tableSlice?.detailRowIndex ?? null;

  const activeQueryExec = usePanelStore((s) =>
    activePanel?.type === 'query' ? s.queryExec.get(activePanel.id) : undefined,
  );
  const updateResultCell = usePanelStore((s) => s.updateResultCell);
  const updateQuerySql = usePanelStore((s) => s.updateSql);

  const activeQueryResult =
    activeQueryExec && activeQueryExec.results.length > 0
      ? (activeQueryExec.results[activeQueryExec.activeResultIdx] ?? null)
      : null;
  const resultDetailRowIndex = activeQueryExec?.resultDetailRowIndex ?? null;

  const detailColumnDefs: ColumnDef[] = useMemo(() => {
    if (activePanel?.type === 'table') {
      return tableColumns.map((c) => ({ id: c.name, name: c.name, type: c.dataType }));
    }
    if (activeQueryResult) {
      return activeQueryResult.columns.map((c) => ({
        id: c.name,
        name: c.name,
        type: c.dataType,
      }));
    }
    return [];
  }, [activePanel?.type, tableColumns, activeQueryResult]);

  const detailRowIdx = activePanel?.type === 'table' ? detailRowIndex : resultDetailRowIndex;

  const detailRow: Record<string, unknown> | null = useMemo(() => {
    if (activePanel?.type === 'table') {
      return detailRowIndex !== null && detailRowIndex < tableRows.length
        ? tableRows[detailRowIndex]
        : null;
    }
    if (
      activeQueryResult &&
      resultDetailRowIndex !== null &&
      resultDetailRowIndex < activeQueryResult.rows.length
    ) {
      return rowToRecord(activeQueryResult.rows[resultDetailRowIndex], activeQueryResult.columns);
    }
    return null;
  }, [activePanel?.type, detailRowIndex, tableRows, activeQueryResult, resultDetailRowIndex]);

  const handleDetailFieldEdit = useCallback(
    (row: number, col: string, value: unknown) => {
      const store = useTableDataStore.getState();
      if (activePanel?.type === 'table') {
        if (selectedRows.size > 1) {
          store.applyColumnToRows(activePanel.id, col, value, [...selectedRows]);
        } else {
          store.stageCellChange(activePanel.id, row, col, value);
        }
      } else if (activePanel?.type === 'query' && activeQueryExec) {
        updateResultCell(activePanel.id, activeQueryExec.activeResultIdx, row, col, value);
      }
    },
    [activePanel, activeQueryExec, updateResultCell, selectedRows],
  );

  const Sidebar = keyPropsSidebarSlot?.Component;

  return (
    <>
      {detailPanelApplicable &&
        (Sidebar && keyPropsSidebarSlot ? (
          // KV panel with a driver-contributed key-props sidebar: the same drawer
          // slot now carries real content instead of an empty row grid.
          // The sidebar owns its own container (width / border / scroll) just like
          // DetailPanel does, and is expected to render nothing while `open` is false.
          <div
            data-slot="kv-key-props-sidebar"
            data-testid="conn-kv-key-props-sidebar"
            className="flex min-w-0 shrink-0"
          >
            <Sidebar {...keyPropsSidebarSlot.props} open={detailOpen} onClose={onCloseDetail} />
          </div>
        ) : (
          <DetailPanel
            open={detailOpen}
            columns={detailColumnDefs}
            row={detailRow}
            rowIndex={detailRowIdx}
            selectedRows={
              activePanel?.type === 'table' || activePanel?.type === 'view'
                ? selectedRows
                : undefined
            }
            editable
            onFieldEdit={handleDetailFieldEdit}
          />
        ))}

      {aiChatOpen && dbSessionId && (
        <>
          <div
            ref={aiHandleRef}
            className="w-1 shrink-0 cursor-col-resize bg-transparent hover:bg-accent/30"
          />
          <aside
            style={{ width: aiSidebarWidth }}
            className="shrink-0 border-l border-edge bg-surface"
          >
            <AiChatPanel
              dbSessionId={dbSessionId}
              database={currentDatabase ?? undefined}
              sqlDialect={databaseType ? DB_REGISTRY[databaseType]?.sqlDialect : undefined}
              onInsertSql={(sql) => {
                if (activePanel?.type === 'query') {
                  updateQuerySql(activePanel.id, sql);
                }
              }}
              kvContext={kvAiContext}
              draftRequest={pendingDraftRequest}
              onDraftConsumed={onDraftConsumed}
            />
          </aside>
        </>
      )}
    </>
  );
}
