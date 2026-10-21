import { useCallback, useEffect, useMemo } from 'react';
import { syncCommands, type DataSyncRowChange, type DataSyncSelectionExclusion, type DataSyncTableResult } from '../../commands/sync';
import type { DataSyncWizardState } from './dataSyncTypes';
import {
  defaultRowSelected,
  operationAllowed,
  rowDiffCounts,
  rowKeyString,
  tableKey,
  tablePageKey,
} from './mappingView';
import { scopeForOperation, scopeSelectsOperation, selectedRowToken } from './selectionScope';
import type { DataSyncFeedback } from './useDataSyncFeedback';
import type { DataSyncSharedRefs } from './useDataSyncRefs';
import type { DataSyncSelectionState } from './useDataSyncSelectionState';
import { isComparedSyncState } from './utils';

export interface UseDataSyncSelectionOptions {
  refs: DataSyncSharedRefs;
  feedback: DataSyncFeedback;
  wizard: DataSyncWizardState;
  selection: DataSyncSelectionState;
}

/**
 * Row level review of a Data Sync comparison: the paged row diffs of the
 * selected table and the selection scopes the user applies to them.
 *
 * A table scope ("select all inserts of this table") and the explicit row
 * selection are two views of the same decision, so the scope always wins over
 * an explicit row and the explicit rows are the remainder.
 */
