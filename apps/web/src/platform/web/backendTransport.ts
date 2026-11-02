/**
 * Web backend transport (WR-E1) — the all-throw stub.
 *
 * Mirrors `packages/frontend/src/platform/web/stub.ts`
 * (`createWebBackendTransportStub`), which is the compile-time same-shape
 * proof: a web assembly must typecheck against `BackendTransport` with zero
 * host imports. The stub in the shared package is not publicly exported (the
 * `@datazen/frontend` public-entry gate, `scripts/check-frontend-public-entry.mjs`,
 * forbids deep imports from `apps/*`), so the production web assembly owns
 * this file — the same ownership split the desktop form has
 * (`apps/desktop/src/platform/desktop/tauriBackendTransport.ts`).
 *
 * There is no web backend yet (that is WR-E2's `WebClient` HTTP/SSE
 * transport). Every method throws `CapabilityUnsupported` **at call time**,
 * never at import time: importing and assembling this module is safe, and an
 * unimplemented capability surfaces as an explicit `ApiError` instead of a
 * silent empty answer (§13 semantics; WR-C4's acceptance criterion).
 *
 * This file, and only this file, is where the web form's *absence* of a
 * kernel lives. WR-E2 replaces it and nothing above it.
 */

import { ApiError, type BackendTransport } from '@datazen/backend-client';

/**
 * The `backendId` every web build reports.
 *
 * Same contract as the desktop's `DESKTOP_BACKEND_ID`: a label on the facade,
 * never a business-method argument.
 */
export const WEB_BACKEND_ID = 'web';

export function createWebBackendTransport(): BackendTransport {
  return {
    async call(method) {
      throw new ApiError(
        'CapabilityUnsupported',
        `${String(method)} is not served by the web backend stub (form-split F5 / WR-E1).`,
      );
    },
  };
}
