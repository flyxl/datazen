/**
 * Transitional desktop binding for the driver-sdk IPC surface.
 *
 * §7.2 requires the SDK's `invoke`/`Channel` calls to be routed through the
 * bound `BackendClient` and `PlatformServices`, with the original export paths
 * kept as thin re-exports so no call site changes. That leaves one question
 * this module answers: what the SDK does when nothing has been injected yet.
 *
 * It binds the desktop adapter lazily, on first use, and only when no explicit
 * binding exists. Explicit injection always wins, because `getBackendClient()`
 * and `isPlatformServicesBound()` are checked first — so the app entry's
 * `setBackendClient` / `setPlatformServices` (§7.3) take effect normally and
 * these paths only cover the transition window.
 *
 * Lazy rather than at import time on purpose: this package declares
 * `"sideEffects": false`, and binding on import would make that false. It would
 * also mean that merely importing the SDK reaches for the desktop adapter, so a
 * web build would break at module load rather than at the call it cannot serve.
 *
 * This is strictly a migration aid and deliberately narrow in scope. The strict
 * contract is unchanged and still enforced: `useBackendClient()` and
 * `requirePlatformServices()` throw when nothing is bound, which is what a *new*
 * frontend bridge should get. What is exempt is only these pre-existing SDK
 * re-exports, whose callers cannot all be switched over in one step — which is
 * the whole reason §7.2 calls for a transition at all.
 *
 * When every consumer injects explicitly, this module goes away and the SDK's
 * re-exports call `useBackendClient()` / `requirePlatformServices()` directly.
 */

import {
  getBackendClient,
  isPlatformServicesBound,
  requirePlatformServices,
  setPlatformServices,
  type BackendTransport,
  type PlatformServices,
} from '@datazen/backend-client';
import {
  bindDesktopBackend,
  createDesktopPlatformServices,
} from '../../../../src/platform/tauriBackendTransport';

/**
 * Resolve the transport the SDK's IPC re-exports should use.
 *
 * Throws the §7.4 unbound error rather than reaching for `invoke` directly, so a
 * binding that genuinely cannot be established is reported as a missing binding
 * instead of silently working on the desktop build only.
 */
export function transitionalTransport(): BackendTransport {
  const bound = getBackendClient();
  if (bound) return bound.transport;
  return bindDesktopBackend().transport;
}

/** Resolve host capabilities the SDK's file re-exports should use. */
export function transitionalPlatformServices(): PlatformServices {
  if (isPlatformServicesBound()) return requirePlatformServices();

  setPlatformServices(createDesktopPlatformServices());
  return requirePlatformServices();
}
