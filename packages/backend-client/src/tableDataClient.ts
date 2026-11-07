/**
 * The table-data half of the `BackendClient` facade.
 *
 * It lives in its own module because it is a distinct domain: the read, the
 * change plan and the commit of a table are one contract with one set of wire
 * shapes (`./types/tableData`), and keeping them together lets a reader of
 * "the web table-data port" find the whole surface in one place. The facade
 * itself composes the two halves — `BackendClient` extends this interface —
 * so a consumer sees one object either way.
 *
 * Every method here is a typed delegation to the bound transport. There is no
 * logic in this file: no SQL, no dialect branching, no transport mechanism.
 * The one decision it does make is the *capability* check, and that decision
 * belongs to the caller of `createBackendClient`, not here: this module is
 * handed a `guard` and uses it like every other method does.
 */

import type { BackendTransport } from './transport';
import type { BackendGuard } from './guard';
import type {
  CommitPendingChangesRequest,
  CommitPendingChangesResponse,
  DeleteNamedViewRequest,
  EstimateTableRowsRequest,
  ForeignKeyTargetResolution,
  InboundForeignKeyCatalog,
  ListInboundForeignKeysRequest,
  SearchTableValuesRequest,
  SearchTableValuesResult,
  ListNamedViewsRequest,
  NamedView,
  PreviewPendingChangesRequest,
  ResolveForeignKeyTargetsRequest,
  ResolveResultOriginRequest,
  ResultCellChangeRequest,
  ResultCellWrite,
  ResultEditability,
  RowChangePlan,
  SaveNamedViewRequest,
  TableDataRequest,
  TableDataResult,
  TableReadPreviewRequest,
  TableReadPreviewSql,
} from './types';

/**
 * The delegation wrapper the facade shares with every method.
 *
 * It normalizes a rejection into an `ApiError` and refuses a method the bound
 * backend declared it cannot serve.
 *
 * Form-split F3: the canonical definition moved to `./guard`; this re-export
 * keeps the pre-F3 import path (`tableDataClient`) working for the desktop
 * adapter and any deep importer.
 */
export type { BackendGuard };

/**
 * The TableData slice of `MethodMap` (form-split F3).
 *
 * Every method here is a typed delegation to the bound transport. There is no
 * logic in this file: no SQL, no dialect branching, no transport mechanism.
 * The one decision it does make is the *capability* check, and that decision
 * belongs to the caller of the factory, not here: this module is handed a
 * `guard` and uses it like every other method does.
 */
