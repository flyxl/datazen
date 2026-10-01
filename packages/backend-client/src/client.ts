/**
 * `BackendClient` — the typed facade every frontend consumer should hold.
 *
 * The method list is the authoritative one from
 * `docs/architecture/platform/system-overview.md` §6.2. Each method is a thin,
 * typed delegation to the bound `BackendTransport`, so this file contains
 * routing and no logic: no SQL, no database-type branching, no transport
 * mechanism. That is the point of the facade — a consumer that needs a new
 * capability asks the backend for it and never learns which backend it is
 * talking to.
 *
 * Backend selection happens *here*, by binding a different instance under a
 * different `backendId` (§7.3). It never happens by passing an id to a method.
 */

import { ApiError, deserializeApiError } from './errors';
import { createEventStream, toEventStream, type ConnectionEventStream } from './streams';
import type { BackendTransport } from './transport';
import type {
  ArtifactChunk,
  ArtifactReadRequest,
  AttachmentRequest,
  CancelReceipt,
  CloseReceipt,
  CloseSessionRequest,
  ContextChangeReceipt,
  ExecuteAtTargetRequest,
  ExecuteInSessionRequest,
  ExecutionReceipt,
  ExecutionView,
  Id,
  JobView,
  OpenSessionReceipt,
  OpenSessionRequest,
  ProfileDraft,
  ProfilePatch,
  ProfileView,
  SessionHandle,
  SessionView,
  SetSessionContextRequest,
  SubmissionToken,
  SubscribeEventsRequest,
} from './types';

export interface BackendClient {
  /** Which backend this facade is bound to. Never a business-method argument. */
  readonly backendId: string;

  /**
   * The binding this facade delegates to.
   *
   * Exposed only for the transitional `packages/driver-sdk` re-exports, which
   * still speak the legacy driver-command gateway. Consumers call the methods
   * below; this goes away with that gateway.
   */
  readonly transport: BackendTransport;

  listConnections(): Promise<readonly ProfileView[]>;
  createConnection(draft: ProfileDraft, submissionToken: SubmissionToken): Promise<ProfileView>;
  updateConnection(connectionId: Id, expectedRevision: number, patch: ProfilePatch): Promise<ProfileView>;
  disableConnection(connectionId: Id, expectedRevision: number): Promise<ProfileView>;

  openSession(request: OpenSessionRequest, submissionToken: SubmissionToken): Promise<OpenSessionReceipt>;
  getSession(handle: SessionHandle): Promise<SessionView>;
  executeInSession(request: ExecuteInSessionRequest, token: SubmissionToken): Promise<ExecutionReceipt>;
  executeAtTarget(request: ExecuteAtTargetRequest, token: SubmissionToken): Promise<ExecutionReceipt>;
  setSessionContext(request: SetSessionContextRequest, token: SubmissionToken): Promise<ContextChangeReceipt>;
  attachSession(request: AttachmentRequest): Promise<SessionView>;
  detachSession(request: AttachmentRequest): Promise<SessionView>;
  closeSession(request: CloseSessionRequest): Promise<CloseReceipt>;

  getExecution(executionId: Id): Promise<ExecutionView>;
  /**
   * Ask the backend to cancel an execution. This is the *only* path that may
   * cancel SQL: ending the event subscription is not cancelling, and keeping
   * them in separate types is what keeps that true.
   */
  cancelExecution(executionId: Id, token: SubmissionToken): Promise<CancelReceipt>;

  /**
   * Subscribe to connection events.
   *
   * The handle is owned by this facade. Breaking out of the iteration ends the
   * subscription locally (§7.3) and leaves any running execution alone.
   */
  subscribeEvents(request: SubscribeEventsRequest): ConnectionEventStream;

  startJob(definition: Record<string, unknown>, token: SubmissionToken): Promise<JobView>;
  listJobs(filter: Record<string, unknown>): Promise<readonly JobView[]>;
  getJob(jobId: Id): Promise<JobView>;
  cancelJob(jobId: Id, token: SubmissionToken): Promise<CancelReceipt>;

  readArtifact(request: ArtifactReadRequest): Promise<ArtifactChunk>;
  issueSubmissionToken(operation: string, handle: SessionHandle | null): Promise<SubmissionToken>;
}

/**
 * Build a facade over a transport.
 *
 * Every method delegates to `transport.call` with its `MethodMap` key, and
 * every rejection passes through `asApiError` so an adapter that throws a raw
 * object cannot leak an unrecognized error type to consumers.
 */
