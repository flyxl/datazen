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

import type { BackendTransport, MethodMap } from './transport';
import type {
  CommitPendingChangesRequest,
  CommitPendingChangesResponse,
  DeleteNamedViewRequest,
  EstimateTableRowsRequest,
  ForeignKeyTargetResolution,
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
} from './types';

/**
 * The delegation wrapper the facade shares with every method.
 *
 * It normalizes a rejection into an `ApiError` and refuses a method the bound
 * backend declared it cannot serve.
 */
export type BackendGuard = <T>(
  method: keyof MethodMap,
  run: () => Promise<T>,
) => Promise<T>;

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
  commitPendingChanges(
    request: CommitPendingChangesRequest,
  ): Promise<CommitPendingChangesResponse>;
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

    previewPendingChanges: (request) =>
      guard('previewPendingChanges', () => transport.call('previewPendingChanges', request)),

    commitPendingChanges: (request) =>
      guard('commitPendingChanges', () => transport.call('commitPendingChanges', request)),

    estimateTableRows: (request) =>
      guard('estimateTableRows', () => transport.call('estimateTableRows', request)),

    resolveForeignKeyTargets: (request) =>
      guard('resolveForeignKeyTargets', () => transport.call('resolveForeignKeyTargets', request)),

    listNamedViews: (request) => guard('listNamedViews', () => transport.call('listNamedViews', request)),

    saveNamedView: (request) => guard('saveNamedView', () => transport.call('saveNamedView', request)),

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
