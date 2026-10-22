import { useCallback, useMemo } from 'react';
import { useResizable } from '../../hooks/useResizable';
import { useI18n } from '../../hooks/useI18n';
import { usePanelStore, type Panel } from '../../stores/panelStore';
import { resultEditKey, useResultEditability } from '../../stores/panelResultEdit';
import {
  gridReadOnlyMessageKey,
  isColumnEditable,
  originChannelWritable,
  resolveResultEditability,
  resolveResultWriteBack,
  resolveRowAddressability,
  type ServerEditability,
} from '../../lib/resultEditability';
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

/**
 * The verdict of a result nobody has judged yet.
 *
 * "Not judged" is read-only: a permission that has not been granted is not a
 * permission. The reason is "the origin is unknown", which is what the user
 * is told until the back end answers.
 */
const UNJUDGED: ServerEditability = {
  writable: false,
  identityColumns: [],
  editableColumns: [],
  table: null,
  reason: 'unknownOrigin',
};

export interface ContentViewDrawersProps {
  activePanel: Panel | null;
  detailOpen: boolean;
  aiChatOpen: boolean;
  detailPanelApplicable: boolean;
  dbSessionId: string;
  /** Persisted connection id; required to grant a shared editor session. */
  connectionId?: string;
  /** Stable owner id (pane key) of the active query editor, when there is one. */
  editorSessionId?: string;
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
  connectionId,
  editorSessionId,
  connectionName,
  currentDatabase,
  databaseType,
  kvPanelState,
  onCloseDetail,
  keyPropsSidebarSlot,
  pendingDraftRequest,
  onDraftConsumed,
}: ContentViewDrawersProps) {
  const { t } = useI18n();
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

  // The write-back verdict of the result on screen. It is fetched per result
  // and dropped again when the result is replaced, so a verdict never outlives
  // the rows it was made for.
  const queryResultKey =
    activePanel?.type === 'query' && activeQueryExec
      ? resultEditKey(activePanel.dbSessionId, activeQueryExec.activeResultIdx)
      : null;
  const { verdict: originVerdict, resolving: resolvingOrigin } = useResultEditability({
    key: queryResultKey,
    dbSessionId: activePanel?.type === 'query' ? activePanel.dbSessionId : '',
    database: activePanel?.type === 'query' ? activePanel.database || null : null,
    schema: activePanel?.type === 'query' ? activePanel.schema ?? null : null,
    sql: activeQueryResult?.sql ?? '',
    resultColumns: activeQueryResult?.columns.map((column) => column.name) ?? [],
    table: activeQueryResult?.source?.relation?.object?.name ?? null,
  });

  /**
   * Whether the result on screen may be edited at all.
   *
   * Two verdicts answer that: the narrow mapping verdict that shipped first
   * and the driver's own origin verdict. Either one is enough — see
   * `resolveResultWriteBack`.
   */
  const queryResultEditable = useMemo(() => {
    if (activePanel?.type !== 'query') return true;
    if (!activeQueryResult) return false;
    return resolveResultWriteBack(
      activeQueryResult.source?.writableMapping === 'verified',
      originVerdict ?? UNJUDGED,
    );
  }, [activePanel?.type, activeQueryResult, originVerdict]);

  /** Why the result on screen cannot be edited, when it cannot. */
  const queryReadOnlyReason = useMemo(() => {
    if (activePanel?.type !== 'query') return null;
    if (!queryResultEditable) return originVerdict?.reason ?? 'unknownOrigin';
    // The result is writable, but the row on screen may still not be
    // addressable: its locating key is NULL, or its identity is not unique on
    // the page. That is the drawer on that row, so it is where the reason
    // belongs.
    if (!activeQueryResult || resultDetailRowIndex === null) return null;
    return resolveRowAddressability(
      originVerdict?.identityColumns ?? [],
      activeQueryResult.rows,
      activeQueryResult.columns,
      resultDetailRowIndex,
    );
  }, [
    activePanel?.type,
    activeQueryResult,
    originVerdict,
    queryResultEditable,
    resultDetailRowIndex,
  ]);

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
      } else if (activePanel?.type === 'query' && activeQueryExec && activeQueryResult) {
        // The page-level guards are decided for **the row being edited**: a
        // NULL key value or a duplicate identity makes that row
        // unaddressable, whatever the rest of the page looks like.
        const editability = resolveResultEditability(
          activeQueryResult.source?.writableMapping === 'verified',
          originVerdict ?? UNJUDGED,
          activeQueryResult.rows,
          activeQueryResult.columns,
          row,
        );
        // A read-only result produces **no write intent at all**: the write
        // path is not entered, so nothing is staged and nothing is sent.
        if (!editability.writable) return;
        // A column that is not a straight copy of a base column has no source
        // to write back to — the back end would refuse it with
        // `grid.result.writeDenied`.
        //
        // §1.3 §5 has **no executor on the narrow channel today**: the origin
        // channel is closed, so no column list exists to check against. The
        // guard is in place and starts to bite the moment the channel opens.
        if (
          originChannelWritable(originVerdict ?? UNJUDGED) &&
          !isColumnEditable(originVerdict ?? UNJUDGED, col)
        ) {
          return;
        }
        void updateResultCell(
          activePanel.id,
          activeQueryExec.activeResultIdx,
          row,
          col,
          value,
        ).catch(() => undefined);
      }
    },
    [
      activePanel,
      activeQueryExec,
      activeQueryResult,
      originVerdict,
      updateResultCell,
      selectedRows,
    ],
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
          <>
            {queryReadOnlyReason && (
              <div
                data-dt-grid-readonly-reason={queryReadOnlyReason}
                className="shrink-0 border-b border-edge bg-surface-alt px-3 py-1.5 text-xs text-fg-muted"
              >
                {t(
                  resolvingOrigin
                    ? 'query.gridEdit.resolving'
                    : gridReadOnlyMessageKey(queryReadOnlyReason),
                )}
              </div>
            )}
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
              editable={
                activePanel?.type !== 'query' ||
                (queryResultEditable && !!activeQueryResult?.runtimeBinding)
              }
              // The drawer shows the same columns as the grid next to it, so it
              // reads the same driver declaration (05 §3.1).
              cellTypeHints={databaseType ? DB_REGISTRY[databaseType]?.cellTypes : undefined}
              onFieldEdit={handleDetailFieldEdit}
            />
          </>
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
              connectionId={connectionId}
              editorSessionId={editorSessionId}
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
