/**
 * Table-data DTO mirrors: the wire shapes of the table browsing surface.
 *
 * These are copies, not designs. Every shape here exists because a backend
 * already emits it or a frontend already consumes it, and the copy exists so
 * that a remote/browser backend and the desktop host speak **one** vocabulary:
 * a table read, a change plan and a commit have to mean the same thing on both
 * sides of the port, or the same UI produces different results depending on
 * which form factor it runs in.
 *
 * The primary source of truth for the *frontend* half is
 * `src/lib/tableChanges.ts` (the desktop mirror of the shared Rust change set
 * in `modules/src/table_editing`). This file is that mirror's
 * counterpart inside the transport-agnostic package, and the two are held
 * together by a compile-time parity assertion in
 * `packages/backend-client/__tests__/tableData.test.ts` — a drift between them
 * is a typecheck failure, not a runtime surprise.
 *
 * What is deliberately *not* here: any transport detail, any host capability
 * and any SQL. A shape in this file says what travels; it never says how.
 */

import type { Counter } from './identity';
import type { RuntimeResultBinding, SessionHandle } from './session';

/**
 * One cell value.
 *
 * The backend's `Value` enum is `#[serde(untagged)]`, so every variant reaches
 * the wire as plain JSON — `null`, a boolean, a number, a string, a byte array
 * or an arbitrary JSON value. This mirrors the frontend's `Value`
 * (`src/types/index.ts`).
 */
export type TableValue = string | number | boolean | null | Record<string, unknown> | unknown[];

/**
 * Narrow an unknown value to a {@link TableValue}.
 *
 * The backend's `Value` is untagged, so the wire carries plain JSON. An
 * editor hands over whatever the user typed, and a value the wire cannot
 * carry — `undefined`, a function, a symbol, a bigint, a non-finite number —
 * must be refused at the boundary rather than sent and silently dropped by
 * the serializer.
 *
 * `undefined` means "refused", not "absent": a cell whose value is SQL `NULL`
 * narrows to `null`, which is a value.
 *
 * The check is deliberately shallow. It answers "can this travel as JSON at
 * the top level"; what a nested element contains is the backend's business.
 */
export function toTableValue(value: unknown): TableValue | undefined {
  if (value === null) return null;
  switch (typeof value) {
    case 'string':
    case 'boolean':
      return value;
    case 'number':
      return Number.isFinite(value) ? value : undefined;
    case 'object':
      if (Array.isArray(value)) return value;
      return value as Record<string, unknown>;
    default:
      return undefined;
  }
}

/** The immutable values that identify a row for an UPDATE or DELETE. */
export type RowIdentity = Record<string, TableValue | null>;

/**
 * The routing context a table operation is bound to.
 *
 * Mirrors the frontend's `TableChangeContext` (`src/lib/tableChanges.ts`).
 * `connectionId` and `driverType` are nullable here because the frontend
 * assembles the context before the connection is resolved; the backend
 * requires every routing field and refuses an incomplete context, which is
 * what the frontend's `isCompleteTableChangeContext` guard exists to prevent.
 */
export interface TableChangeContext {
  connectionId: string | null;
  dbSessionId: string;
  driverType: string | null;
  database: string | null;
  schema: string | null;
  table: string;
}

/**
 * One staged row change.
 *
 * `insertMarked` is the flag that separates a **new row draft** from a change
 * to an existing row: an insert has no identity yet, so `changedColumns` is
 * the exact INSERT column list and a column absent from it keeps the database
 * default. The backend never receives `insertMarked: false` — the flag is
 * omitted when it is unset, which keeps the fingerprint of an insert-free plan
 * byte-identical to the payload that predates the flag.
 */
export interface PendingRowChange {
  /** UI-only hint for keeping a PK edit associated with its original row. */
  rowIndex?: number;
  rowIdentity: RowIdentity;
  originalValues: Record<string, TableValue | null>;
  currentValues: Record<string, TableValue | null>;
  changedColumns: string[];
  deleteMarked: boolean;
  /**
   * Marks this entry as a **new row draft** rather than a change to an existing
   * row. Such a row has no identity yet, so `changedColumns` is the exact
   * INSERT column list and a column absent from it keeps the database default
   * (auto-increment, generated, `DEFAULT now()`). `null` in `currentValues`
   * still means SQL `NULL`. The host never serializes `false`, which keeps
   * fingerprints of insert-free plans byte-identical.
   */
  insertMarked?: boolean;
}

