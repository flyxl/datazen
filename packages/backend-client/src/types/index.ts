/**
 * `@datazen/backend-client` 的契约词汇总出口。
 *
 * 与 Rust 侧 `packages/platform-api/src/dto/**` + `packages/application/src/dto/requests.rs`
 * 对应。前端一律从 `@datazen/backend-client` 导入，**不要深入具体文件**：
 * 字段重命名时只需改一处，跨文件的相对路径 import 也天然被杜绝。
 *
 * 边界：这里只有**形状**，没有传输、没有宿主、没有 React。传输在 `../transport`，
 * 门面在 `../client`，错误类型在 `../errors`。
 */

export type { Counter, Id, IsoTimestamp, JsonObject, JsonValue, Timestamp } from './ids';
export { compareCounters, incrementCounter, isCounter, parseCounter } from './ids';

export type {
  CanonicalNamespace,
  CanonicalNamespaceId,
  CanonicalTarget,
  ExecutionTarget,
  LayerRequirement,
  NamespaceLayer,
  NamespaceTarget,
  ObjectTarget,
} from './target';
export {
  EMPTY_NAMESPACE,
  emptyNamespaceTarget,
  hasNamespaceValue,
  isWellFormedNamespace,
  namespaceFingerprint,
  namespaceValue,
  objectSignature,
  sameNamespace,
} from './target';

export type { DelegationRef, OwnerRef, RequestContext } from './context';
export { isClientBoundOwner, isDelegated, ownerJobId } from './context';

export type { SessionHandle } from './session';
export type {
  AttachmentState,
  CancelDisposition,
  CancelReceipt,
  CloseReceipt,
  ContextChangeReceipt,
  ContextConfidence,
  OpenSessionReceipt,
  ResourceRelease,
  SessionContext,
  SessionState,
  SessionView,
  TransactionState,
} from './session';
export { isSameSession, isSessionUsable, sessionDefaultsUsable } from './session';

export type {
  CapabilitySnapshot,
  EffectOutcome,
  ExecutionErrorCode,
  ExecutionReceipt,
  ExecutionState,
  ExecutionView,
  ResultCompleteness,
  ResultProvenance,
  RuntimeResultBinding,
  StatementResultSource,
  WritableMapping,
} from './execution';
export {
  confirmedCapability,
  describeExecutionFailure,
  hasLiveRuntime,
  isTerminalState,
} from './execution';

export type { JobDefinition, JobFilter, JobState, JobView, StageRecord } from './job';
export { isJobSettled, isTerminalJobState, jobArtifactIds, jobStageLabel } from './job';

export type { ProfileDraft, ProfilePatch, ProfileView } from './profile';
export {
  emptyProfilePatch,
  hasCredentials,
  isConfigRevisionStale,
  profileDraft,
  touchesCredentials,
  withoutCredentials,
} from './profile';

export type { ArtifactChunk, ArtifactProvenance, ArtifactSummary } from './artifact';
export { chunkByteLength, chunkEndOffset, decodeChunk, sliceChunk } from './artifact';

export type {
  ArchivedEnvelope,
  ConnectionEvent,
  EventEnvelope,
  EventSequence,
  ResultChunkEvent,
  StreamResetRequiredEvent,
} from './event';
export { archiveEvent, belongsToEpoch, isContiguousAfter, requiresResubscribe } from './event';

export type { IdempotentOperation, SubmissionToken } from './idempotency';
export { bindsRuntimeEpoch, requiresSessionHandle } from './idempotency';

export type {
  AttachmentRequest,
  CloseMode,
  CloseSessionRequest,
  CommandCall,
  ExecuteAtTargetRequest,
  ExecuteInSessionRequest,
  IdempotencyKey,
  IssueSubmissionTokenRequest,
  OpenSessionRequest,
  ReadArtifactRequest,
  SetSessionContextRequest,
  StartJobRequest,
  SubscribeEventsRequest,
} from './requests';
export { commandCall } from './requests';
