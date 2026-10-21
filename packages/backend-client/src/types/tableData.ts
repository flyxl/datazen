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
 * in `packages/feature-core/src/data_browsing`). This file is that mirror's
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
  page: number;
  pageSize: number;
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
  /** RFC3339. */
  updatedAt: string;
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
