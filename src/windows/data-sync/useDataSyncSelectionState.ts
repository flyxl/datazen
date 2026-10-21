import { useEffect, useState, type Dispatch, type SetStateAction } from 'react';
import type { DataSyncSelectedRow, DataSyncTableSelection } from '../../commands/sync';
import type { DataSyncSharedRefs } from './useDataSyncRefs';

export interface DataSyncPageCursor {
  current: string | null;
  next: string | null;
  previous: string[];
}

/**
 * Row selection state of the Data Sync wizard.
 *
 * Holds the explicit row selection, the per-table operation scopes and the
 * pagination cursors of the comparison detail view. Changing the endpoint
 * scope invalidates all of it, so the state is reset whenever the source or
 * target endpoint changes.
 */
export interface DataSyncSelectionState {
  selectedRows: DataSyncSelectedRow[];
  setSelectedRows: Dispatch<SetStateAction<DataSyncSelectedRow[]>>;
  tableSelections: DataSyncTableSelection[];
  setTableSelections: Dispatch<SetStateAction<DataSyncTableSelection[]>>;
  pageLoading: boolean;
  setPageLoading: Dispatch<SetStateAction<boolean>>;
  pageIndex: Record<string, number>;
  setPageIndex: Dispatch<SetStateAction<Record<string, number>>>;
  pageCursors: Record<string, DataSyncPageCursor>;
  setPageCursors: Dispatch<SetStateAction<Record<string, DataSyncPageCursor>>>;
}

export interface UseDataSyncSelectionStateOptions {
  refs: DataSyncSharedRefs;
  sourceId: string;
  targetId: string;
  sourceDatabase: string;
  targetDatabase: string;
  sourceSchema: string;
  targetSchema: string;
}

export function useDataSyncSelectionState({
  refs,
  sourceId,
  targetId,
  sourceDatabase,
  targetDatabase,
  sourceSchema,
  targetSchema,
}: UseDataSyncSelectionStateOptions): DataSyncSelectionState {
  const [selectedRows, setSelectedRows] = useState<DataSyncSelectedRow[]>([]);
  const [tableSelections, setTableSelections] = useState<DataSyncTableSelection[]>([]);
  const [pageLoading, setPageLoading] = useState(false);
  const [pageIndex, setPageIndex] = useState<Record<string, number>>({});
  const [pageCursors, setPageCursors] = useState<Record<string, DataSyncPageCursor>>({});
  refs.selectedRowsRef.current = selectedRows;
  refs.tableSelectionsRef.current = tableSelections;

  useEffect(() => {
    setSelectedRows([]);
    setTableSelections([]);
    setPageIndex({});
    setPageCursors({});
    refs.loadedPageRef.current.clear();
  }, [sourceId, targetId, sourceDatabase, targetDatabase, sourceSchema, targetSchema]);

  return {
    selectedRows,
    setSelectedRows,
    tableSelections,
    setTableSelections,
    pageLoading,
    setPageLoading,
    pageIndex,
    setPageIndex,
    pageCursors,
    setPageCursors,
  };
}
