/**
 * The Session domain client (form-split F3).
 *
 * The editor-session lifecycle: open, read, re-context, attach/detach, close.
 * Executing *inside* a session belongs to the Execution domain; what this
 * domain owns is the session resource itself and its context revision.
 */

import type { BackendGuard } from './guard';
import type { BackendTransport } from './transport';
import type {
  AttachmentRequest,
  CloseReceipt,
  CloseSessionRequest,
  ContextChangeReceipt,
  OpenSessionReceipt,
  OpenSessionRequest,
  SessionHandle,
  SessionView,
  SetSessionContextRequest,
  SubmitRequest,
  SubmissionToken,
} from './types';

/** The Session slice of `MethodMap`. */
export interface SessionMethodMap {
  openSession: { request: OpenSessionRequest & SubmitRequest; response: OpenSessionReceipt };
  getSession: { request: { handle: SessionHandle }; response: SessionView };
  setSessionContext: {
    request: SetSessionContextRequest & SubmitRequest;
    response: ContextChangeReceipt;
  };
  attachSession: { request: AttachmentRequest; response: SessionView };
  detachSession: { request: AttachmentRequest; response: SessionView };
  closeSession: { request: CloseSessionRequest; response: CloseReceipt };
}

/** The Session domain client surface. */
export interface SessionClient {
  openSession(
    request: OpenSessionRequest,
    submissionToken: SubmissionToken,
  ): Promise<OpenSessionReceipt>;
  getSession(handle: SessionHandle): Promise<SessionView>;
  setSessionContext(
    request: SetSessionContextRequest,
    token: SubmissionToken,
  ): Promise<ContextChangeReceipt>;
  attachSession(request: AttachmentRequest): Promise<SessionView>;
  detachSession(request: AttachmentRequest): Promise<SessionView>;
  closeSession(request: CloseSessionRequest): Promise<CloseReceipt>;
}

/** Build the Session domain client over a transport. */
export function createSessionMethods(
  transport: BackendTransport,
  guard: BackendGuard,
): SessionClient {
  return {
    openSession: (request, submissionToken) =>
      guard('openSession', () => transport.call('openSession', { ...request, submissionToken })),

    getSession: (handle) => guard('getSession', () => transport.call('getSession', { handle })),

    setSessionContext: (request, submissionToken) =>
      guard('setSessionContext', () =>
        transport.call('setSessionContext', { ...request, submissionToken }),
      ),

    attachSession: (request) =>
      guard('attachSession', () => transport.call('attachSession', request)),
    detachSession: (request) =>
      guard('detachSession', () => transport.call('detachSession', request)),

    closeSession: (request) => guard('closeSession', () => transport.call('closeSession', request)),
  };
}

/** Compile-time: every Session map key is served by this domain client. */
type Assert<T extends true> = T;
export type SessionClientCoversMap = Assert<
  keyof SessionMethodMap extends keyof SessionClient ? true : false
>;
