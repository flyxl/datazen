import type { ColumnSchema, FilterCondition, SortCondition } from '../../types';
import type {
  PendingInsert,
  PendingRowChange,
  PendingStatus,
  RowChangePlan,
  TableChangeContext,
} from '../../lib/tableChanges';
import type { GridSelection } from './gridSelection';

export interface CellEdit {
  rowIndex: number;
  columnName: string;
  originalValue: unknown;
  newValue: unknown;
  pkSnapshot: Record<string, unknown>;
}

/**
 * Table-data state for one panel (tab). The panel is the unit of lifetime and of
 * cache scope: page/filters/selection/staged edits belong to the tab that set them,
 * never to the session, so two tabs on the same table never overwrite each other.
 */
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
  /**
   * New-row drafts, keyed by their draft id.
   *
   * Deliberately **not** part of `pendingChanges`: every consumer of that map
   * assumes the row already exists and has a primary-key identity, while a
   * draft has none (and tables without a primary key may still be inserted
   * into). The two collections only meet at the wire boundary, where
   * `draftsForWire()` turns a draft into an `insertMarked` row change.
   */
  draftInserts: Map<string, PendingInsert>;
  /** Creation order of the drafts; it decides the plan order and the render order. */
  draftInsertOrder: string[];
  /** Identity key of the row highlighted right after an insert commit; `null` when none. */
  highlightedNewRowKey: string | null;
  /** Ephemeral row-index anchor; it only points back to an original identity key. */
  rowIdentityAnchors: Map<number, string>;
  previewPlan: RowChangePlan | null;
  pendingStatus: PendingStatus;
  selectedRows: Set<number>;
  lastSelectedIndex: number | null;
  /** Cell/region selection. The store is the only writer; `none` is the default. */
  gridSelection: GridSelection;
  editingCell: { row: number; col: string } | null;
  /** Row highlighted by the detail drawer. */
  detailRowIndex: number | null;
  loading: boolean;
  /** Monotonic revision of the latest requested page/filter/sort state. */
  requestRevision: number;
  /** Revision currently represented by the in-flight request, if any. */
  loadingRevision: number | null;
  error: string | null;
  visibleColumns: string[] | null;
}
