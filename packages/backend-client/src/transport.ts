import type { SharedSettings } from './settings';
import type { UnsupportedMethodKind } from './errors';
/**
 * The two binding interfaces the host must satisfy, and the registry that
 * hands them out.
 *
 * `BackendTransport` and `PlatformServices` are reproduced from
 * `docs/architecture/platform/shared-boundaries-and-ports.md` §7.1. They
 * declare *method names and payload shapes only* — no transport module
 * specifier, no network primitive literal (architecture guard F-07 scans this
 * whole package and fails the build if one appears). An adapter supplies the
 * mechanism; nothing here knows how any backend is reached.
 *
 * The distinction the two interfaces encode is deliberate and not
 * interchangeable: `BackendTransport` reaches the *kernel* (connections,
 * sessions, executions, jobs, artifacts); `PlatformServices` reaches the *host*
 * (native file dialogs, clipboard). Only a desktop adapter can implement the
 * latter, and a browser build is expected to bind a narrower one.
 */

import type {
  ArtifactChunk,
  ArtifactReadRequest,
  AttachmentRequest,
  CancelReceipt,
  CloseReceipt,
  CloseSessionRequest,
  ConnectionEvent,
  ContextChangeReceipt,
  DeleteNamedViewRequest,
  EstimateTableRowsRequest,
  EventEnvelope,
  ExecuteAtTargetRequest,
  ExecuteInSessionRequest,
  ExecutionReceipt,
  ExecutionView,
  ForeignKeyTargetResolution,
  JobView,
  ListNamedViewsRequest,
  NamedView,
  OpenSessionReceipt,
  OpenSessionRequest,
  PreviewPendingChangesRequest,
  ProfileDraft,
  ProfilePatch,
  ProfileView,
  ResolveForeignKeyTargetsRequest,
  ResolveResultOriginRequest,
  ResultCellChangeRequest,
  ResultCellWrite,
  ResultEditability,
  RowChangePlan,
  SaveNamedViewRequest,
  SessionHandle,
  SessionView,
  SetSessionContextRequest,
  OwnerRef,
  SubmitRequest,
  SubscribeEventsRequest,
  SubmissionToken,
  TableDataRequest,
  TableDataResult,
  TableReadPreviewRequest,
  TableReadPreviewSql,
  CommitPendingChangesRequest,
  CommitPendingChangesResponse,
} from './types';

/** Legacy gateway envelope. Kept because the `{ data }` unwrap is a public contract. */
export interface CommandResult {
  data: unknown;
}

/**
 * Every method a `BackendTransport` may be asked to perform, with its request
 * and response types.
 *
 * Two groups, and the difference matters:
 *
 * - **Kernel surface** — keys are exactly the method names of `BackendClient`
 *   (`system-overview.md` §6.2). These are the platform's real API.
 * - **Legacy driver-command gateway** — keys are the *existing* desktop IPC
 *   command names (`execute_driver_command`, ...). They exist today because
 *   `packages/driver-sdk` predates the kernel and calls them directly; they are
 *   here only so the SDK can be migrated without changing a single call site,
 *   and they go away when the kernel implements those methods.
 */
export interface MethodMap {
  getSettings: { request: undefined; response: SharedSettings };
  saveSettings: { request: { settings: SharedSettings }; response: SharedSettings };

