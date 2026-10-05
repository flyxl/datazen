/**
 * `@datazen/backend-client` — the transport-agnostic frontend boundary.
 *
 * Consumers import from here and get the whole surface: base types, the
 * facade, the binding interfaces, the stream abstraction and `ApiError`.
 *
 * What is deliberately absent is any notion of *how* a backend is reached. No
 * desktop module specifier, no network primitive (architecture guard F-07
 * enforces this over this whole package, tests included). An adapter binds the
 * transport; everything above this line is backend-agnostic.
 */

export type { ApiErrorCode, ApiErrorOptions, ApiErrorPayload, RetryDisposition } from './errors';
export { ApiError, API_ERROR_CODES, defaultRetryDisposition, deserializeApiError } from './errors';

export type { ConnectionEventStream, EventStream, EventStreamSource } from './streams';
export { createEventStream, toEventStream } from './streams';

export type {
  BackendTransport,
  CommandResult,
  MethodMap,
  OpenDirectoryInput,
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
  parseCancelReceipt,
  parseJobView,
  selectBackend,
  setBackendClient,
  useBackendClient,
} from './client';

export * from './types';
