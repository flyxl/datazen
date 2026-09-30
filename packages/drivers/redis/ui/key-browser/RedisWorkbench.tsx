import { forwardRef, useCallback, useEffect, useImperativeHandle, useMemo, useState } from 'react';
import { Database } from 'lucide-react';
import { useI18n } from '@datazen/ui';
import {
  useBoundSchemaStore,
  useBoundConnectionSchemaField,
  useBoundSettingsStore,
  readBooleanField,
} from '@datazen/driver-sdk';
import { ImportExport } from './ImportExport';
import { KeyTreePane } from './KeyTreePane';
import { DetailColumn } from './DetailColumn';
import { useRedisKeyScan } from './useRedisKeyScan';
import { useKeyTreeView } from './useKeyTreeView';
import { KeyWorkbenchDialogs } from './KeyWorkbenchDialogs';
import { mergeDatabases, dbIndexOfName } from './workbenchDatabases';
import { DbSidebar } from './DbSidebar';
import { BatchSummaryBanner } from './WorkbenchToolbar';
import { useWorkbenchSplit } from './useWorkbenchSplit';
import { useKeySelection } from './useKeySelection';
import { useKeyDetailState } from './useKeyDetailState';
import { useKeyRowActions } from './useKeyRowActions';
import { useKvSlotRelay } from './useKvSlotRelay';
import { useWorkbenchSearch } from './useWorkbenchSearch';
import { useDbKeyCounts } from './useDbKeyCounts';
import { useCreateTypes, useReJsonModules } from './useReJsonModules';
import { useWorkbenchOverlays } from './useWorkbenchOverlays';
import { useKeyJump } from './useKeyJump';
import { isDraftDirty, requestDraftLeave } from '../shared/draftGuard';

export type { RedisWorkbenchProps, RedisWorkbenchHandle } from './workbenchTypes';
import type { RedisWorkbenchProps, RedisWorkbenchHandle } from './workbenchTypes';

/**
 * 屏 B of the Redis workbench: database picker + key tree (left) + key detail
 * (right).
 *
 * This file is deliberately a *composition*. After the D-0 split every block of
 * the previous 705-line wall owns a module (`DbSidebar`, `WorkbenchToolbar`,
 * `KeyTreePane` — the whole left column, R1/R2/R3 header plus the list —,
 * `DetailColumn`, `useRedisKeyScan`/`useKeyTreeView`, the `useKey*` state hooks,
 * `useWorkbenchOverlays` and `KeyWorkbenchDialogs`). What stays here is only what
 * genuinely has to be shared: the single writers of selection and detail state,
 * the refresh fan-out (including the I-8 "refresh without dropping the selection"
 * variant), the host KV relay (contract F-2) and the imperative handle the host tabs drive.
 *
 * It also owns the **I-1 draft gate** (PRD §4) for every navigation action except
 * the two selection moves: 切db / 刷新 / 搜索 / 关面板 / 对话框出口 ask before they
 * run (the guard is module-level, so it answers "already asked?" itself). The 切键
 * ask and BUG-001's in-place rule live in `useKeyDetailState`.
 */