  getPlatformIdentity: {
    request: undefined;
    response: { clientInstanceId: string; organizationId: string; principalId: string };
  };
  // ---- Kernel surface (§6.2) ---------------------------------------------
  listConnections: { request: undefined; response: readonly ProfileView[] };
  createConnection: { request: ProfileDraft & SubmitRequest; response: ProfileView };
  updateConnection: {
    request: { connectionId: string; expectedRevision: number; patch: ProfilePatch };
    response: ProfileView;
  };
  disableConnection: {
    request: { connectionId: string; expectedRevision: number };
    response: ProfileView;
  };
  openSession: { request: OpenSessionRequest & SubmitRequest; response: OpenSessionReceipt };
  getSession: { request: { handle: SessionHandle }; response: SessionView };
  executeInSession: {
    request: ExecuteInSessionRequest & SubmitRequest;
    response: ExecutionReceipt;
  };
  executeAtTarget: { request: ExecuteAtTargetRequest & SubmitRequest; response: ExecutionReceipt };
  setSessionContext: {
    request: SetSessionContextRequest & SubmitRequest;
    response: ContextChangeReceipt;
  };
  attachSession: { request: AttachmentRequest; response: SessionView };
  detachSession: { request: AttachmentRequest; response: SessionView };
  closeSession: { request: CloseSessionRequest; response: CloseReceipt };
  getExecution: { request: { executionId: string }; response: ExecutionView };
  cancelExecution: { request: SubmitRequest & { executionId: string }; response: CancelReceipt };
  startJob: { request: Record<string, unknown> & SubmitRequest; response: JobView };
  listJobs: { request: Record<string, unknown>; response: readonly JobView[] };
  getJob: { request: { jobId: string }; response: JobView };
  cancelJob: { request: SubmitRequest & { jobId: string }; response: CancelReceipt };
  readArtifact: { request: ArtifactReadRequest; response: ArtifactChunk };
  issueSubmissionToken: {
    request: {
      operation: string;
      handle: SessionHandle | null;
      connectionId?: string;
      owner?: OwnerRef;
    };
    response: SubmissionToken;
  };

  // ---- Table data surface (data browsing) --------------------------------
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
  /**
   * The named views of one scope (or of every scope when `scope` is omitted).
   *
   * The three named-view commands are **desktop-only** for now: their storage
   * is the desktop host's persistent store, which a browser build has no
   * counterpart for. They are declared here so the port stays complete, and a
   * backend that cannot serve them declares that in
   * {@link BackendTransport.unsupportedMethods} — the caller then gets an
   * explicit "unsupported" failure instead of an empty list that looks like
   * "you have no saved views".
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

  // ---- Legacy driver-command gateway (transitional) ------------------------
  get_driver_commands: { request: { driverType: string }; response: unknown[] };
  get_connection_commands: { request: { dbSessionId: string }; response: unknown[] };
  execute_driver_command: { request: unknown; response: CommandResult };
  /**
   * The streaming gateway. It takes a push callback rather than a subscription
   * handle because that is what the current backend offers, and this contract
   * must describe what exists: the `AsyncIterable` form belongs on
   * `BackendTransport.subscribe`, which a backend implements once it can push
   * an ordered event stream instead of a bare callback.
   */
  execute_driver_command_stream: {
    request: {
      /** The nested driver-command request, forwarded untouched. */
      request: unknown;
      /**
       * The adapter's own push handle — a channel on the desktop, a WebSocket
       * on the web — passed through opaquely. Typing it concretely here would
       * name a transport, and this package names none.
       */
      onEvent: unknown;
      applyResultLimit?: boolean;
      recordHistory?: boolean;
    };
    response: undefined;
  };
  /**
   * Binary open has no `PlatformServices` counterpart: §7.1 declares
   * text-open and directory-open only. The legacy command therefore stays on
   * the transport until the spec adds an open-binary capability.
   */
  open_base64_with_dialog: { request: Record<string, unknown>; response: unknown };
}

/**
 * Bound by the host to exactly one backend.
 *
 * `call` is generic over `MethodMap`, so a method name that does not exist
 * cannot be compiled — including a `backendId` smuggled in as a business
 * parameter. The backend is chosen by holding a different instance, never by
 * passing an argument.
 */
