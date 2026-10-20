import { useCallback, useState } from 'react';
import type { SyncOptions } from '../../commands/sync';
import type { DataSyncWizardState } from './dataSyncTypes';
import { applyOptionsToRows, operationAllowed } from './mappingView';
import type { DataSyncSelectionState } from './useDataSyncSelectionState';

export interface UseDataSyncOptionsOptions {
  wizard: DataSyncWizardState;
  selection: DataSyncSelectionState;
}

/**
 * Sync options of the Data Sync wizard.
 *
 * Changing the conflict policy invalidates the immutable comparison plan the
 * server bound to the rows, so the comparison is dropped and the wizard
 * returns to the setup step. Enabling deletes needs an explicit confirmation.
 */
export function useDataSyncOptions({ wizard, selection }: UseDataSyncOptionsOptions) {
  const { syncOptions, setSyncOptions, setMappingResults, setSelectedTableKey, step, setStep, syncState, setSyncState } =
    wizard;
  const { setSelectedRows, setTableSelections } = selection;
  const [deleteConfirmOpen, setDeleteConfirmOpen] = useState(false);

  const handleOptionsChange = useCallback(
    (next: SyncOptions) => {
      const previousPolicy = syncOptions.conflictPolicy ?? 'abort';
      const nextPolicy = next.conflictPolicy ?? 'abort';
      setSyncOptions(next);
      if (
        previousPolicy !== nextPolicy &&
        (syncState === 'compared' || step === 'preview' || step === 'result')
      ) {
        // The server binds conflict handling to the immutable comparison plan.
        // Drop row-level comparison state before allowing the user to continue.
        setMappingResults((rows) => rows.map((row) => ({ ...row, rows: undefined })));
        setSelectedTableKey(null);
        setSyncState('idle');
        setStep('setup');
        return;
      }
      setMappingResults((rows) =>
        rows.map((row) => {
          if (!row.rows) return row;
          return { ...row, rows: applyOptionsToRows(row.rows, next) };
        }),
      );
      setSelectedRows((rows) =>
        rows.filter((row) => {
          if (row.operation === 'INSERT') return next.insert;
          if (row.operation === 'UPDATE') return next.update;
          if (row.operation === 'DELETE') return next.delete;
          return false;
        }),
      );
      setTableSelections((scopes) =>
        scopes
          .map((scope) => ({
            ...scope,
            operations: scope.operations.filter((operation) => operationAllowed(operation, next)),
            excludedRows: scope.excludedRows.filter((row) => operationAllowed(row.operation, next)),
          }))
          .filter((scope) => scope.operations.length > 0),
      );
    },
    [
      setMappingResults,
      setSelectedTableKey,
      setStep,
      setSyncOptions,
      setSyncState,
      step,
      syncOptions.conflictPolicy,
      syncState,
    ],
  );

  const handleEnableDelete = useCallback(() => {
    setDeleteConfirmOpen(true);
  }, []);

  const confirmEnableDelete = useCallback(() => {
    setSyncOptions((prev) => ({ ...prev, delete: true }));
    setDeleteConfirmOpen(false);
  }, []);

  return {
    handleOptionsChange,
    handleEnableDelete,
    confirmEnableDelete,
    deleteConfirmOpen,
    setDeleteConfirmOpen,
  };
}

export type DataSyncOptions = ReturnType<typeof useDataSyncOptions>;
