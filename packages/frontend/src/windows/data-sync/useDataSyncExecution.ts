import { useCallback, useState } from 'react';
import {
  syncCommands,
  type DataSyncSourceFilter,
  type DataSyncTableResult,
} from '../../commands/sync';
import { useI18n } from '../../hooks/useI18n';
import { isStalePlanError } from '../../lib/migrationJobHydration';
import type { DataSyncWizardState } from './dataSyncTypes';
import { applyOptionsToRows, mergeCompareIntoMappings, tableHasRowDiffs } from './mappingView';
import type { DataSyncEndpoints } from './useDataSyncEndpoints';
import type { DataSyncFeedback } from './useDataSyncFeedback';
import type { DataSyncJobLifecycle } from './useDataSyncJobLifecycle';
import type { DataSyncRowSelection } from './useDataSyncSelection';
import type { DataSyncRunState } from './useDataSyncRunState';
import type { DataSyncSharedRefs } from './useDataSyncRefs';
import type { DataSyncSelectionState } from './useDataSyncSelectionState';

export interface UseDataSyncExecutionOptions {
  refs: DataSyncSharedRefs;
  feedback: DataSyncFeedback;
  run: DataSyncRunState;
  wizard: DataSyncWizardState;
  selection: DataSyncSelectionState;
  endpoints: DataSyncEndpoints;
  jobs: DataSyncJobLifecycle;
  rowSelection: DataSyncRowSelection;
}

/**
 * Execution phase of the Data Sync wizard.
 *
 * The write is submitted as a durable job. Its outcome may be unknown when
 * the backend cannot confirm the commit, and that unknown state stays fenced
 * until a follow-up comparison proves the databases are consistent again.
 */
