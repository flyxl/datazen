import type { CountStrategy } from '@datazen/backend-client';
import type { ColumnSchema, FilterCondition, SortCondition } from '../../types';
import type {
  PendingInsert,
  PendingRowChange,
  PendingStatus,
  RowChangePlan,
  TableChangeContext,
} from '../../lib/tableChanges';
import type { GridSelection } from './gridSelection';
import type { KeysetPagingState } from '../../lib/keysetPaging';

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
  /**
   * How much `totalRows` may be trusted.
   *
   * The three states are the whole point of the count contract: an exact
   * number, an estimate that has to be shown as "about N rows", and "this
   * database cannot say". A response that carries no kind keeps the value
   * this panel already had — that is what a paged re-read sends.
   */
  totalRowsKind: CountStrategy;
  /**
   * The panel's temporary override of the default order. `inherit` follows
   * the global setting; the other values are this panel's own choice and are
   * not persisted.
   */
  defaultOrderOverride: 'inherit' | 'primaryKeyAsc' | 'primaryKeyDesc' | 'none';
  page: number;
  pageSize: number;
  /**
   * How this panel is paged.
   *
   * `page` stays the ordinal it has always been — "the Nth page" — and the
   * keyset state says how that page is **read**: by offset, or by seek from
   * a boundary that was really read. There is no second page number.
   */
  keyset: KeysetPagingState;
  /**
   * The page whose row count was already recomputed after a drift.
   *
   * Rows deleted while the user was paging make the count stale, and the
   * count is then read again — once per page. This remembers which page
   * that was, so a table that keeps shrinking cannot make the panel read
   * itself in a loop.
   */
  driftRecountPage: number | null;
  /** Applied filters used by queries. */
  filters: FilterCondition[];
  filterLogic: 'and' | 'or';
  draftFilters: FilterCondition[];
  draftFilterLogic: 'and' | 'or';
  filterPanelOpen: boolean;
  sorts: SortCondition[];
  /**
   * The panel's explicit column **render order** (DB-12): `null` follows
   * the schema order. It is a permutation hint over the *displayed* columns
   * — membership stays with `visibleColumns`, so the two never answer the
   * same question twice. Columns missing from the list keep their schema
   * position at the end; names that no longer exist are ignored.
   */
  columnOrder: string[] | null;
  /**
   * How many leading displayed columns are frozen (DB-12): they stay put
   * while the grid scrolls horizontally. `0` — the default — freezes
   * nothing. Panel-scoped on purpose: only widths and order persist.
   */
  frozenColumnCount: number;
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
  /**
   * The candidate values of the enumeration columns, keyed by column name
   * (DB-05).
   *
   * The cache is filled **only** when the user asks for the candidates:
   * opening an editor must never query anything. It lives in the panel
   * because the candidates belong to a table.
   */
  enumCandidates: Map<string, readonly string[]>;
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