/** One statement of a change plan, with its driver-rendered preview. */
export interface PlannedStatement {
  rowIdentity: RowIdentity;
  originalValues: Record<string, TableValue | null>;
  currentValues: Record<string, TableValue | null>;
  changedColumns: string[];
  /** Driver-rendered SQL preview. This is not a wire-level SQL audit record. */
  sqlTemplate: string;
  /** Human-readable summaries of the values used by the statement. */
  parameterSummary: string[];
}

/** A non-fatal remark attached to a change plan. */
export interface ChangeWarning {
  code: string;
  message: string;
  severity: 'info' | 'warning' | 'error';
}

/**
 * The serializable plan of a staged change set.
 *
 * `inserts` carries the new-row statements in **request order**: the user's
 * visual draft order is also the execution order, because insert order decides
 * auto-increment allocation. The field is **always present**: the host declares
 * it with `#[serde(default)]` (a tolerance for deserializing a plan that does
 * not carry it) and serializes it unconditionally, so a plan without inserts
 * carries an empty list rather than a missing key.
 */
export interface RowChangePlan {
  planId: string;
  fingerprint: string;
  /**
   * The connection's configuration revision at plan time.
   *
   * The wire carries a `Counter`, and a `Counter` travels as a **decimal
   * string** (`platform-api` serializes `u64` with `collect_str`, because a
   * JSON number cannot hold a count above 2^53).
   *
   * **This package does not narrow it.** The field is declared as the
   * application-facing branded `number`, but the value that reaches the
   * application is whatever the adapter handed over: the desktop adapter
   * forwards the payload unchanged, so a string is still a string when it
   * arrives. Narrowing with `toCounter` is the adapter's job, and no adapter
   * in the tree does it today — that is a known gap in the whole package
   * (`ProfileView.configRevision` has the same shape and the same gap), not
   * something this type can close. `toCounter` does exist here, but the only
   * place the package uses it is the job progress counters, which the facade
   * genuinely decodes.
   */
  configRevision?: Counter | null;
  table: TableChangeContext;
  updates: PlannedStatement[];
  deletes: PlannedStatement[];
  /** New-row statements in request order: insertion order decides auto-increment. */
  inserts: PlannedStatement[];
  warnings: ChangeWarning[];
}

/**
 * What one committed statement did.
 *
 * `operation` is the discriminant a frontend switches on to tell an insert
 * from an update or a delete — the insert case is the reason the field exists
 * at all: a row that did not exist before cannot be reported by an "updated
 * rows" counter.
 */
export type CommitOperation = 'insert' | 'update' | 'delete';

export interface CommitStatementResult {
  operation: CommitOperation;
  rowIdentity: RowIdentity;
  affectedRows: number;
}

export interface CommitPendingChangesResponse {
  planId: string;
  fingerprint: string;
  statements: CommitStatementResult[];
  affectedRows: number;
}

/** The request of a change preview: a context plus the staged changes. */
export interface PreviewPendingChangesRequest {
  context: TableChangeContext;
  changes: PendingRowChange[];
}

/**
 * The request of a change commit.
 *
 * `fingerprint` is the guard: the backend re-computes the plan's fingerprint
 * and refuses a commit whose plan no longer matches the request, so a stale
 * preview can never be applied.
 */
export interface CommitPendingChangesRequest {
  dbSessionId: string;
  manualHandle?: SessionHandle | null;
  plan: RowChangePlan;
  fingerprint: string;
}

/**
 * One filter condition.
 *
 * `operator` is a plain string on the wire. The backend validates it against
 * the dialect contract, and the frontend narrows it to its own operator union
 * — a union pinned here would freeze the operator set in a package that must
 * not know which operators a dialect declares.
 */
export interface FilterCondition {
  column: string;
  operator: string;
  value?: TableValue;
  /**
   * How this condition joins the condition before it.
   *
   * The connector hangs on the *current* condition: concatenation runs left
   * to right, so the first condition has no predecessor and its own connector
   * is meaningless — the host ignores it. `undefined` falls back to the
   * request-wide logical operator.
   *
   * The host's `FilterCondition` (`src/types/filters.ts`) is the mirror of
   * this interface, and the two are held in step by a compile-time
   * assertion (`src/lib/__tests__/namedViews.test.ts`). A saved view carries
   * the connector with it, so a view that was saved with per-condition logic
   * restores that logic instead of falling back to the global one.
   */
  connector?: 'and' | 'or';
}

/** One sort key. */
export interface SortCondition {
  column: string;
  descending: boolean;
}