export interface TableDataMethodMap {
  /**
   * A target-bound table read.
   *
   * The read never borrows an editor's session: it runs against the explicit
   * target named by the context, or against the session a `manualHandle`
   * pins.
   */
  getTableData: { request: TableDataRequest; response: TableDataResult };
  /**
   * The SQL preview of a table read (DB-15).
   *
   * The backend assembles through the same code a real read executes, so the
   * statements it returns are the statements the product would send — the
   * frontend never builds preview SQL of its own.
   */
  previewTableReadSql: {
    request: TableReadPreviewRequest;
    response: TableReadPreviewSql;
  };
  /**
   * The change preview. It produces a serialisable plan and executes nothing:
   * the backend renders SQL, the frontend never does.
   */
  previewPendingChanges: {
    request: PreviewPendingChangesRequest;
    response: RowChangePlan;
  };
  /**
   * The change commit. It applies the exact plan the preview returned and is
   * guarded by that plan's fingerprint, so a stale preview cannot be applied.
   */
  commitPendingChanges: {
    request: CommitPendingChangesRequest;
    response: CommitPendingChangesResponse;
  };
  /**
   * The estimated row count. `null` means "this driver cannot estimate", which
   * is an answer and not an error; the caller decides whether to fall back to
   * an exact count.
   */
  estimateTableRows: {
    request: EstimateTableRowsRequest;
    response: number | null;
  };
  /**
   * The foreign-key resolution: which table a local column points at, and
   * whether the referenced row still exists.
   */
  resolveForeignKeyTargets: {
    request: ResolveForeignKeyTargetsRequest;
    response: ForeignKeyTargetResolution;
  };
  listInboundForeignKeys: {
    request: ListInboundForeignKeysRequest;
    response: InboundForeignKeyCatalog;
  };
  searchTableValues: {
    request: SearchTableValuesRequest;
    response: SearchTableValuesResult;
  };
  /**
   * The named views of one scope (or of every scope when `scope` is omitted).
   *
   * The three named-view commands are **desktop-only** for now: their storage
   * is the desktop host's persistent store, which a browser build has no
   * counterpart for. They are declared here so the port stays complete, and a
   * backend that cannot serve them declares that in
   * `BackendTransport.unsupportedMethods` — the caller then gets an explicit
   * "unsupported" failure instead of an empty list that looks like "you have
   * no saved views".
   */
  listNamedViews: { request: ListNamedViewsRequest; response: readonly NamedView[] };
  saveNamedView: { request: SaveNamedViewRequest; response: NamedView };
  deleteNamedView: { request: DeleteNamedViewRequest; response: readonly NamedView[] };
  /**
   * The write of one edited cell of a query result, bound to the runtime
   * resource that produced the result.
   */
  commitQueryResultCell: { request: ResultCellWrite; response: undefined };
  /**
   * Where a query result came from, and whether it may be written back.
   *
   * The verdict is computed by the backend because only the driver knows the
   * provenance of the rows; the interface renders it and never recomputes it.
   */
  resolveResultOrigin: {
    request: ResolveResultOriginRequest;
    response: ResultEditability;
  };
  /**
   * The change plan for one edited cell of a query result.
   *
   * The preview produces a serialisable plan and executes nothing: the
   * backend renders the statement, the frontend never does.
   */
  previewResultCellChange: {
    request: ResultCellChangeRequest;
    response: RowChangePlan;
  };
}

/** The table-data methods of the `BackendClient` facade. */
export interface TableDataClient {
  /**
   * Read one page of a table.
   *
   * The read is target-bound: it runs against the table named by the request's
   * context, or against the session a manual handle pins. It never borrows an
   * editor's session, so a grid read cannot be steered by an unrelated query.
   */
  getTableData(request: TableDataRequest): Promise<TableDataResult>;
  /**
   * Show the statements a table read would send, without sending them
   * (DB-15).
   *
   * The statements are assembled by the same backend code a real read
   * executes through — the schema round trip, the filter validation and the
   * paging decision are all real — so the preview is what the product sends,
   * not a description of it. An `unbounded` request drops the paging frame and
   * is what the filtered full-table export streams.
   */
  previewTableReadSql(request: TableReadPreviewRequest): Promise<TableReadPreviewSql>;
  /**
   * Preview a staged change set.
   *
   * The preview produces a serialisable plan and executes nothing. The plan's
   * fingerprint is what `commitPendingChanges` is guarded by, so a preview
   * that is not committed is only ever a preview.
   */
  previewPendingChanges(request: PreviewPendingChangesRequest): Promise<RowChangePlan>;
  /**
   * Commit the exact plan a preview returned.
   *
   * The fingerprint travels with the request and is re-checked by the
   * backend; a plan that no longer matches is refused rather than applied.
   */
  commitPendingChanges(request: CommitPendingChangesRequest): Promise<CommitPendingChangesResponse>;
  /**
   * Estimate the row count of a table.
   *
   * `null` means "this driver cannot estimate" — an answer, not a failure.
   * Deciding to fall back to an exact count is the caller's business, not
   * this method's.
   */
  estimateTableRows(request: EstimateTableRowsRequest): Promise<number | null>;
  /**
   * Resolve the foreign key a local column points at, and whether the
   * referenced row still exists.
   */
  resolveForeignKeyTargets(
    request: ResolveForeignKeyTargetsRequest,
  ): Promise<ForeignKeyTargetResolution>;
  /** The reverse foreign-key catalog: the constraints of other tables that reference one table. */
  listInboundForeignKeys(request: ListInboundForeignKeysRequest): Promise<InboundForeignKeyCatalog>;
  /** Count the rows of one table whose searchable columns match a value (DB-19). */
  searchTableValues(request: SearchTableValuesRequest): Promise<SearchTableValuesResult>;
  /**
   * The named views of one scope, or of every scope when `scope` is omitted.
   *
   * These three commands are desktop-only for now: their storage is the
   * desktop host's persistent store, which a browser build has no counterpart
   * for. A backend that cannot serve them declares that on its transport, and
   * the call fails with `CapabilityUnsupported` — never with an empty list
   * that reads like "you have no saved views".
   */
  listNamedViews(request: ListNamedViewsRequest): Promise<readonly NamedView[]>;
  /** Save a named view; the backend mints the id and stamps the timestamps. */
  saveNamedView(request: SaveNamedViewRequest): Promise<NamedView>;
  /** Delete a named view; the remaining views come back with the receipt. */
  deleteNamedView(request: DeleteNamedViewRequest): Promise<readonly NamedView[]>;
  /**
   * Write one edited cell of a query result.
   *
   * The write carries the runtime binding of the result it edits, so a result
   * whose resource is gone is refused instead of being written through some
   * other handle.
   */
  commitQueryResultCell(request: ResultCellWrite): Promise<void>;
  /**
   * Ask where a query result came from, and whether it may be written back.
   *
   * The verdict is the backend's answer — only the driver knows the
   * provenance of the rows — so the interface renders it and never
   * recomputes it.
   */
  resolveResultOrigin(request: ResolveResultOriginRequest): Promise<ResultEditability>;
  /**
   * The change plan for one edited cell of a query result.
   *
   * The preview produces a serialisable plan and executes nothing.
   */
  previewResultCellChange(request: ResultCellChangeRequest): Promise<RowChangePlan>;
}

