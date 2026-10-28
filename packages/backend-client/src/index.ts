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
  PlatformServices,
  SaveBinaryInput,
  SaveTextInput,
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

export type { ArtifactMethodMap, ArtifactClient } from './artifactClient';
export { createArtifactMethods } from './artifactClient';

export type { TableDataMethodMap, TableDataClient } from './tableDataClient';
export { createTableDataMethods } from './tableDataClient';

export type { DriverGatewayMethodMap, DriverGatewayClient } from './driverGatewayClient';
export { createDriverGatewayMethods } from './driverGatewayClient';

export * from './types';

export * from './settings';
