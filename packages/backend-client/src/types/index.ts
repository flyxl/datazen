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
  ArtifactLifecycle,
  ArtifactMetadata,
  ArtifactMetadataReadRequest,
  ArtifactReadRequest,
  ArtifactUploadEnvelope,
  ArtifactUploadReceipt,
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
  SearchTableValuesRequest,
  SearchTableValuesResult,
  ValueSearchMode,
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
export * from './driverCommands';

// WR-F1：wire-direct 域客户端共用的最小 HTTP wire（注入式 fetch，包内无网络
// 原语字面量——F-07；工厂与 §13 错误归一在此，域语义在 mcp/security 客户端）。
export type {
  HttpWire,
  HttpWireMethod,
  HttpWireOptions,
  HttpWireRequest,
} from './httpWire';
export {
  apiErrorFromWireResponse,
  createHttpWire,
  jsonBodyOfWireResponse,
} from './httpWire';