export function useDataSyncExecution({
  refs,
  feedback,
  run,
  wizard,
  selection,
  endpoints,
  jobs,
  rowSelection,
}: UseDataSyncExecutionOptions) {
  const { t } = useI18n();
  const { setErrorMsg, setErrorOpen, setStatusMsg, setExecuteProgress } = feedback;
  const { setLastExecutionResult, setWriteOutcomeUncertain, writeOutcomeUncertain } = run;
  const { syncOptions, mappingResults, setMappingResults, setStep, setSyncState } = wizard;
  const { selectedRows, setSelectedRows, tableSelections, setTableSelections, setPageIndex, setPageCursors } =
    selection;
  const {
    sourceId,
    targetId,
    sourceDatabase,
    targetDatabase,
    sourceSchema,
    targetSchema,
    targetReadOnly,
    refreshEndpointSessions,
  } = endpoints;
  const { refreshDataSyncJobs } = jobs;
  const { hasSelectedDeletes } = rowSelection;
  const {
    jobIdRef,
    jobKindRef,
    cancelRequestedJobRef,
    cancellingStatusJobRef,
    writeInFlightRef,
    loadedPageRef,
  } = refs;

  const [executeConfirmOpen, setExecuteConfirmOpen] = useState(false);

  const runExecute = useCallback(async () => {
    if (!sourceId || !targetId) return;
    if (writeOutcomeUncertain) {
      setErrorMsg(t('sync.executionUnknown'));
      setErrorOpen(true);
      setSyncState('unknown');
      return;
    }
    if (targetReadOnly) {
      setErrorMsg(t('sync.targetReadOnly'));
      setErrorOpen(true);
      return;
    }
    setSyncState('executing');
    setExecuteProgress(t('sync.executing'));
    const jobId = crypto.randomUUID();
    jobIdRef.current = jobId;
    jobKindRef.current = 'execute';
    cancelRequestedJobRef.current = null;
    cancellingStatusJobRef.current = null;

    let writeStarted = false;
    let executionResolved = false;
    let recompareJobId: string | null = null;
    try {
      const { source, target } = await refreshEndpointSessions();
      const srcConnId = source?.dbSessionId;
      const tgtConnId = target?.dbSessionId;
      if (!srcConnId || !tgtConnId) {
        setSyncState('compared');
        return;
      }

      const tablesWithSelection = mappingResults.filter(
        (r) =>
          r.status === 'MATCHED' &&
          (selectedRows.some(
            (selected) =>
              selected.sourceTable === r.sourceTable && selected.targetTable === r.targetTable,
          ) ||
            tableSelections.some(
              (scope) => scope.sourceTable === r.sourceTable && scope.targetTable === r.targetTable,
            )),
      );

      if (jobIdRef.current !== jobId || cancelRequestedJobRef.current === jobId) return;
      if (selectedRows.length === 0 && tableSelections.length === 0) {
        setSyncState('compared');
        setExecuteProgress('');
        return;
      }
      setExecuteProgress(t('sync.executing'));
      writeStarted = true;
      writeInFlightRef.current = true;
      const result = await syncCommands.executeDataSyncJob(
        srcConnId,
        tgtConnId,
        jobId,
        syncOptions,
        selectedRows,
        tableSelections,
      );
      refreshDataSyncJobs();
      writeStarted = false;
      executionResolved = true;
      writeInFlightRef.current = false;
      setLastExecutionResult(result);

      const executionOutcome = result.outcome ?? (result.rolledBack ? 'rolled_back' : 'committed');
      if (executionOutcome === 'not_started') {
        setErrorMsg(result.error || t('sync.executionNotStarted'));
        setErrorOpen(true);
        setWriteOutcomeUncertain(false);
        setSyncState('compared');
        setExecuteProgress('');
        return;
      }
      if (executionOutcome === 'unknown') {
        setErrorMsg(`${t('sync.executionUnknown')} ${result.error || ''}`.trim());
        setErrorOpen(true);
        setWriteOutcomeUncertain(true);
        setSyncState('unknown');
        setStep('result');
        setExecuteProgress('');
        return;
      }

      setExecuteProgress(t('sync.recomparing'));
      const recompareFilters = Object.fromEntries(
        tablesWithSelection
          .filter((row) => row.sourceFilter)
          .map((row) => [row.sourceTable, row.sourceFilter as DataSyncSourceFilter]),
      );
      recompareJobId = crypto.randomUUID();
      jobIdRef.current = recompareJobId;
      jobKindRef.current = 'compare';
      cancelRequestedJobRef.current = null;
      cancellingStatusJobRef.current = null;
      const recomparedResponse = Object.keys(recompareFilters).length
        ? await syncCommands.compareDataSync(
            srcConnId,
            tgtConnId,
            tablesWithSelection.map((r) => r.sourceTable),
            recompareJobId,
            sourceDatabase,
            targetDatabase,
            sourceSchema || undefined,
            targetSchema || undefined,
            syncOptions,
            recompareFilters,
          )
        : await syncCommands.compareDataSync(
            srcConnId,
            tgtConnId,
            tablesWithSelection.map((r) => r.sourceTable),
            recompareJobId,
            sourceDatabase,
            targetDatabase,
            sourceSchema || undefined,
            targetSchema || undefined,
            syncOptions,
          );
      refreshDataSyncJobs();
      const recompared = Array.isArray(recomparedResponse)
        ? recomparedResponse
        : recomparedResponse.tables;
      const pagedRecompare =
        !Array.isArray(recomparedResponse) && recomparedResponse.contractVersion != null;
      if (pagedRecompare) {
        setSelectedRows([]);
        setTableSelections([]);
        setPageIndex({});
        setPageCursors({});
        loadedPageRef.current.clear();
      }
      setMappingResults((prev) => {
        const merged = mergeCompareIntoMappings(prev, recompared);
        return merged.map((row) => {
          if (pagedRecompare) return { ...row, rows: undefined };
          if (row.status !== 'MATCHED' || !row.rows) return row;
          return { ...row, rows: applyOptionsToRows(row.rows, syncOptions) };
        });
      });
      if (!pagedRecompare) {
        setSelectedRows(
          (recompared as DataSyncTableResult[]).flatMap((table) =>
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
      setWriteOutcomeUncertain(false);
      setSyncState('done');
      setStep('result');
      setExecuteProgress('');
      setStatusMsg('');
    } catch (e) {
      if (isStalePlanError(e)) {
        setErrorMsg(t('migrationJob.reprepareOnStalePlan'));
        setErrorOpen(true);
        setSyncState('compared');
        setLastExecutionResult(null);
        setExecuteProgress('');
        return;
      }
      setErrorMsg(
        `${writeStarted ? t('sync.executionUnknown') + ' ' : ''}${e instanceof Error ? e.message : String(e)}`,
      );
      setErrorOpen(true);
      if (writeStarted) {
        setWriteOutcomeUncertain(true);
        setSyncState('unknown');
      } else if (executionResolved) {
        setWriteOutcomeUncertain(false);
        setSyncState('done');
        setStep('result');
      } else {
        setSyncState('compared');
      }
      setExecuteProgress('');
    } finally {
      writeInFlightRef.current = false;
      if (cancellingStatusJobRef.current === jobId) {
        cancellingStatusJobRef.current = null;
        setStatusMsg((current) => (current === t('sync.cancellingExecution') ? '' : current));
      }
      if (jobIdRef.current === jobId && jobKindRef.current === 'execute') {
        jobIdRef.current = null;
        jobKindRef.current = null;
      }
      if (
        recompareJobId &&
        jobIdRef.current === recompareJobId &&
        jobKindRef.current === 'compare'
      ) {
        jobIdRef.current = null;
        jobKindRef.current = null;
      }
      if (cancelRequestedJobRef.current === jobId) cancelRequestedJobRef.current = null;
    }
  }, [
    sourceId,
    targetId,
    mappingResults,
    selectedRows,
    tableSelections,
    syncOptions,
    refreshEndpointSessions,
    sourceDatabase,
    targetDatabase,
    sourceSchema,
    targetSchema,
    targetReadOnly,
    t,
    writeOutcomeUncertain,
    refreshDataSyncJobs,
  ]);

  const handleExecute = useCallback(() => {
    if (targetReadOnly) {
      setErrorMsg(t('sync.targetReadOnly'));
      setErrorOpen(true);
      return;
    }
    if (hasSelectedDeletes) {
      setExecuteConfirmOpen(true);
      return;
    }
    void runExecute();
  }, [targetReadOnly, hasSelectedDeletes, runExecute]);

  return {
    runExecute,
    handleExecute,
    executeConfirmOpen,
    setExecuteConfirmOpen,
    canExecute: !writeOutcomeUncertain && mappingResults.some((r) => r.status === 'MATCHED' && tableHasRowDiffs(r)),
  };
}

export type DataSyncExecution = ReturnType<typeof useDataSyncExecution>;
