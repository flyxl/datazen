import type { ColumnSchema, FilterCondition, SortCondition } from '../../types';
import type {
  CommitPendingChangesResult,
  PendingRowChange,
  PendingStatus,
  RowChangePlan,
  TableChangeContext,
} from '../../lib/tableChanges';

export interface CellEdit {
  rowIndex: number;
  columnName: string;
  originalValue: unknown;
  newValue: unknown;
  pkSnapshot: Record<string, unknown>;
}

/** Per-table state slice */
export interface TableState {
  context: TableChangeContext | null;
  columns: ColumnSchema[];
  rows: Record<string, unknown>[];
  totalRows: number;
  page: number;
  pageSize: number;
  /** Applied filters used by queries. */
  filters: FilterCondition[];
  filterLogic: 'and' | 'or';
  draftFilters: FilterCondition[];
  draftFilterLogic: 'and' | 'or';
  filterPanelOpen: boolean;
  sorts: SortCondition[];
  editBuffer: Map<string, CellEdit>;
  /** Changes staged against this table; the map key is the stable PK identity. */
  pendingChanges: Map<string, PendingRowChange>;
  /** Ephemeral row-index anchor; it only points back to an original identity key. */
  rowIdentityAnchors: Map<number, string>;
  previewPlan: RowChangePlan | null;
  pendingStatus: PendingStatus;
  lastCommitResult: CommitPendingChangesResult | null;
  selectedRows: Set<number>;
  lastSelectedIndex: number | null;
  editingCell: { row: number; col: string } | null;
  loading: boolean;
  /** Monotonic revision of the latest requested page/filter/sort state. */
  requestRevision: number;
  /** Revision currently represented by the in-flight request, if any. */
  loadingRevision: number | null;
  error: string | null;
  visibleColumns: string[] | null;
}

/** Per-connection table-data state */
export interface ConnectionTableState {
  activeTable: string | null;
  activeTableKey: string | null;
  connectionId: string | null;
  databaseType: string | null;
  /** F1: last target database used for table data loads on this session. */
  activeDatabase: string | null;
  activeSchema: string | null;
  tableStates: Map<string, TableState>;
  detailRowIndex: number | null;
}
