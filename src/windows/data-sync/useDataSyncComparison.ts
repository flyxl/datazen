import { useCallback, useMemo } from 'react';
import { syncCommands, type DataSyncSourceFilter } from '../../commands/sync';
import { useI18n } from '../../hooks/useI18n';
import { buildCompareReportText } from './compareReport';
import type { DataSyncEndpoints } from './useDataSyncEndpoints';
import type { DataSyncFeedback } from './useDataSyncFeedback';
import type { DataSyncJobLifecycle } from './useDataSyncJobLifecycle';
import {
  applyOptionsToRows,
  markDisabledTables,
  mergeCompareIntoMappings,
  summarizeCompare,
  tableHasRowDiffs,
  tableKey,
  tablesForCompare,
} from './mappingView';
import type { DataSyncRunState } from './useDataSyncRunState';
import type { DataSyncSharedRefs } from './useDataSyncRefs';
import type { DataSyncSelectionState } from './useDataSyncSelectionState';
import type { DataSyncWizardState } from './dataSyncTypes';
import { isComparedSyncState } from './utils';

export interface UseDataSyncComparisonOptions {
  refs: DataSyncSharedRefs;
  feedback: DataSyncFeedback;
  run: DataSyncRunState;
  wizard: DataSyncWizardState;
  selection: DataSyncSelectionState;
  endpoints: DataSyncEndpoints;
  jobs: DataSyncJobLifecycle;
}

/**
 * Comparison phase of the Data Sync wizard: endpoint validation, the
 * inspection that discovers the table mapping and the comparison that
 * produces the row diffs.
 *
 * The comparison generation fence (`compareGenerationRef`) guarantees that
 * only the newest comparison may publish its result, even when an older
 * request resolves later.
 */
