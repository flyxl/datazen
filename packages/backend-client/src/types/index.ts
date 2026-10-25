/**
 * Barrel for the mirrored backend DTOs.
 *
 * Consumers should import from here (`@datazen/backend-client`) rather than
 * reaching into `./identity` or `./session`, so that splitting a module later
 * does not ripple through call sites.
 */

export type { Counter, Id, Timestamp } from './identity';
export { toCounter, toId, toTimestamp } from './identity';

export type { CountStrategy } from './tableData';

export type {
  CursorValue,
  KeysetUnavailableReason,
  OrderColumn,
  PagePagingInfo,
  PagingMode,
} from './tableData';

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
  JobDetails,
  JobDomainResult,
  JobProgress,
  JobRecoveryResult,
  JobRecoveryTarget,
  JobRecoveryVerdict,
  JobState,
  JobView,
  ProfileDraft,
  ProfilePatch,
  ProfileView,
  ResultCompleteness,
} from './jobs';
export { emptyJobProgress } from './jobs';

export type {
  ChangeWarning,
  CommitOperation,
  CommitPendingChangesRequest,
  CommitPendingChangesResponse,
  CommitStatementResult,
  DeleteNamedViewRequest,
  EstimateTableRowsRequest,
  FilterCondition,
  ForeignKeyTargetRef,
  ForeignKeyTargetResolution,
  InboundForeignKeyCatalog,
  InboundForeignKeyRef,
  ListInboundForeignKeysRequest,
  ListNamedViewsRequest,
  NamedView,
  NamedViewScope,
  PendingRowChange,
  PlannedStatement,
  PreviewPendingChangesRequest,
  ResolveForeignKeyTargetsRequest,
  ResultCellWrite,
  RowChangePlan,
  RowIdentity,
  SaveNamedViewRequest,
  SortCondition,
  TableChangeContext,
  TableColumnSchema,
  TableDataRequest,
  TableDataResult,
  TableReadPreviewRequest,
  TableReadPreviewSql,
  TableValue,
} from './tableData';
export { toTableValue } from './tableData';

export type {
  GridReadOnlyReason,
  IdentityColumn,
  ResolveResultOriginRequest,
  ResultCellChangeRequest,
  ResultEditability,
  ResultTableRef,
} from './resultOrigin';
