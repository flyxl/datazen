/**
 * The Connection domain client (form-split F3).
 *
 * The connection profile CRUD: list, create, update, disable. It is the F4
 * demonstration domain for the reserved kernel commands
 * (`platform_create/update/disable_profile`) — until the frontend switches
 * onto them, this client delegates to the map keys exactly as the facade
 * always did, and the desktop transport maps them to whatever command name
 * the host registers today.
 */

import type { BackendGuard } from './guard';
import type { BackendTransport } from './transport';
import type {
  Id,
  ProfileDraft,
  ProfilePatch,
  ProfileView,
  SubmitRequest,
  SubmissionToken,
} from './types';

/** The Connection slice of `MethodMap`. */
export interface ConnectionMethodMap {
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
}

/** The Connection domain client surface. */
export interface ConnectionClient {
  listConnections(): Promise<readonly ProfileView[]>;
  createConnection(draft: ProfileDraft, submissionToken: SubmissionToken): Promise<ProfileView>;
  updateConnection(
    connectionId: Id,
    expectedRevision: number,
    patch: ProfilePatch,
  ): Promise<ProfileView>;
  disableConnection(connectionId: Id, expectedRevision: number): Promise<ProfileView>;
}

/** Build the Connection domain client over a transport. */
export function createConnectionMethods(
  transport: BackendTransport,
  guard: BackendGuard,
): ConnectionClient {
  return {
    listConnections: () =>
      guard('listConnections', () => transport.call('listConnections', undefined)),

    createConnection: (draft, submissionToken) =>
      guard('createConnection', () =>
        transport.call('createConnection', { ...draft, submissionToken }),
      ),

    updateConnection: (connectionId, expectedRevision, patch) =>
      guard('updateConnection', () =>
        transport.call('updateConnection', { connectionId, expectedRevision, patch }),
      ),

    disableConnection: (connectionId, expectedRevision) =>
      guard('disableConnection', () =>
        transport.call('disableConnection', { connectionId, expectedRevision }),
      ),
  };
}

/** Compile-time: every Connection map key is served by this domain client. */
type Assert<T extends true> = T;
export type ConnectionClientCoversMap = Assert<
  keyof ConnectionMethodMap extends keyof ConnectionClient ? true : false
>;