/**
 * The column metadata a table read returns.
 *
 * Mirrors the driver SDK's `ColumnSchema`; the backend-client cannot import
 * the driver SDK (the dependency runs the other way), so the shape is copied
 * and held in place by the same parity assertion as the change-set types.
 */
export interface TableColumnSchema {
  name: string;
  dataType: string;
  nullable: boolean;
  defaultValue?: string | null;
  isPrimaryKey?: boolean;
  isAutoIncrement?: boolean;
  comment?: string;
}

/**
 * How much the row count of a table read may be trusted.
 *
 * The count used to be a boolean on the request side ("count it or do not
 * count it"), which cannot say *how* the number was obtained. A driver that
 * can only offer a catalog estimate and a driver that cannot count at all
 * are both "no exact count", and the UI has to render them differently: one
 * shows "≈ 1.2M rows", the other shows "row count unavailable".
 *
 * The names are the host's `CountStrategy` enum, in its camelCase form.
 */
export type CountStrategy = 'exact' | 'estimated' | 'unsupported';

/**
 * How a page was read.
 *
 * The mirror of the host's `PagingMode`: by offset (the page starts
 * `page × pageSize` rows in) or by seek (the page starts after a value
 * boundary that was read from a real row).
 */
export type PagingMode = 'offset' | 'keyset';

/**
 * Why a page could not be read by seek.
 *
 * The mirror of the host's `KeysetUnavailableReason`. Every one of them is a
 * **degradation**, not an error: the page is read by offset and the reason
 * says why.
 */
export type KeysetUnavailableReason =
  | 'driverUnsupported'
  | 'noStableOrder'
  | 'nullableKeyColumn'
  | 'nullBoundary'
  | 'predicateUnavailable'
  | 'noPaginationClause'
  | 'staleCursor'
  | 'notAdjacent';

/**
 * One boundary value, **with its tag**.
 *
 * The host's `Value` is untagged, so `[1, 2, 3]` reads as `Bytes` *or* as
 * `Json`. A boundary travels "back end → front end → back end", and untagged
 * it would change its variant on the way back — the seek would then compare
 * against the wrong literal, with no error anywhere. The tag is what keeps
 * the two apart.
 */
export type CursorValue =
  | { kind: 'null' }
  | { kind: 'bool'; value: boolean }
  | { kind: 'integer'; value: number }
  | { kind: 'float'; value: number }
  | { kind: 'string'; value: string }
  | { kind: 'bytes'; value: number[] }
  | { kind: 'timestamp'; value: string }
  | { kind: 'json'; value: unknown };

/** One ordering column and its direction. */
export interface OrderColumn {
  column: string;
  descending: boolean;
}

/** What the host did about the paging of one page. */
export interface PagePagingInfo {
  /**
   * The strategy that was used. **The front end believes this field**, not
   * its own guess about which strategy is in force.
   */
  mode: PagingMode;
  /** The real column order of the page, key columns included. */
  orderColumns: OrderColumn[];
  /** The key values of the last row, to be sent back for the next page. */
  nextAfterKey?: CursorValue[];
  /** Set when the page was **not** read by seek: why not. */
  unavailableReason?: KeysetUnavailableReason;
  /** Rows were deleted while the user was paging. */
  driftSuspected: boolean;
}

/** The result of a table read. */
export interface TableDataResult {
  columns: TableColumnSchema[];
  rows: (TableValue | null)[][];
  /**
   * The row count, or `null` when the backend could not count.
   *
   * The backend's field is an optional count, so the key is always present
   * and the *value* may be `null` — "the count was not taken" is a fact the
   * UI has to render, not a missing field.
   */
  totalRows?: number | null;
  /**
   * How much `totalRows` may be trusted.
   *
   * `null` (or a missing key) means "this response carries no fresh row
   * count" — the caller asked to reuse the one it already had, or the
   * backend is older than this field — and the caller keeps its own value.
   * It is not the same as `'unsupported'`, which says the count was
   * attempted and this database cannot provide one.
   */
  totalRowsKind?: CountStrategy | null;
  page: number;
  pageSize: number;
  /**
   * How the page was paged.
   *
   * A missing key means the back end is older than this field — the page was
   * read by offset, which is what every read did before it existed. A
   * `null` value says the same thing; both are "this response carries no
   * paging decision".
   */
  paging?: PagePagingInfo | null;
}