export interface BackendTransport {
  call<K extends keyof MethodMap>(
    method: K,
    payload: MethodMap[K]['request'],
  ): Promise<MethodMap[K]['response']>;
  /** Stream transport adapted to AsyncIterable; ending it only cancels the
   * subscription. */
  subscribe?(
    method: 'subscribeEvents',
    payload: SubscribeEventsRequest,
  ): AsyncIterable<EventEnvelope<ConnectionEvent>>;
  cancel?(opaque: string, reason: string): Promise<void>;
  /**
   * The methods this backend cannot serve, and why.
   *
   * A capability that belongs to the *host* and not to the backend — the
   * desktop's persistent store, for instance — does not exist in every form
   * factor. Declaring the gap here is what keeps the port honest: the facade
   * turns a call to one of these methods into an `ApiError` with the §13 code
   * `CapabilityUnsupported` **before** the call leaves the process, instead of
   * letting it travel and come back as an opaque transport failure or — worse
   * — as a plausible-looking empty result.
   *
   * The value says *which kind* of gap it is, because the two call for
   * different reactions from a caller:
   *
   * - `capability` — this form factor has no way to serve the method, now or
   *   later. A browser build has no host store, so it can never serve the
   *   saved views. A caller hides the feature.
   * - `notImplemented` — the backend could serve the method and does not yet.
   *   The desktop has not registered the 07/08/09 commands. A caller may say
   *   "not available yet" instead of hiding the feature.
   *
   * Absent means "every method in `MethodMap` is served". It is a capability
   * statement, not a platform identity: a backend that *can* serve named views
   * says so by leaving the field out, whatever form factor it runs in.
   *
   * Note what this field is **not**: it is not the list of methods a backend
   * happens to lack today. Whether a method is *registered* in `MethodMap` is
   * a question of contract completeness — the port lists every method the
   * product has, so that a method landing later cannot slip past the port
   * unnoticed — while whether a particular backend *serves* one is declared
   * here, by that backend.
   */
  unsupportedMethods?: Partial<Record<keyof MethodMap, UnsupportedMethodKind>>;
}

/**
 * Host capabilities, injected separately from the backend.
 *
 * A `false`/`null` result means the *user cancelled*. That is a normal outcome
 * and resolves rather than rejects, so callers can `if (!saved)` without a
 * try/catch.
 */
export interface PlatformServices {
  saveTextWithDialog(input: SaveTextInput): Promise<boolean>;
  saveBinaryWithDialog(input: SaveBinaryInput): Promise<boolean>;
  openTextWithDialog(input: OpenTextInput): Promise<OpenedTextFile | null>;
  openDirectoryWithDialog(input: OpenDirectoryInput): Promise<OpenedDirectory | null>;
  writeClipboard(text: string): Promise<void>;
  readClipboard(): Promise<string>;
}

export interface SaveTextInput {
  contents: string;
  defaultFileName: string;
  filterName: string;
  extensions: readonly string[];
}

export interface SaveBinaryInput {
  dataBase64: string;
  defaultFileName: string;
  filterName: string;
  extensions: readonly string[];
}

export interface OpenTextInput {
  filterName: string;
  extensions: readonly string[];
}

export interface OpenDirectoryInput {
  title?: string;
}

/** Result of a text open. Never carries a host path. */
export interface OpenedTextFile {
  fileName: string;
  content: string;
}

/**
 * The one place a host path legitimately exists.
 *
 * `PlatformServices` is the only boundary that may surface a path, because it
 * is the only one that already had one; every artifact read stays path-free by
 * design.
 */
export interface OpenedDirectory {
  path: string;
}

let boundPlatformServices: PlatformServices | null = null;

/**
 * Install host capabilities. Called by the app entry before first paint.
 *
 * Re-binding replaces the previous instance, which is what the desktop adapter
 * needs when it rebuilds itself across a backend switch.
 */
export function setPlatformServices(services: PlatformServices): void {
  boundPlatformServices = services;
}

/** Drop the binding. Exists so tests can exercise the unbound path. */
export function clearPlatformServices(): void {
  boundPlatformServices = null;
}

/** Whether host capabilities are currently bound. */
export function isPlatformServicesBound(): boolean {
  return boundPlatformServices !== null;
}

/**
 * Resolve the bound capabilities, or fail loudly.
 *
 * §7.4 is explicit: an unbound call must throw a locatable error and must
 * never silently fall back to a direct desktop connection. The message names
 * the missing binding, so the stack trace alone points at the adapter that
 * was supposed to run.
 */
export function requirePlatformServices(): PlatformServices {
  if (!boundPlatformServices) {
    throw new Error('PlatformServices has not been bound; check that the platform adapter ran.');
  }
  return boundPlatformServices;
}