export const RedisWorkbench = forwardRef<RedisWorkbenchHandle, RedisWorkbenchProps>(
  function RedisWorkbench(
    {
      dbSessionId,
      connectionId,
      initialDatabase,
      hideSidebar,
      onDbIndexChange,
      onDatabaseChange,
      onKeysChange,
      kvSlotState,
      pendingAction,
      renderRightPanel,
    },
    ref,
  ) {
    const { t } = useI18n();
    const databasesFromStore = useBoundConnectionSchemaField(dbSessionId, 'databases');
    const loading = useBoundConnectionSchemaField(dbSessionId, 'loading');
    const loadForConnection = useBoundSchemaStore((s) => s.loadForConnection);
    const driverSettings = useBoundSettingsStore((s) => s.settings.driverSettings);
    const allowFlush = readBooleanField(
      (driverSettings?.redis ?? {}) as Record<string, unknown>,
      'allowFlush',
      false,
    );

    const databases = useMemo(() => mergeDatabases(databasesFromStore), [databasesFromStore]);

    const [selectedDb, setSelectedDb] = useState<string | null>(null);
    const [dbIndex, setDbIndex] = useState(0);
    // R2 chip: wrap a literal input in `*…*` when applying (see toScanPattern).
    const [fuzzyPattern, setFuzzyPattern] = useState(false);

    const overlays = useWorkbenchOverlays();
    const { treeWidth, startSplitDrag } = useWorkbenchSplit();
    const selection = useKeySelection();
    const detail = useKeyDetailState(dbSessionId, dbIndex);

    // Flat `scan_keys` list as one object (`scan.*`): 屏 B reads a dozen fields.
    const scan = useRedisKeyScan({
      dbSessionId,
      dbIndex,
      enabled: selectedDb !== null,
    });

    /*
     * D-3 + D-8 in one owner: the R3 separator reaches both the `list_children`
     * request and the row fold, and the four named empty states (I-11) derive from
     * the same rows. BUG-001: the pattern joins them as the **applied** one
     * (`scan.appliedPattern`), never the merely typed `searchPattern`.
     */
    const treeView = useKeyTreeView({
      connectionId,
      dbSessionId,
      dbIndex,
      enabled: selectedDb !== null,
      noTtlOnly: scan.noTtlOnly,
      keyType: scan.keyTypeFilter,
      loadedKeys: scan.keys,
      pattern: scan.searchPattern,
      appliedPattern: scan.appliedPattern,
      loading: scan.keysLoading,
      scanOpen: scan.cursor !== 0,
    });

    useEffect(() => {
      void loadForConnection(dbSessionId, { skipLoadTables: true });
    }, [dbSessionId, loadForConnection]);

    // Stable identities: `scan`/`treeView` are fresh every render (see deps below).
    const tree = treeView.tree;
    const { setSearchPattern, resetSelectionState, loadKeys, refresh: scanRefresh } = scan;

    const modules = useReJsonModules(dbSessionId);
    const { dbCounts, loadDbSizes } = useDbKeyCounts(dbSessionId, dbIndex, selectedDb, scan.dbSize);
    const createTypes = useCreateTypes(modules);

    /** Drop the mounted detail *and* the checkbox selection (db switch, refresh). */
    const clearFocus = useCallback(() => {
      detail.clearDetail();
      selection.clearSelection();
    }, [detail.clearDetail, selection.clearSelection]);

    // I-1 for search sits inside `applySearch`, so no ask is needed at this site.
    const search = useWorkbenchSearch({
      dbSessionId,
      dbIndex,
      clearFocus,
      runKeySearch: scan.search,
    });

    const handleSelectDb = useCallback(
      async (db: string) => {
        const idx = dbIndexOfName(db);
        // Same-db re-click (incl. the initial auto-select) is not a 切db: no dialog.
        if (selectedDb === db && dbIndex === idx) return;
        // I-1: switching databases drops the selection, i.e. the live draft.
        if (!(await requestDraftLeave())) return;
        setSelectedDb(db);
        setDbIndex(idx);
        onDbIndexChange?.(idx);
        onDatabaseChange?.(db);
        clearFocus();
        setSearchPattern('*');
        resetSelectionState();
        void loadKeys(idx, '*', 0, true);
      },
      [
        selectedDb,
        dbIndex,
        clearFocus,
        setSearchPattern,
        resetSelectionState,
        loadKeys,
        onDatabaseChange,
        onDbIndexChange,
      ],
    );

    useEffect(() => {
      onKeysChange?.(scan.keys.map((entry) => entry.key));
    }, [scan.keys, onKeysChange]);

    useEffect(() => {
      if (databases.length > 0 && !selectedDb) {
        const initial = initialDatabase
          ? (databases.find((d) => d === initialDatabase) ?? initialDatabase)
          : databases[0];
        if (initial) void handleSelectDb(initial);
      }
    }, [databases, initialDatabase, selectedDb, handleSelectDb]);

    const refreshKeys = useCallback(async () => {
      if (!selectedDb) return;
      // I-1: this body drops the selection (the draft with it) — ask first.
      if (!(await requestDraftLeave())) return;
      clearFocus();
      scanRefresh();
      tree.refresh();
    }, [selectedDb, clearFocus, scanRefresh, tree.refresh]);

    const handleRefresh = useCallback(async () => {
      // I-1: refuse ⇒ nothing reloads. `refreshKeys` has its own I-1 guard too.
      if (!(await requestDraftLeave())) return;
      void loadForConnection(dbSessionId, { skipLoadTables: true });
      void refreshKeys();
      void loadDbSizes();
    }, [dbSessionId, loadForConnection, refreshKeys, loadDbSizes]);

    // 屏 A → 屏 B jump bridge: key selection + dialog pending actions.
    const { selectKey } = useKeyJump({
      dbIndex,
      selectedDb,
      handleSelectDb,
      detail,
      scanKeys: scan.keys,
      scanKeysLoading: scan.keysLoading,
      pendingAction,
      overlays,
    });

    useImperativeHandle(ref, () => ({ refreshKeys, selectDatabase: handleSelectDb, selectKey }), [
      refreshKeys,
      handleSelectDb,
      selectKey,
    ]);

    // E-5 fix: refetch DETAIL only — `refreshKeys()` would clear selection.
    const reloadDetail = useCallback(async () => {
      if (!detail.selectedKey) return;
      await detail.selectKey(detail.selectedKey);
      scanRefresh();
      tree.refresh();
    }, [detail.selectedKey, detail.selectKey, scanRefresh, tree.refresh]);

    // Contract F-2: publish selection + draft flag into the host's KV atom.
    useKvSlotRelay(kvSlotState, dbSessionId, detail.selectedKey, detail.editorDirty);

    // Row context menu + hover delete (confirm → write gate → refresh).
    const { handleKeyContextMenu, handleDeleteRow, handleDeleteSelected, actionDialogs } =
      useKeyRowActions({
        dbSessionId,
        dbIndex,
        onRefreshKeys: refreshKeys,
        onBatchSummary: overlays.setBatchSummary,
        onKeyCtxDialog: overlays.setKeyCtxDialog,
      });

    // Dialog-side refresh (BUG-002): dirty ⇒ rescan only; clean ⇒ full refresh.
    const refreshKeysForDialogs = useCallback(() => {
      if (isDraftDirty()) {
        scanRefresh();
        tree.refresh();
        return;
      }
      void refreshKeys();
    }, [scanRefresh, tree.refresh, refreshKeys]);

    return (
      <div className="flex min-h-0 flex-1">
        {!hideSidebar && (
          <DbSidebar
            loading={loading}
            databases={databases}
            selectedDb={selectedDb}
            dbCounts={dbCounts}
            onSelectDb={handleSelectDb}
          />
        )}

        <div className="flex min-h-0 min-w-0 flex-1 flex-col">
          {selectedDb ? (
            <>
              <BatchSummaryBanner
                summary={overlays.batchSummary}
                onDismiss={() => overlays.setBatchSummary(null)}
              />

              <div className="flex min-h-0 flex-1">
                <div
                  className="flex min-w-0 shrink-0 flex-col"
                  style={{ width: treeWidth }}
                  data-testid="redis-tree-pane"
                  data-tree-width={treeWidth}
                >
                  <KeyTreePane
                    view={treeView}
                    scan={scan}
                    search={search}
                    selection={selection}
                    detail={detail}
                    onKeyContextMenu={handleKeyContextMenu}
                    onDeleteRow={handleDeleteRow}
                    onDeleteSelected={handleDeleteSelected}
                    totalCount={scan.dbSize}
                    fuzzy={fuzzyPattern}
                    onFuzzyChange={setFuzzyPattern}
                    onCreateKey={() => overlays.setCreateOpen(true)}
                    onRefresh={handleRefresh}
                  />
                </div>

                <div
                  role="separator"
                  aria-orientation="vertical"
                  onPointerDown={startSplitDrag}
                  className="w-1 shrink-0 cursor-col-resize bg-edge transition-colors hover:bg-accent/60"
                  data-testid="redis-split-handle"
                />

                <div className="flex min-w-0 flex-1 flex-col border-l border-edge">
                  {renderRightPanel ? (
                    renderRightPanel({
                      dbSessionId,
                      dbIndex,
                      selectedKey: detail.selectedKey,
                      detail: detail.keyDetail,
                      detailLoading: detail.detailLoading,
                      modules,
                      onRefresh: reloadDetail,
                      onRenamed: (newKey) => {
                        detail.retargetKey(newKey);
                        void refreshKeys();
                      },
                      onDirtyChange: detail.setEditorDirty,
                    })
                  ) : (
                    <DetailColumn
                      dbSessionId={dbSessionId}
                      dbIndex={dbIndex}
                      selectedKey={detail.selectedKey}
                      detail={detail.keyDetail}
                      detailLoading={detail.detailLoading}
                      modules={modules}
                      onRefresh={reloadDetail}
                      onRenamed={(newKey) => {
                        detail.retargetKey(newKey);
                        // Draft already settled upstream (rename passes the I-1
                        // guard before it runs), so this refresh sails through.
                        void refreshKeys();
                      }}
                      onDirtyChange={detail.setEditorDirty}
                    />
                  )}
                </div>
              </div>
            </>
          ) : (
            <div className="flex flex-1 items-center justify-center text-fg-muted">
              <div className="text-center" data-testid="redis-select-db-state">
                <Database className="mx-auto h-10 w-10 opacity-20" />
                <div className="mt-3 text-sm">{t('redis.selectDb')}</div>
              </div>
            </div>
          )}
        </div>

        <ImportExport
          dbSessionId={dbSessionId}
          dbIndex={dbIndex}
          selectedKeys={[...selection.selectedKeys]}
          searchPattern={scan.searchPattern}
          open={overlays.importExportOpen}
          onOpenChange={overlays.setImportExportOpen}
          onRefresh={refreshKeys}
          onSummary={overlays.setBatchSummary}
        />

        <KeyWorkbenchDialogs
          dbSessionId={dbSessionId}
          dbIndex={dbIndex}
          allowFlush={allowFlush}
          createTypes={createTypes}
          selectedKey={detail.selectedKey}
          onRefreshKeys={refreshKeysForDialogs}
          onSelectKey={detail.selectKey}
          onClearSelectedKey={detail.clearDetailGuarded}
          onUpdateSelectedKeys={selection.update}
          onBatchSummary={overlays.setBatchSummary}
          createOpen={overlays.createOpen}
          onCreateOpenChange={overlays.setCreateOpen}
          flushDialog={overlays.flushDialog}
          onFlushDialogChange={overlays.setFlushDialog}
          keyCtxDialog={overlays.keyCtxDialog}
          onKeyCtxDialogChange={overlays.setKeyCtxDialog}
        />

        {actionDialogs}
      </div>
    );
  },
);
