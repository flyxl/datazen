/**
 * The delegation guard every facade method passes through.
 *
 * Two obligations, applied uniformly to every domain client built over a
 * `BackendTransport`:
 *
 * 1. **Capability honesty.** A method the bound backend declared it cannot
 *    serve (§7.1 `unsupportedMethods`) fails here, on *use* rather than on
 *    bind, with the §13 code `CapabilityUnsupported` — never a bare TypeError,
 *    never a plausible-looking empty result.
 * 2. **Error normalization.** A rejection of any shape becomes an `ApiError`,
 *    so an adapter that throws a raw object cannot leak an unrecognized error
 *    type to consumers.
 *
 * Extracted from `client.ts` in form-split F3 so the domain-client composition
 * root (`clients.ts`) can share the exact same guard the facade used — a
 * domain client and the facade must not disagree about what a gap or a
 * malformed rejection looks like.
 */

import { ApiError, deserializeApiError } from './errors';
import type { MethodMap } from './transport';

/**
 * Delegation plus error normalization, applied to every method.
 *
 * The type is re-exported from `./tableDataClient` for compatibility with the
 * pre-F3 import path.
 */
export type BackendGuard = <T>(method: keyof MethodMap, run: () => Promise<T>) => Promise<T>;

/** What a transport declares about the methods it cannot serve. */
export type UnsupportedMethods = Partial<
  Record<keyof MethodMap, import('./errors').UnsupportedMethodKind>
>;

/**
 * Build the guard for one bound backend.
 *
 * `backendId` is used only in the unsupported-method wording, so the error
 * names the backend that declined — the one fact a caller cannot otherwise
 * know.
 */
export function createBackendGuard(
  backendId: string,
  unsupportedMethods: UnsupportedMethods,
): BackendGuard {
  return async <T>(method: keyof MethodMap, run: () => Promise<T>): Promise<T> => {
    const kind = unsupportedMethods[method];
    if (kind !== undefined) {
      throw new ApiError(
        'CapabilityUnsupported',
        kind === 'capability'
          ? `${method} is not supported by the ${backendId} backend.`
          : `${method} is not implemented by the ${backendId} backend yet.`,
        { unsupportedKind: kind },
      );
    }
    try {
      return await run();
    } catch (error) {
      throw asApiError(error, method);
    }
  };
}

/**
 * Normalize anything thrown inside a facade method into an `ApiError`.
 *
 * A well-formed `ApiError` passes through untouched — re-wrapping it would
 * discard its `requestId`, which is the one field that makes a user-visible
 * failure traceable to a backend log line.
 */
function asApiError(error: unknown, method: string): ApiError {
  if (error instanceof ApiError) return error;
  const apiError = deserializeApiError(error);
  if (apiError.code === 'ServiceUnavailable' && !apiError.requestId) {
    return new ApiError(apiError.code, `${method} failed: ${apiError.message}`, { cause: error });
  }
  return apiError;
}