/**
 * The request of a target-bound table read.
 *
 * The read never borrows an editor's session: it either runs against the
 * explicit target named by `context` or against the session `manualHandle`
 * pins. `manualHandle` is `null` for the ordinary, target-bound read.
 */
export interface TableDataRequest {
  context: TableChangeContext;
  manualHandle?: SessionHandle | null;
  page: number;
  pageSize: number;
  filters?: FilterCondition[];
  sorts?: SortCondition[];
  skipCount?: boolean;
  filterLogic?: 'and' | 'or';
  /**
   * The row-count strategy to use. Omitted means "the driver decides"; the
   * backend never counts against the driver's own declaration.
   */
  countStrategy?: CountStrategy;
  /**
   * The columns to order by when the caller did not ask for an order. An
   * empty array means "do not order at all"; omitted means "the driver's
   * default order".
   */
  defaultOrderColumns?: string[];
  /**
   * The boundary of the page: the key values of the last row of the
   * previous page.
   *
   * Omitted means "the first page of a read". The values are **not**
   * computed by the caller — they are what the back end reported for the
   * previous page, sent back verbatim. A caller that derives a boundary
   * from a page number gets a wrong page, silently.
   */
  afterKey?: CursorValue[];
}

/**
 * The request of a row-count estimate.
 *
 * `null` is a legitimate answer, not an error: a driver without an estimate
 * statement cannot say how many rows a table has, and the caller — not this
 * command — decides whether to fall back to an exact count.
 */
export interface EstimateTableRowsRequest {
  dbSessionId: string;
  table: string;
  database?: string | null;
  schema?: string | null;
}

/** The referenced side of a foreign key, as the backend resolved it. */
export interface ForeignKeyTargetRef {
  database: string;
  schema: string | null;
  table: string;
  /** The referenced columns, in the same order as the local columns. */
  columns: string[];
  /** The type names of `columns`, in the same order. */
  dataTypes: string[];
  primaryKeys: string[];
  /** The referenced columns are covered by a primary key or a unique index. */
  unique: boolean;
  candidateLimit: number;
}

/**
 * The result of resolving a foreign key.
 *
 * `exists: false` means the target table is readable but the referenced row is
 * gone — a successful answer, not an error.
 */
export interface ForeignKeyTargetResolution {
  target: ForeignKeyTargetRef;
  exists: boolean;
  candidates: (TableValue | null)[][];
  truncated: boolean;
}

/** The request of a foreign-key resolution. */
export interface ResolveForeignKeyTargetsRequest {
  dbSessionId: string;
  database?: string | null;
  schema?: string | null;
  table: string;
  qualifiedTable?: string | null;
  /** The local columns of the foreign key, in pairing order. */
  columns: string[];
  /** Disambiguates two constraints over the same columns. */
  constraintName?: string | null;
  /** The local column values, positionally paired with `columns`. */
  values: TableValue[];
  limit?: number;
  /**
   * Browse mode (DB-20): the in-cell selector asks for what the target table
   * **offers** — the first `limit + 1` rows of the referenced columns —
   * instead of looking the current value up. The key `values` are ignored on
   * this path, so the selector may open before anything is picked.
   */
  browse?: boolean;
}

/** How the search value must match a column's value (DB-19). */
export type ValueSearchMode = 'contains' | 'exact' | 'prefix' | 'suffix';

/** The request of one table's value search (DB-19). */
export interface SearchTableValuesRequest {
  dbSessionId: string;
  /** The table to scan. */
  table: string;
  /** The columns to search — the caller picks them by type; the host never guesses. */
  columns: string[];
  /** The value to look for. */
  value: string;
  /** How the value must match; `contains` when omitted. */
  mode?: ValueSearchMode;
  database?: string | null;
  schema?: string | null;
}

/** The answer of one table's value search (DB-19). */
export interface SearchTableValuesResult {
  table: string;
  /** The columns the statement actually searched. */
  searchedColumns: string[];
  /** The rows matching at least one column. */
  matches: number;
}

/** The request of a reverse foreign-key catalog: who references this table? */
export interface ListInboundForeignKeysRequest {
  dbSessionId: string;
  database?: string | null;
  schema?: string | null;
  /** The table the references point at. */
  table: string;
  qualifiedTable?: string | null;
}

/** One referencing constraint, as the reverse jump needs it. */
export interface InboundForeignKeyRef {
  /** The referencing table, in the driver's own naming. */
  table: string;
  schema: string | null;
  constraintName: string;
  /** The referencing table's own columns that carry the reference. */
  columns: string[];
  /** The referenced columns of **this** table, in pairing order. */
  referencedColumns: string[];
}

