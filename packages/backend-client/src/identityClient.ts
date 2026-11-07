/**
 * The Identity domain client (form-split F3).
 *
 * Two methods that establish *who is asking*: the platform identity (which
 * client instance / organization / principal this frontend runs as) and the
 * submission tokens that make mutating requests idempotent. Both are
 * prerequisites for any guarded write, which is why they form a domain of
 * their own rather than dangling off Session or Job.
 */

import type { BackendGuard } from './guard';
import type { BackendTransport } from './transport';
import type { OwnerRef, SessionHandle, SubmissionToken } from './types';

/** The Identity slice of `MethodMap`. */
export interface IdentityMethodMap {
  getPlatformIdentity: {
    request: undefined;
    response: { clientInstanceId: string; organizationId: string; principalId: string };
  };
  issueSubmissionToken: {
    request: {
      operation: string;
      handle: SessionHandle | null;
      connectionId?: string;
      owner?: OwnerRef;
    };
    response: SubmissionToken;
  };
}

/** The Identity domain client surface. */
export interface IdentityClient {
  getPlatformIdentity(): Promise<{
    clientInstanceId: string;
    organizationId: string;
    principalId: string;
  }>;
  issueSubmissionToken(
    operation: string,
    handle: SessionHandle | null,
    scope?: { connectionId: string; owner: OwnerRef },
  ): Promise<SubmissionToken>;
}

/** Build the Identity domain client over a transport. */
export function createIdentityMethods(
  transport: BackendTransport,
  guard: BackendGuard,
): IdentityClient {
  return {
    getPlatformIdentity: () =>
      guard('getPlatformIdentity', () => transport.call('getPlatformIdentity', undefined)),

    issueSubmissionToken: (operation, handle, scope) =>
      guard('issueSubmissionToken', () =>
        transport.call('issueSubmissionToken', { operation, handle, ...scope }),
      ),
  };
}

/** Compile-time: every Identity map key is served by this domain client. */
type Assert<T extends true> = T;
export type IdentityClientCoversMap = Assert<
  keyof IdentityMethodMap extends keyof IdentityClient ? true : false
>;
