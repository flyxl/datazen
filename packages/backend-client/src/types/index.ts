/**
 * Barrel for the mirrored backend DTOs.
 *
 * Consumers should import from here (`@datazen/backend-client`) rather than
 * reaching into `./identity` or `./session`, so that splitting a module later
 * does not ripple through call sites.
 */

export type { Counter, Id, Timestamp } from './identity';
export { toCounter, toId, toTimestamp } from './identity';

export type {
  AttachmentRequest,
  AttachmentState,
  CancelReceipt,
  CapabilitySnapshot,
  CloseMode,
  CloseReceipt,
  CloseSessionRequest,
  CommandCall,
  ConnectionEvent,
  ContextChangeReceipt,
  EffectOutcome,
  EventEnvelope,
  ExecuteAtTargetRequest,
  ExecuteInSessionRequest,
  ExecutionErrorCode,
  ExecutionReceipt,
  ExecutionState,
  ExecutionTarget,
  ExecutionView,
  NamespaceTarget,
  ObjectTarget,
  OpenSessionReceipt,
  OpenSessionRequest,
  OwnerRef,
  ResultProvenance,
  RuntimeResultBinding,
  SessionContext,
  SessionHandle,
  SessionState,
  SessionView,
  SetSessionContextRequest,
  StatementResultSource,
  SubmitRequest,
  SubscribeEventsRequest,
  SubmissionToken,
  TransactionHandle,
  TransactionState,
} from './session';

export type {
  ArtifactChunk,
  ArtifactReadRequest,
  CommitBoundary,
  JobProgress,
  JobState,
  JobView,
  ProfileDraft,
  ProfilePatch,
  ProfileView,
  ResultCompleteness,
} from './jobs';
export { emptyJobProgress } from './jobs';