/** The catalog: every inbound constraint the scan found, plus how far it got. */
export interface InboundForeignKeyCatalog {
  table: string;
  database: string;
  references: InboundForeignKeyRef[];
  scannedTables: number;
  /** The scan hit its cap before the table list ran out. */
  truncated: boolean;
}

/**
 * The scope a named view belongs to.
 *
 * `connectionId` is the **persistent** connection id; a `dbSessionId` is
 * runtime state and is never persisted, so it must never appear here.
 */
export interface NamedViewScope {
  connectionId: string;
  database: string | null;
  schema: string | null;
  table: string;
}

/**
 * A saved (filter + sort + column layout) view of one table.
 *
 * The three named-view commands are **desktop-only** for now: their storage
 * lives in the desktop host's persistent store, which has no counterpart in a
 * browser build. The shapes are declared here so the port stays complete, and
 * a backend that cannot serve them says so explicitly instead of answering
 * with an empty list.
 */
export interface NamedView {
  /** Stable identity, minted by the backend. Renaming never changes it. */
  id: string;
  name: string;
  scope: NamedViewScope;
  /** The **applied** filters, not the draft. */
  filters: FilterCondition[];
  filterLogic: 'and' | 'or';
  sorts: SortCondition[];
  /** `null` means every column is visible. */
  visibleColumns: string[] | null;
  /** `null` means the driver's default column order. */
  columnOrder: string[] | null;
  /** At most one per scope. */
  isDefault: boolean;
  /** RFC3339. */
  createdAt: string;
  /**
   * RFC3339, or `null` for a record that was never changed after it was
   * created.
   *
   * The host's field is an optional timestamp, so the wire carries `null`
   * for "never updated" — and this type says exactly that instead of
   * claiming a string the host does not always send.
   */
  updatedAt: string | null;
}

/** The request of a named-view listing; omitting `scope` lists every view. */
export interface ListNamedViewsRequest {
  scope?: NamedViewScope;
}

/** The request of a named-view save. An empty `id` asks the backend to mint one. */
export interface SaveNamedViewRequest {
  view: NamedView;
}

/** The request of a named-view delete. Deleting an unknown id is a no-op. */
export interface DeleteNamedViewRequest {
  id: string;
}

/**
 * One edited cell of a query result.
 *
 * The write is bound to the exact runtime resource that produced the result
 * (`binding`), so a result whose resource is gone is refused instead of being
 * written through some other handle.
 */
export interface ResultCellWrite {
  binding: RuntimeResultBinding;
  statementIndex: number;
  rowIndex: number;
  column: string;
  value: TableValue;
}

/**
 * The request of a SQL preview (DB-15): the query shape of a page read,
 * plus the one switch only a preview makes sense of.
 *
 * The shape deliberately mirrors {@link TableDataRequest} minus the fields a
 * preview has no use for (`skipCount`: a preview always shows the count
 * statements, it never "reuses a total") — the backend assembles through the
 * same code a real read executes, so what this returns is what the product
 * would send.
 */
export interface TableReadPreviewRequest {
  context: TableChangeContext;
  manualHandle?: SessionHandle | null;
  page: number;
  pageSize: number;
  filters?: FilterCondition[];
  sorts?: SortCondition[];
  filterLogic?: 'and' | 'or';
  countStrategy?: CountStrategy;
  defaultOrderColumns?: string[];
  afterKey?: CursorValue[];
  /**
   * Read the whole filtered table instead of one page: the paging frame
   * (`LIMIT`/`OFFSET` and the seek predicate) is dropped.
   *
   * The filtered full-table export streams this statement — the export and
   * the grid read the same builder, so the exported rows are the rows the
   * filters really select.
   */
  unbounded?: boolean;
}

/**
 * The answer of a SQL preview (DB-15): the statements a read would send.
 *
 * Every field is text the backend assembled — the frontend never builds SQL
 * of its own for the preview, so the preview cannot drift from what runs.
 */
export interface TableReadPreviewSql {
  /** The statement that reads the rows (paged, or unbounded on request). */
  dataSql: string;
  /** The statement that counts the rows, when one runs with the read. */
  countSql: string | null;
  /**
   * The exact `COUNT(*)` of the filtered table, always present — an
   * estimate falls back to this, and the preview shows both facts.
   */
  exactCountSql: string;
  /** How the count would be read: `exact`, `estimated` or `unsupported`. */
  countKind: CountStrategy;
}
