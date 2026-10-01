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
  EventEnvelope,
  ExecuteAtTargetRequest,
  ExecuteInSessionRequest,
  ExecutionReceipt,
  ExecutionView,
  JobView,
  OpenSessionReceipt,
  OpenSessionRequest,
  ProfileDraft,
  ProfilePatch,
  ProfileView,
  SessionHandle,
  SessionView,
  SubmitRequest,
  SubscribeEventsRequest,
  SubmissionToken,
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
  executeInSession: { request: ExecuteInSessionRequest & SubmitRequest; response: ExecutionReceipt };
  executeAtTarget: { request: ExecuteAtTargetRequest & SubmitRequest; response: ExecutionReceipt };
  setSessionContext: { request: SubmitRequest; response: ContextChangeReceipt };
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
    request: { operation: string; handle: SessionHandle | null };
    response: SubmissionToken;
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