/**
 * Connection / session / execution DTO mirrors.
 *
 * Field-for-field copies of the frontend contract in
 * `docs/architecture/platform/connection-management.md` §4.1 and
 * `docs/architecture/platform/system-overview.md` §6.2. They are read-only
 * views of backend state: the frontend never constructs them by hand, it only
 * forwards what an adapter decoded.
 *
 * The invariant that matters most here: a `dbSessionId` is a runtime,
 * in-memory session id that is never persisted, and a `connectionId` is the
 * persistent configuration id. Keeping them as distinct `Id` usages inside a
 * named handle is what stops the two from being swapped.
 */

import type { Counter, Id, Timestamp } from './identity';

/** Database / catalog / schema resolution, normalized per §4.3. */
export interface NamespaceTarget {
  database: string | null;
  catalog: string | null;
  schema: string | null;
  path: readonly string[];
}

/** A single addressable object inside a namespace (table, view, ...). */
export interface ObjectTarget {
  kind: string;
  name: string;
  signature: string | null;
}

/**
 * What an execution runs *against*.
 *
 * Note what is absent: `backendId`. The backend only selects which facade you
 * hold, it is never part of an execution target.
 */
export interface ExecutionTarget {
  connectionId: Id;
  namespace: NamespaceTarget;
  object: ObjectTarget | null;
}

/**
 * A session handle pairs the session with the runtime epoch it was bound to.
 *
 * After an epoch change the old handle is dead — that is why it is a pair and
 * not a bare id.
 */
export interface SessionHandle {
  dbSessionId: Id;
  runtimeEpoch: Id;
}

/** Who owns a session. `job` and `workflowBlock` owners outlive any window. */
export type OwnerRef =
  | { kind: 'editor'; clientInstanceId: Id; editorSessionId: Id }
  | { kind: 'job'; jobId: Id; stageId: Id }
  | { kind: 'workflowBlock'; jobId: Id; blockId: Id }
  | { kind: 'clientSession'; clientInstanceId: Id; purpose: string };

export type TransactionState = 'none' | 'active' | 'aborted' | 'unknown' | 'unsupported';

/**
 * The context an operation actually ran in.
 *
 * `confidence` exists because a driver that cannot observe the batch-accurate
 * context reports `partial`/`unknown` rather than guessing; the UI reads it to
 * decide whether a result may be treated as writable.
 */
export interface SessionContext {
  namespace: NamespaceTarget;
  searchPath: readonly string[] | null;
  effectiveIdentity: string | null;
  transactionState: TransactionState;
  autocommit: boolean | null;
  confidence: 'confirmed' | 'partial' | 'unknown';
}

export type AttachmentState = 'attached' | 'detached' | 'expired';

export type SessionState =
  | 'new'
  | 'opening'
  | 'ready'
  | 'executing'
  | 'reconfiguring'
  | 'closing'
  | 'closed'
  | 'lost';

export interface CapabilitySnapshot {
  driverId: string;
  driverVersion: string;
  protocolVersion: number;
  capabilityRevision: Counter;
  confirmed: Readonly<Record<string, unknown>>;
}

/** Result provenance for the whole call, before/after, not per statement. */
export interface ResultProvenance {
  organizationId: Id;
  principalId: Id;
  connectionId: Id;
  configRevision: Counter;
  contextBefore: SessionContext;
  contextAfter: SessionContext;
  requestedTarget: ExecutionTarget;
  capabilitySnapshot: CapabilitySnapshot;
  executedAt: Timestamp;
}

export interface RuntimeResultBinding {
  handle: SessionHandle;
  resourceBindingId: Id;
  executionId: Id;
}

export interface SessionView {
  handle: SessionHandle;
  connectionId: Id;
  configRevision: Counter;
  owner: OwnerRef;
  initialTarget: ExecutionTarget;
  observedContext: SessionContext;
  contextRevision: Counter;
  state: SessionState;
  attachmentState: AttachmentState;
  activeExecutionId: Id | null;
  expiresAt: Timestamp | null;
}

export type ExecutionState =
  | 'queued'
  | 'running'
  | 'cancelRequested'
  | 'succeeded'
  | 'failed'
  | 'cancelled';

/**
 * Terminal execution error code.
 *
 * This is a namespace of its own, deliberately disjoint from `ApiError.code`:
 * an execution that *ran* and failed reports one of these, while a request
 * rejected before an execution record existed surfaces an `ApiError`.
 */
export type ExecutionErrorCode =
  | 'sqlError'
  | 'protocolError'
  | 'cancelled'
  | 'timeout'
  | 'resourceLost'
  | 'pipelineAborted'
  | 'hostRejected';

