/**
 * `@datazen/backend-client` — the transport-agnostic frontend boundary.
 *
 * Consumers import from here and get the whole surface: base types, the
 * facade, the domain clients, the binding interfaces, the stream abstraction
 * and `ApiError`.
 *
 * What is deliberately absent is any notion of *how* a backend is reached. No
 * desktop module specifier, no network primitive (architecture guard F-07
 * enforces this over this whole package, tests included). An adapter binds the
 * transport; everything above this line is backend-agnostic.
 */

export type {
  ApiErrorCode,
  ApiErrorOptions,
  ApiErrorPayload,
  RetryDisposition,
  UnsupportedMethodKind,
} from './errors';
export { ApiError, API_ERROR_CODES, defaultRetryDisposition, deserializeApiError } from './errors';

export type { ConnectionEventStream, EventClient, EventStream, EventStreamSource } from './streams';
export { createEventStream, createEventMethods, toEventStream } from './streams';

export type {
  BackendTransport,
  CommandResult,
  MethodMap,
  OpenDirectoryInput,
  OpenExternalInput,
  OpenTextInput,
  OpenedDirectory,
  OpenedTextFile,
  OsPlatform,
  PickPathFileFilter,
  PickPathInput,
  PlatformServices,
  SaveBinaryInput,
  SaveTextInput,
  ShowNotificationInput,
  WriteClipboardHtmlInput,
} from './transport';
export {
  clearPlatformServices,
  isPlatformServicesBound,
  requirePlatformServices,
  setPlatformServices,
} from './transport';

export type { BackendClient, JobWatchHandle, JobWatchOptions, SubmitJobOptions } from './client';
export {
  clearBackendClients,
  createBackendClient,
  getBackendClient,
  getSelectedBackendId,
  isBackendClientBound,
  requireBackendClient,
  selectBackend,
  setBackendClient,
  useBackendClient,
} from './client';

// ── Domain clients (form-split F3) ──────────────────────────────────────────
export type { BackendGuard } from './guard';
export { createBackendGuard } from './guard';

export type { DomainClients } from './clients';
export { createClients } from './clients';

export type { SettingsMethodMap, SettingsClient } from './settingsClient';
export { createSettingsMethods } from './settingsClient';

export type { IdentityMethodMap, IdentityClient } from './identityClient';
export { createIdentityMethods } from './identityClient';

export type { ConnectionMethodMap, ConnectionClient } from './connectionClient';
export { createConnectionMethods } from './connectionClient';

export type { SessionMethodMap, SessionClient } from './sessionClient';
export { createSessionMethods } from './sessionClient';

export type { ExecutionMethodMap, ExecutionClient } from './executionClient';
export { createExecutionMethods } from './executionClient';

export type { JobMethodMap, JobClient } from './jobClient';
export { createJobMethods, parseJobView, parseCancelReceipt } from './jobClient';

export type {
  ArtifactMethodMap,
  ArtifactClient,
  ArtifactUploadBeginRequest,
  ArtifactUploadFinalizeOptions,
  ArtifactUploadWriter,
} from './artifactClient';
export {
  createArtifactMethods,
  sha256Hex,
  toBase64,
  UPLOAD_CHUNK_BYTES,
} from './artifactClient';

export type { TableDataMethodMap, TableDataClient } from './tableDataClient';
export { createTableDataMethods } from './tableDataClient';

export type { DriverGatewayMethodMap, DriverGatewayClient } from './driverGatewayClient';
export { createDriverGatewayMethods } from './driverGatewayClient';

export type { LegacyCommandName, LegacyHostMethodMap } from './legacyHostCommands';
export { legacyInvoke } from './legacyHostCommands';

// ── WR-F1 wire-direct domains (mcp / security)；共享 HTTP wire 经下方
// `export * from './types'` 出面（types/httpWire）。──────────────────────────
export type {
  McpClientCoversContract,
  McpContentBlock,
  McpDomainClient,
  McpInitializeOptions,
  McpInitializeResult,
  McpListToolsResult,
  McpToolCallRequest,
  McpToolCallResult,
  McpToolDescriptor,
} from './mcpClient';
export {
  createMcpClient,
  MCP_PROTOCOL_VERSION,
  MCP_SESSION_ID_HEADER,
  WEB_MCP_SERVER_ID,
} from './mcpClient';

export type {
  SecurityClient,
  SecurityClientCoversContract,
  SecurityPutSecretRequest,
  SecurityResolveSecretRequest,
  SecurityResolveSecretResponse,
} from './securityClient';
export { createSecurityClient } from './securityClient';

export type { WireDirectDomains } from './clients';

export * from './types';

export * from './settings';