export function useDataSyncComparison({
  refs,
  feedback,
  run,
  wizard,
  selection,
  endpoints,
  jobs,
}: UseDataSyncComparisonOptions) {
  const { t } = useI18n();
  const { setErrorMsg, setErrorOpen, setStatusMsg, setExecuteProgress } = feedback;
  const { setLastExecutionResult, setWriteOutcomeUncertain, writeOutcomeUncertain } = run;
  const {
    syncOptions,
    mappingResults,
    setMappingResults,
    disabledTables,
    setDisabledTables,
    inspectionComplete,
    setInspectionComplete,
    syncState,
    setSyncState,
    setSelectedTableKey,
    setStep,
  } = wizard;
  const { setSelectedRows, setTableSelections, setPageIndex, setPageCursors } = selection;
  const {
    sourceId,
    targetId,
    sourceDatabase,
    targetDatabase,
    sourceSchema,
    targetSchema,
    activePairing,
    isSameEndpoint,
    refreshEndpointSessions,
  } = endpoints;
  const { refreshDataSyncJobs } = jobs;
  const { compareGenerationRef, pendingProfileMappingsRef, loadedPageRef } = refs;

  const validateEndpoints = useCallback((): boolean => {
    if (!sourceId || !targetId) {
      setErrorMsg(t('sync.selectBoth'));
      setErrorOpen(true);
      return false;
    }
    if (isSameEndpoint()) {
      setErrorMsg(t('sync.cannotSameDb'));
      setErrorOpen(true);
      return false;
    }
    if (!sourceDatabase || !targetDatabase) {
      setErrorMsg(t('sync.selectDbRequired'));
      setErrorOpen(true);
      return false;
    }
    if (!activePairing?.supported) {
      setErrorMsg(t('sync.useTransferHint'));
      setErrorOpen(true);
      return false;
    }
    return true;
  }, [sourceId, targetId, isSameEndpoint, sourceDatabase, targetDatabase, activePairing, t]);

  const handleInspect = useCallback(async (): Promise<boolean> => {
    if (!validateEndpoints()) return false;

    const generation = ++compareGenerationRef.current;
    setSyncState('inspecting');
    setSelectedTableKey(null);
    setStatusMsg('');
    setInspectionComplete(false);

    try {
      const { source, target } = await refreshEndpointSessions();
      if (generation !== compareGenerationRef.current) return false;
      const srcConnId = source?.dbSessionId;
      const tgtConnId = target?.dbSessionId;
      if (!srcConnId || !tgtConnId) {
        setSyncState(writeOutcomeUncertain ? 'unknown' : 'idle');
        return false;
      }

      const profileMappings = pendingProfileMappingsRef.current;
      const profileDisabled = profileMappings
        ? new Set(
            profileMappings
              .filter((mapping) => !mapping.enabled)
              .map((mapping) => mapping.sourceTable),
          )
        : disabledTables;

      const inspected = profileMappings
        ? await syncCommands.inspectDataSync(
            srcConnId,
            tgtConnId,
            sourceDatabase,
            targetDatabase,
            sourceSchema || undefined,
            targetSchema || undefined,
            profileMappings,
          )
        : await syncCommands.inspectDataSync(
            srcConnId,
            tgtConnId,
            sourceDatabase,
            targetDatabase,
            sourceSchema || undefined,
            targetSchema || undefined,
          );
      if (generation !== compareGenerationRef.current) return false;
      if (profileMappings) setDisabledTables(profileDisabled);
      const withDisabled = markDisabledTables(inspected, profileDisabled);
      setMappingResults(withDisabled);
      pendingProfileMappingsRef.current = null;
      setInspectionComplete(true);
      setSyncState('idle');
      return true;
    } catch (e) {
      if (generation !== compareGenerationRef.current) return false;
      setErrorMsg(e instanceof Error ? e.message : String(e));
      setErrorOpen(true);
      setSyncState(writeOutcomeUncertain ? 'unknown' : 'idle');
      return false;
    }
  }, [
    validateEndpoints,
    refreshEndpointSessions,
    sourceId,
    targetId,
    sourceDatabase,
    targetDatabase,
    sourceSchema,
    targetSchema,
    disabledTables,
    setDisabledTables,
    writeOutcomeUncertain,
  ]);

  const handleCompare = useCallback(async (): Promise<boolean> => {
    if (!validateEndpoints()) return false;
    if (!inspectionComplete) {
      const inspected = await handleInspect();
      if (!inspected) return false;
    }

    const generation = ++compareGenerationRef.current;
    setSyncState('comparing');
    setLastExecutionResult(null);
    setSelectedTableKey(null);
    setSelectedRows([]);
    setTableSelections([]);
    setPageIndex({});
    setPageCursors({});
    loadedPageRef.current.clear();
    setStatusMsg('');
    const jobId = crypto.randomUUID();
    refs.jobIdRef.current = jobId;
    refs.jobKindRef.current = 'compare';
    refs.cancelRequestedJobRef.current = null;

    try {
      const { source, target } = await refreshEndpointSessions();
      if (generation !== compareGenerationRef.current) return false;
      const srcConnId = source?.dbSessionId;
      const tgtConnId = target?.dbSessionId;
      if (!srcConnId || !tgtConnId) {
        setSyncState(writeOutcomeUncertain ? 'unknown' : 'idle');
        return false;
      }

      const toCompare = tablesForCompare(mappingResults);
      const filters = Object.fromEntries(
        mappingResults
          .filter((row) => row.status === 'MATCHED' && row.sourceFilter)
          .map((row) => [row.sourceTable, row.sourceFilter as DataSyncSourceFilter]),
      );
      const comparedResponse = Object.keys(filters).length
        ? await syncCommands.compareDataSync(
            srcConnId,
            tgtConnId,
            toCompare,
            jobId,
            sourceDatabase,
            targetDatabase,
            sourceSchema || undefined,
            targetSchema || undefined,
            syncOptions,
            filters,
          )
        : await syncCommands.compareDataSync(
            srcConnId,
            tgtConnId,
            toCompare,
            jobId,
            sourceDatabase,
            targetDatabase,
            sourceSchema || undefined,
            targetSchema || undefined,
            syncOptions,
          );

      refreshDataSyncJobs();

      if (generation !== compareGenerationRef.current) return false;
      const compared = Array.isArray(comparedResponse) ? comparedResponse : comparedResponse.tables;
      const pagedComparison =
        !Array.isArray(comparedResponse) && comparedResponse.contractVersion != null;
      if (refs.jobIdRef.current === jobId && refs.jobKindRef.current === 'compare') {
        refs.jobIdRef.current = null;
        refs.jobKindRef.current = null;
      }
      const merged = mergeCompareIntoMappings(mappingResults, compared).map((row) => {
        if (!pagedComparison && row.rows) {
          return { ...row, rows: applyOptionsToRows(row.rows, syncOptions) };
        }
        return { ...row, rows: undefined };
      });
      if (!pagedComparison) {
        setSelectedRows(
          merged.flatMap((table) =>
            (table.rows ?? [])
              .filter((row) => row.selected && row.operation !== 'UNCHANGED')
              .map((row) => ({
                sourceTable: table.sourceTable,
                targetTable: table.targetTable,
                operation: row.operation,
                key: row.key,
              })),
          ),
        );
      }
      setMappingResults(merged);
      setWriteOutcomeUncertain(false);
      const firstDiff = merged.find((r) => r.status === 'MATCHED' && tableHasRowDiffs(r));
      if (firstDiff) setSelectedTableKey(tableKey(firstDiff));
      setSyncState('compared');
      return true;
    } catch (e) {
      if (generation !== compareGenerationRef.current) return false;
      if (refs.jobIdRef.current === jobId && refs.jobKindRef.current === 'compare') {
        refs.jobIdRef.current = null;
        refs.jobKindRef.current = null;
      }
      setErrorMsg(e instanceof Error ? e.message : String(e));
      setErrorOpen(true);
      setSyncState(writeOutcomeUncertain ? 'unknown' : 'idle');
      return false;
    }
  }, [
    validateEndpoints,
    inspectionComplete,
    handleInspect,
    refreshEndpointSessions,
    refreshDataSyncJobs,
    mappingResults,
    sourceDatabase,
    targetDatabase,
    sourceSchema,
    targetSchema,
    syncOptions,
    writeOutcomeUncertain,
    setLastExecutionResult,
  ]);

  const handleCancel = useCallback(async () => {
    const cancellationGeneration = ++compareGenerationRef.current;
    // Reassert the fence before yielding so a comparison completion queued in the same tick cannot win.
    if (writeOutcomeUncertain) setWriteOutcomeUncertain(true);
    const jobId = refs.jobIdRef.current;
    const jobKind = refs.jobKindRef.current;
    const cancelledPhase = refs.syncStateRef.current;
    if (jobId) refs.cancelRequestedJobRef.current = jobId;

    if (jobKind === 'execute' && refs.writeInFlightRef.current) {
      refs.cancellingStatusJobRef.current = jobId;
      setStatusMsg(t('sync.cancellingExecution'));
    } else {
      setSyncState(
        writeOutcomeUncertain ? 'unknown' : mappingResults.length > 0 ? 'compared' : 'idle',
      );
      setExecuteProgress('');
      setStatusMsg(t('sync.compareCancelled'));
    }

    if (jobId) {
      await syncCommands.cancelDataSync(jobId);
    }
    if (
      cancellationGeneration !== compareGenerationRef.current ||
      refs.jobIdRef.current !== jobId ||
      refs.jobKindRef.current !== jobKind
    )
      return;
    if (jobKind === 'execute' && refs.syncStateRef.current !== cancelledPhase) return;
    if (jobKind === 'execute' && refs.writeInFlightRef.current) return;
    refs.jobIdRef.current = null;
    refs.jobKindRef.current = null;
    if (refs.cancelRequestedJobRef.current === jobId) refs.cancelRequestedJobRef.current = null;
  }, [mappingResults.length, t, writeOutcomeUncertain]);

  const toggleDisabledTable = useCallback((sourceTable: string) => {
    setSyncState('idle');
    setDisabledTables((prev) => {
      const next = new Set(prev);
      if (next.has(sourceTable)) next.delete(sourceTable);
      else next.add(sourceTable);
      return next;
    });
    setMappingResults((rows) =>
      rows.map((r) => {
        if (r.sourceTable !== sourceTable) return r;
        if (r.status === 'DISABLED') return { ...r, status: 'MATCHED' as const };
        if (r.status === 'MATCHED') return { ...r, status: 'DISABLED' as const, rows: undefined };
        return r;
      }),
    );
  }, []);

  const updateSourceFilter = useCallback(
    (sourceTable: string, sourceFilter: DataSyncSourceFilter | undefined) => {
      setSyncState('idle');
      setMappingResults((rows) =>
        rows.map((row) =>
          row.sourceTable === sourceTable ? { ...row, sourceFilter, rows: undefined } : row,
        ),
      );
    },
    [],
  );

  const compareStats = useMemo(() => summarizeCompare(mappingResults), [mappingResults]);

  const handleCopyCompareReport = useCallback(() => {
    const text = buildCompareReportText(mappingResults, compareStats);
    void navigator.clipboard.writeText(text);
  }, [mappingResults, compareStats]);

  const handleReCompare = useCallback(() => {
    setStep('compare');
    void handleCompare();
  }, [handleCompare]);

  return {
    validateEndpoints,
    handleInspect,
    handleCompare,
    handleCancel,
    toggleDisabledTable,
    updateSourceFilter,
    compareStats,
    handleCopyCompareReport,
    handleReCompare,
    /** The comparison produced a reviewable result (or is still running). */
    compared: isComparedSyncState(syncState),
  };
}

export type DataSyncComparison = ReturnType<typeof useDataSyncComparison>;
