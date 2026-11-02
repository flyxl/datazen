/**
 * Web assembly root (WR-E1) — the web form's counterpart of
 * `apps/desktop/src/shell/desktop/assembly.ts`.
 *
 * Runs before the first React paint: binds the all-throw web backend
 * transport (WR-E2's HTTP/SSE `WebClient` replaces it and nothing above),
 * injects `PlatformServices` into the backend-client registry, configures
 * the platform ports with the web halves and binds the driver-UI host-event
 * rail. Semantics follow `packages/frontend/src/platform/web/stub.ts`
 * (`assembleWebPlatformStub`), the compile-time same-shape proof this
 * assembly realizes in production.
 *
 * Why this is NOT passed to `startSharedApp` as `configureHost`: the shared
 * bootstrap invokes `configureHost` only under `IS_DESKTOP_BUILD` — a
 * compile-time constant that folds to `false` in this build (no
 * `TAURI_ENV_*` envPrefix here), so the call site is dead code in a web
 * bundle. The web entry therefore runs the assembly itself, before
 * `startSharedApp()` — same ordering contract (ports bound before the
 * settings preload / first paint).
 *
 * Differences from the desktop assembly, all by form:
 * - no backend: the transport throws `CapabilityUnsupported` on every call
 *   (an explicit gap, never a silent empty answer — §13 / §7.4);
 * - `hostSettings` binds the inert port (the F4 web shape; the kernel save
 *   is the whole write);
 * - `capabilities` binds the static `WEB_CAPABILITIES` table (no updater,
 *   build-constant version source — consumers branch on the table, never on
 *   a probe);
 * - no `registerWindowChrome`: the browser ships its own chrome; the shared
 *   slot's default (null) is the documented web form (`windowChrome.ts`);
 * - `tablesExporter` is left UNBOUND on purpose: the Blob half needs a real
 *   table page reader (WR-E3), and `requireTablesExporter()` fails loudly at
 *   the user action instead of pretending an export exists.
 */

import { bindHostEventListener } from '@datazen/driver-sdk';
import {
  createBackendClient,
  setBackendClient,
  setPlatformServices,
} from '@datazen/backend-client';
import {
  configurePlatform,
  createBrowserEventChannel,
  inertHostSettingsPort,
  requirePlatform,
  WEB_CAPABILITIES,
} from '@datazen/frontend';
import { createWebBackendTransport, WEB_BACKEND_ID } from './backendTransport';
import { createWebPlatformServices } from './platformServices';
import { createWebWindowRouter } from './windowRouter';

/** Bind backend + inject ports + bind the host-event rail. Idempotent per boot. */
export function assembleWebPlatform(): void {
  // Kernel binding: the facade over the all-throw transport. Every domain
  // call surfaces as an explicit `CapabilityUnsupported` ApiError until
  // WR-E2 binds the HTTP transport under the same port.
  const client = createBackendClient(WEB_BACKEND_ID, createWebBackendTransport());
  setBackendClient(WEB_BACKEND_ID, client);

  const services = createWebPlatformServices();
  setPlatformServices(services);

  configurePlatform({
    services,
    windows: createWebWindowRouter(),
    // Cross-window events = cross-TAB events in a browser: the shared
    // BroadcastChannel rail (the same one the desktop bundle binds in
    // browser-dev mode).
    events: createBrowserEventChannel(),
    // Form-split F4: the inert host-settings half — extras `{}`, writes are
    // a no-op, the kernel save is the whole write.
    hostSettings: inertHostSettingsPort(),
    // Form-split F5: the static capability table — sections, updater and
    // version source branch on this, never on a form probe.
    capabilities: WEB_CAPABILITIES,
  });

  // Form-split F4 (R8 retirement): driver UI listens to host broadcasts
  // through the EventChannel port via the driver-sdk bridge — the same
  // wiring the desktop assembly performs; the driver layer never imports
  // the host runtime.
  bindHostEventListener((event, handler) =>
    requirePlatform().events.listenCrossWindow(event, handler),
  );
}