export function useDataSyncSelection({
  refs,
  feedback,
  wizard,
  selection,
}: UseDataSyncSelectionOptions) {
  const { setErrorMsg, setErrorOpen } = feedback;
  const { syncOptions, mappingResults, setMappingResults, selectedTableKey, syncState } = wizard;
  const {
    selectedRows,
    setSelectedRows,
    tableSelections,
    setTableSelections,
    setPageLoading,
    setPageIndex,
    pageCursors,
    setPageCursors,
  } = selection;
  const { selectedRowsRef, tableSelectionsRef, loadedPageRef } = refs;

  const selectedTable = useMemo(
    () => mappingResults.find((r) => tableKey(r) === selectedTableKey) ?? null,
    [mappingResults, selectedTableKey],
  );

  const loadTablePage = useCallback(
    async (table: DataSyncTableResult, direction: 'initial' | 'previous' | 'next' = 'initial') => {
      if (table.status !== 'MATCHED') return;
      const key = tablePageKey(table);
      const current = pageCursors[key] ?? {
        current: table.firstCursor ?? null,
        next: null,
        previous: [],
      };
      const cursor =
        direction === 'next'
          ? current.next
          : direction === 'previous'
            ? (current.previous[current.previous.length - 1] ?? null)
            : current.current;
      if (direction === 'next' && !current.next) return;
      if (direction === 'previous' && current.previous.length === 0) return;
      setPageLoading(true);
      try {
        const page = await syncCommands.getDataSyncComparisonPage(
          cursor,
          table.sourceTable,
          table.targetTable,
          table.pageSize,
        );
        const pageToken = `${key}\u0000${cursor ?? ''}`;
        const firstLoad = !loadedPageRef.current.has(pageToken);
        loadedPageRef.current.add(pageToken);
        const selectedTokenSet = new Set(
          selectedRowsRef.current
            .filter(
              (row) =>
                row.sourceTable === table.sourceTable && row.targetTable === table.targetTable,
            )
            .map(selectedRowToken),
        );
        const pageRows = page.rows.map((row) => {
          const scope = scopeForOperation(tableSelectionsRef.current, table, row.operation);
          return {
            ...row,
            selected:
              scope && scopeSelectsOperation(scope, row.operation, syncOptions)
                ? !scope.excludedRows.some(
                    (excluded) =>
                      excluded.operation === row.operation &&
                      rowKeyString(excluded.key) === rowKeyString(row.key),
                  )
                : firstLoad
                  ? row.selected
                  : selectedTokenSet.has(
                      selectedRowToken({
                        sourceTable: table.sourceTable,
                        targetTable: table.targetTable,
                        operation: row.operation,
                        key: row.key,
                      }),
                    ),
          };
        });
        if (firstLoad) {
          setSelectedRows((previous) => {
            const known = new Set(previous.map(selectedRowToken));
            const additions = pageRows
              .filter((row) => row.selected && row.operation !== 'UNCHANGED')
              .filter(
                (row) =>
                  !tableSelectionsRef.current.some(
                    (scope) =>
                      scope.sourceTable === table.sourceTable &&
                      scope.targetTable === table.targetTable &&
                      scopeSelectsOperation(scope, row.operation, syncOptions),
                  ),
              )
              .map((row) => ({
                sourceTable: table.sourceTable,
                targetTable: table.targetTable,
                operation: row.operation,
                key: row.key,
              }))
              .filter((row) => !known.has(selectedRowToken(row)));
            return additions.length ? [...previous, ...additions] : previous;
          });
        }
        setMappingResults((previous) =>
          previous.map((row) =>
            tableKey(row) === tableKey(table) ? { ...row, rows: pageRows } : row,
          ),
        );
        const nextPrevious =
          direction === 'next'
            ? [...current.previous, current.current ?? '']
            : direction === 'previous'
              ? current.previous.slice(0, -1)
              : [];
        setPageCursors((previous) => ({
          ...previous,
          [key]: { current: cursor, next: page.nextCursor, previous: nextPrevious },
        }));
        setPageIndex((previous) => ({
          ...previous,
          [key]: Math.max(
            0,
            (previous[key] ?? 0) + (direction === 'next' ? 1 : direction === 'previous' ? -1 : 0),
          ),
        }));
      } catch (error) {
        setErrorMsg(error instanceof Error ? error.message : String(error));
        setErrorOpen(true);
      } finally {
        setPageLoading(false);
      }
    },
    [pageCursors, setMappingResults, syncOptions],
  );

  const compared = useMemo(() => isComparedSyncState(syncState), [syncState]);

  useEffect(() => {
    if (!compared || !selectedTable || selectedTable.rows || selectedTable.status !== 'MATCHED')
      return;
    void loadTablePage(selectedTable, 'initial');
  }, [compared, selectedTableKey, selectedTable, loadTablePage]);

  const totalSelectedRows = useMemo(() => {
    const scopedCount = tableSelections.reduce((total, scope) => {
      const table = mappingResults.find(
        (row) => row.sourceTable === scope.sourceTable && row.targetTable === scope.targetTable,
      );
      if (!table) return total;
      const counts = rowDiffCounts(table);
      return (
        total +
        scope.operations.reduce((subtotal, operation) => {
          if (scope.selectionMode === 'defaults' && !defaultRowSelected(operation, syncOptions)) {
            return subtotal;
          }
          const count =
            operation === 'INSERT'
              ? counts.inserts
              : operation === 'UPDATE'
                ? counts.updates
                : counts.deletes;
          const excluded = scope.excludedRows.filter((row) => row.operation === operation).length;
          return subtotal + Math.max(0, count - excluded);
        }, 0)
      );
    }, 0);
    const explicitCount = selectedRows.filter((row) => {
      if (!operationAllowed(row.operation, syncOptions)) return false;
      return !tableSelections.some(
        (scope) =>
          scope.sourceTable === row.sourceTable &&
          scope.targetTable === row.targetTable &&
          scopeSelectsOperation(scope, row.operation, syncOptions),
      );
    }).length;
    return scopedCount + explicitCount;
  }, [mappingResults, selectedRows, syncOptions, tableSelections]);

  const hasSelectedDeletes = useMemo(() => {
    return (
      syncOptions.delete &&
      (selectedRows.some((row) => row.operation === 'DELETE') ||
        tableSelections.some((scope) => scopeSelectsOperation(scope, 'DELETE', syncOptions)))
    );
  }, [selectedRows, syncOptions.delete, tableSelections]);

  const updateTableRows = useCallback(
    (key: string, rows: DataSyncRowChange[]) => {
      const table = mappingResults.find((row) => tableKey(row) === key);
      if (!table) return;
      setSelectedRows((previous) => {
        const pageTokens = new Set(
          rows
            .filter((row) => row.operation !== 'UNCHANGED')
            .map((row) =>
              selectedRowToken({
                sourceTable: table.sourceTable,
                targetTable: table.targetTable,
                operation: row.operation,
                key: row.key,
              }),
            ),
        );
        const retained = previous.filter((row) => {
          if (!pageTokens.has(selectedRowToken(row))) return true;
          const scope = scopeForOperation(tableSelectionsRef.current, table, row.operation);
          return Boolean(scope && scopeSelectsOperation(scope, row.operation, syncOptions));
        });
        const next = rows
          .filter((row) => {
            if (!row.selected || row.operation === 'UNCHANGED') return false;
            const scope = scopeForOperation(tableSelectionsRef.current, table, row.operation);
            return !(scope && scopeSelectsOperation(scope, row.operation, syncOptions));
          })
          .map((row) => ({
            sourceTable: table.sourceTable,
            targetTable: table.targetTable,
            operation: row.operation,
            key: row.key,
          }));
        const known = new Set(retained.map(selectedRowToken));
        return [...retained, ...next.filter((row) => !known.has(selectedRowToken(row)))];
      });
      setTableSelections((previous) =>
        previous.map((candidate) => {
          if (
            candidate.sourceTable !== table.sourceTable ||
            candidate.targetTable !== table.targetTable
          )
            return candidate;
          const candidateRows = rows.filter((row) =>
            candidate.operations.includes(
              row.operation as Exclude<DataSyncRowChange['operation'], 'UNCHANGED'>,
            ),
          );
          const exclusions = candidate.excludedRows.filter(
            (excluded) =>
              !candidateRows.some(
                (row) =>
                  row.operation === excluded.operation &&
                  rowKeyString(row.key) === rowKeyString(excluded.key),
              ),
          );
          const additions: DataSyncSelectionExclusion[] = candidateRows
            .filter(
              (row) =>
                scopeSelectsOperation(candidate, row.operation, syncOptions) && !row.selected,
            )
            .map((row) => ({
              operation: row.operation as Exclude<DataSyncRowChange['operation'], 'UNCHANGED'>,
              key: row.key,
            }))
            .filter(
              (row) =>
                !exclusions.some(
                  (excluded) =>
                    excluded.operation === row.operation &&
                    rowKeyString(excluded.key) === rowKeyString(row.key),
                ),
            );
          return { ...candidate, excludedRows: [...exclusions, ...additions] };
        }),
      );
      setMappingResults((prev) => prev.map((r) => (tableKey(r) === key ? { ...r, rows } : r)));
    },
    [mappingResults, syncOptions],
  );

  const setTableOperationScope = useCallback(
    (
      table: DataSyncTableResult,
      operation: Exclude<DataSyncRowChange['operation'], 'UNCHANGED'>,
    ) => {
      if (!operationAllowed(operation, syncOptions)) return;
      setTableSelections((previous) => {
        const allScope = previous.find(
          (scope) =>
            scope.sourceTable === table.sourceTable &&
            scope.targetTable === table.targetTable &&
            scope.selectionMode === 'all',
        );
        if (allScope?.operations.includes(operation)) return previous;
        const cleaned = previous
          .map((scope) =>
            scope.sourceTable === table.sourceTable && scope.targetTable === table.targetTable
              ? {
                  ...scope,
                  operations: scope.operations.filter((candidate) => candidate !== operation),
                  excludedRows: scope.excludedRows.filter((row) => row.operation !== operation),
                }
              : scope,
          )
          .filter((scope) => scope.operations.length > 0);
        if (allScope) {
          return cleaned.map((scope) =>
            scope.sourceTable === table.sourceTable &&
            scope.targetTable === table.targetTable &&
            scope.selectionMode === 'all'
              ? { ...scope, operations: [...scope.operations, operation] }
              : scope,
          );
        }
        return [
          ...cleaned,
          {
            sourceTable: table.sourceTable,
            targetTable: table.targetTable,
            selectionMode: 'all',
            operations: [operation],
            excludedRows: [],
          },
        ];
      });
      setSelectedRows((previous) =>
        previous.filter(
          (row) =>
            row.sourceTable !== table.sourceTable ||
            row.targetTable !== table.targetTable ||
            row.operation !== operation,
        ),
      );
      setMappingResults((previous) =>
        previous.map((candidate) =>
          tableKey(candidate) === tableKey(table)
            ? {
                ...candidate,
                rows: candidate.rows?.map((row) =>
                  row.operation === operation ? { ...row, selected: true } : row,
                ),
              }
            : candidate,
        ),
      );
    },
    [setMappingResults, syncOptions],
  );

  const clearTableOperationScope = useCallback(
    (
      table: DataSyncTableResult,
      operation: Exclude<DataSyncRowChange['operation'], 'UNCHANGED'>,
    ) => {
      setTableSelections((previous) => {
        const cleaned = previous
          .map((scope) =>
            scope.sourceTable === table.sourceTable && scope.targetTable === table.targetTable
              ? {
                  ...scope,
                  operations: scope.operations.filter((candidate) => candidate !== operation),
                  excludedRows: scope.excludedRows.filter((row) => row.operation !== operation),
                }
              : scope,
          )
          .filter((scope) => scope.operations.length > 0);
        const defaultsScope = cleaned.find(
          (scope) =>
            scope.sourceTable === table.sourceTable &&
            scope.targetTable === table.targetTable &&
            scope.selectionMode === 'defaults',
        );
        if (defaultsScope) {
          return cleaned.map((scope) =>
            scope === defaultsScope
              ? { ...scope, operations: [...scope.operations, operation] }
              : scope,
          );
        }
        return [
          ...cleaned,
          {
            sourceTable: table.sourceTable,
            targetTable: table.targetTable,
            selectionMode: 'defaults',
            operations: [operation],
            excludedRows: [],
          },
        ];
      });
      setSelectedRows((previous) =>
        previous.filter(
          (row) =>
            row.sourceTable !== table.sourceTable ||
            row.targetTable !== table.targetTable ||
            row.operation !== operation,
        ),
      );
      setMappingResults((previous) =>
        previous.map((candidate) =>
          tableKey(candidate) === tableKey(table)
            ? {
                ...candidate,
                rows: candidate.rows?.map((row) =>
                  row.operation === operation
                    ? { ...row, selected: defaultRowSelected(row.operation, syncOptions) }
                    : row,
                ),
              }
            : candidate,
        ),
      );
    },
    [setMappingResults, syncOptions],
  );

  const isTableOperationScoped = useCallback(
    (table: DataSyncTableResult, operation: Exclude<DataSyncRowChange['operation'], 'UNCHANGED'>) =>
      tableSelections.some(
        (scope) =>
          scope.sourceTable === table.sourceTable &&
          scope.targetTable === table.targetTable &&
          scope.operations.includes(operation),
      ),
    [tableSelections],
  );

  return {
    selectedTable,
    loadTablePage,
    totalSelectedRows,
    hasSelectedDeletes,
    updateTableRows,
    setTableOperationScope,
    clearTableOperationScope,
    isTableOperationScoped,
  };
}

export type DataSyncRowSelection = ReturnType<typeof useDataSyncSelection>;