/** Build the table-data half of a facade over a transport. */
export function createTableDataMethods(
  transport: BackendTransport,
  guard: BackendGuard,
): TableDataClient {
  return {
    getTableData: (request) => guard('getTableData', () => transport.call('getTableData', request)),

    previewTableReadSql: (request) =>
      guard('previewTableReadSql', () => transport.call('previewTableReadSql', request)),

    previewPendingChanges: (request) =>
      guard('previewPendingChanges', () => transport.call('previewPendingChanges', request)),

    commitPendingChanges: (request) =>
      guard('commitPendingChanges', () => transport.call('commitPendingChanges', request)),

    estimateTableRows: (request) =>
      guard('estimateTableRows', () => transport.call('estimateTableRows', request)),

    resolveForeignKeyTargets: (request) =>
      guard('resolveForeignKeyTargets', () => transport.call('resolveForeignKeyTargets', request)),

    listInboundForeignKeys: (request) =>
      guard('listInboundForeignKeys', () => transport.call('listInboundForeignKeys', request)),

    searchTableValues: (request) =>
      guard('searchTableValues', () => transport.call('searchTableValues', request)),

    listNamedViews: (request) =>
      guard('listNamedViews', () => transport.call('listNamedViews', request)),

    saveNamedView: (request) =>
      guard('saveNamedView', () => transport.call('saveNamedView', request)),

    deleteNamedView: (request) =>
      guard('deleteNamedView', () => transport.call('deleteNamedView', request)),

    commitQueryResultCell: (request) =>
      guard('commitQueryResultCell', async () => {
        await transport.call('commitQueryResultCell', request);
      }),

    resolveResultOrigin: (request) =>
      guard('resolveResultOrigin', () => transport.call('resolveResultOrigin', request)),

    previewResultCellChange: (request) =>
      guard('previewResultCellChange', () => transport.call('previewResultCellChange', request)),
  };
}

/** Compile-time: every TableData map key is served by this domain client. */
type Assert<T extends true> = T;
export type TableDataClientCoversMap = Assert<
  keyof TableDataMethodMap extends keyof TableDataClient ? true : false
>;