export function createBackendClient(backendId: string, transport: BackendTransport): BackendClient {
  /** Delegation plus error normalization, applied to every kernel method. */
  const guard = async <T>(method: string, run: () => Promise<T>): Promise<T> => {
    try {
      return await run();
    } catch (error) {
      throw asApiError(error, method);
    }
  };

  return {
    backendId,
    transport,

    listConnections: () => guard('listConnections', () => transport.call('listConnections', undefined)),

    createConnection: (draft, submissionToken) =>
      guard('createConnection', () => transport.call('createConnection', { ...draft, submissionToken })),

    updateConnection: (connectionId, expectedRevision, patch) =>
      guard('updateConnection', () => transport.call('updateConnection', { connectionId, expectedRevision, patch })),

    disableConnection: (connectionId, expectedRevision) =>
      guard('disableConnection', () => transport.call('disableConnection', { connectionId, expectedRevision })),

    openSession: (request, submissionToken) =>
      guard('openSession', () => transport.call('openSession', { ...request, submissionToken })),

    getSession: (handle) => guard('getSession', () => transport.call('getSession', { handle })),

    executeInSession: (request, submissionToken) =>
      guard('executeInSession', () => transport.call('executeInSession', { ...request, submissionToken })),

    executeAtTarget: (request, submissionToken) =>
      guard('executeAtTarget', () => transport.call('executeAtTarget', { ...request, submissionToken })),

    setSessionContext: (request, submissionToken) =>
      guard('setSessionContext', () => transport.call('setSessionContext', { ...request, submissionToken })),

    attachSession: (request) => guard('attachSession', () => transport.call('attachSession', request)),
    detachSession: (request) => guard('detachSession', () => transport.call('detachSession', request)),

    closeSession: (request) => guard('closeSession', () => transport.call('closeSession', request)),

    getExecution: (executionId) => guard('getExecution', () => transport.call('getExecution', { executionId })),

    cancelExecution: (executionId, submissionToken) =>
      guard('cancelExecution', () => transport.call('cancelExecution', { executionId, submissionToken })),

    subscribeEvents: (request) => {
      const { subscribe } = transport;
      if (!subscribe) {
        // An adapter without stream support fails on *use*, not on bind, and
        // with a code from the §13 table rather than a bare TypeError.
        return createEventStream<never>(
          {
            next: () =>
              Promise.reject(
                new ApiError('CapabilityUnsupported', 'Event subscription is not supported by this backend.'),
              ),
            release: () => undefined,
          },
          request.streamId,
        );
      }
      return toEventStream(subscribe.call(transport, 'subscribeEvents', request), request.streamId);
    },

    startJob: (definition, submissionToken) =>
      guard('startJob', () => transport.call('startJob', { ...definition, submissionToken })),

    listJobs: (filter) => guard('listJobs', () => transport.call('listJobs', filter)),

    getJob: (jobId) => guard('getJob', () => transport.call('getJob', { jobId })),

    cancelJob: (jobId, submissionToken) =>
      guard('cancelJob', () => transport.call('cancelJob', { jobId, submissionToken })),

    readArtifact: (request) => guard('readArtifact', () => transport.call('readArtifact', request)),

    issueSubmissionToken: (operation, handle) =>
      guard('issueSubmissionToken', () => transport.call('issueSubmissionToken', { operation, handle })),
  };
}

/**
 * Normalize anything thrown inside a facade method into an `ApiError`.
 *
 * A well-formed `ApiError` passes through untouched — re-wrapping it would
 * discard its `requestId`, which is the one field that makes a user-visible
 * failure traceable to a backend log line.
 */
function asApiError(error: unknown, method: string): ApiError {
  if (error instanceof ApiError) return error;
  const apiError = deserializeApiError(error);
  if (apiError.code === 'ServiceUnavailable' && !apiError.requestId) {
    return new ApiError(apiError.code, `${method} failed: ${apiError.message}`, { cause: error });
  }
  return apiError;
}

/**
 * The binding registry (§7.3, §7.4).
 *
 * `backendId` selects a facade; it is never a business-method argument. The app
 * entry binds before first paint and afterwards only swaps the selection.
 */
const clientsByBackend = new Map<string, BackendClient>();
let selectedBackendId: string | null = null;

/**
 * Bind (or rebind) one backend's facade.
 *
 * The *first* binding also selects it, because the single-backend app entry
 * calls this once and then expects lookups to resolve. Later bindings only
 * register: switching between already-bound backends is `selectBackend`'s job,
 * so binding a second facade must not silently steal the current selection.
 */
export function setBackendClient(backendId: string, client: BackendClient): void {
  clientsByBackend.set(backendId, client);
  if (selectedBackendId === null) selectedBackendId = backendId;
}

/** Choose which bound facade subsequent lookups resolve to. */
export function selectBackend(backendId: string): void {
  selectedBackendId = backendId;
}

/** The selected facade, or `null` when nothing resolves. */
export function getBackendClient(): BackendClient | null {
  if (selectedBackendId === null) return null;
  return clientsByBackend.get(selectedBackendId) ?? null;
}

/** Which backend is selected, or `null`. */
export function getSelectedBackendId(): string | null {
  return selectedBackendId;
}

/** Whether a facade is currently resolvable. */
export function isBackendClientBound(): boolean {
  return getBackendClient() !== null;
}

/** Drop every binding. Exists so tests can exercise the unbound path. */
export function clearBackendClients(): void {
  clientsByBackend.clear();
  selectedBackendId = null;
}

/**
 * Resolve the selected facade, or throw a locatable error.
 *
 * §7.4's rule: never fall back to a direct connection when nothing is bound. A
 * silent fallback is exactly how a desktop-only path survives a failed
 * injection and fails later, somewhere unrelated. Throwing names the missing
 * binding instead.
 */
export function useBackendClient(): BackendClient {
  const client = getBackendClient();
  if (!client) {
    throw new Error('BackendClient has not been bound; check that the platform adapter ran.');
  }
  return client;
}