export type EffectOutcome =
  | 'notStarted'
  | 'completed'
  | 'rolledBack'
  | 'partiallyApplied'
  | 'unknown';

export interface ExecutionView {
  executionId: Id;
  state: ExecutionState;
  effectOutcome: EffectOutcome;
  provenance: ResultProvenance | null;
  artifactIds: readonly Id[];
  resultCompleteness: 'pending' | 'complete' | 'truncated';
  truncationReason: string | null;
  errorCode: ExecutionErrorCode | null;
  runtimeBinding: RuntimeResultBinding | null;
}

export type CloseMode = 'requireNoTransaction' | 'rollbackAndClose';

export interface OpenSessionReceipt {
  session: SessionView;
  attachmentToken: string;
}

export interface ContextChangeReceipt {
  session: SessionView;
  replacedSessionId: Id | null;
  attachmentToken: string | null;
}

export interface CloseReceipt {
  dbSessionId: Id;
  state: 'closing' | 'closed' | 'lost';
  effectOutcome: EffectOutcome;
  resourceRelease: 'pending' | 'confirmed' | 'quarantined';
}

export interface CancelReceipt {
  executionId: Id;
  disposition: 'requested' | 'unsupported' | 'alreadyFinished';
  state: ExecutionState;
}

/** Acceptance receipt. It does *not* mean the statement succeeded. */
export interface ExecutionReceipt {
  executionId: Id;
  streamId: Id;
  state: ExecutionState;
}

export interface SubmissionToken {
  idempotencyKey: string;
  expiresAt: Timestamp;
}

/** Session-level resource handles (transaction / cursor / prepared stmt). */
export interface TransactionHandle {
  handleId: Id;
  kind: 'transaction' | 'cursor' | 'serverPrepared';
  resourceId: Id;
  runtimeEpoch: Counter;
  closed: boolean;
}

export interface OpenSessionRequest {
  initialTarget: ExecutionTarget;
  owner: OwnerRef;
  idempotencyKey: string;
}

/**
 * A single driver command to run.
 *
 * `input` stays `unknown`: command schemas belong to the driver crate, and the
 * gateway validates them against the driver API before anything executes. The
 * frontend's job is to not second-guess that shape.
 */
export interface CommandCall {
  command: string;
  input: unknown;
}

/**
 * The signed submission token that every mutating call carries.
 *
 * §13.1: the client obtains the token from the application service, it does
 * not invent an idempotency key. Presenting it is what binds the request to an
 * identity, an operation and an expiry, and what makes a retry deduplicate
 * instead of executing twice.
 */
export interface SubmitRequest {
  submissionToken: SubmissionToken;
}

export interface ExecuteInSessionRequest {
  handle: SessionHandle;
  expectedContextRevision: Counter | null;
  call: CommandCall;
  idempotencyKey: string;
}

export interface ExecuteAtTargetRequest {
  target: ExecutionTarget;
  expectedConfigRevision: Counter;
  call: CommandCall;
  idempotencyKey: string;
}

export interface AttachmentRequest {
  handle: SessionHandle;
  attachmentToken: string;
}

export interface SetSessionContextRequest {
  handle: SessionHandle;
  expectedContextRevision: Counter;
  desired: NamespaceTarget;
  idempotencyKey: string;
}

export interface CloseSessionRequest {
  handle: SessionHandle;
  mode: CloseMode;
}

/**
 * Where one statement's result came from.
 *
 * Two results from the same script must not both carry the script's *final*
 * context, so provenance is recorded per statement rather than per execution.
 */
export interface StatementResultSource {
  executionId: Id;
  statementIndex: number;
  context: SessionContext;
  relation: ExecutionTarget | null;
  writableMapping: 'verified' | 'readOnly';
}

export type ConnectionEvent =
  | { kind: 'sessionChanged'; session: SessionView }
  | { kind: 'executionChanged'; execution: ExecutionView }
  | { kind: 'resultChunk'; artifactId: Id; chunkIndex: Counter; source: StatementResultSource }
  | { kind: 'streamResetRequired'; reason: string };

/**
 * One delivery on a subscription.
 *
 * `sequence` is what makes resumption possible: a client that drops a
 * subscription reconnects with `afterSequence` rather than replaying blind.
 */
export interface EventEnvelope<T> {
  streamId: Id;
  sequence: Counter;
  runtimeEpoch: Id;
  executionId: Id | null;
  jobId: Id | null;
  sessionHandle: SessionHandle | null;
  contextRevision: Counter | null;
  payload: T;
}

export interface SubscribeEventsRequest {
  streamId: Id;
  afterSequence: Counter | null;
}
