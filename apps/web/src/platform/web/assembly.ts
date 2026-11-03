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
 * - `tablesExporter` binds the shared Blob half (`createBlobTablesExporter`)
 *   over a paged `getTableData` reader (WR-E3): the same export request the
 *   desktop answers with the Rust-streaming dialog writer assembles here into
 *   CSV pages and downloads through the anchor half (`downloadBlob`).
 */

import { bindHostEventListener } from '@datazen/driver-sdk';
import {
  createBackendClient,
  setBackendClient,
  setPlatformServices,
  type BackendClient,
  type CursorValue,
} from '@datazen/backend-client';
import {
  configurePlatform,
  createBlobTablesExporter,
  createBrowserEventChannel,
  inertHostSettingsPort,
  requirePlatform,
  WEB_CAPABILITIES,
  type TablePageReader,
} from '@datazen/frontend';
import { createWebBackendTransport, WEB_BACKEND_ID } from './backendTransport';
import { createWebPlatformServices } from './platformServices';
import { createWebWindowRouter } from './windowRouter';

/**
 * Rows per `getTableData` page while assembling an export. A short page ends
 * the read (the backend's own paging contract: the caller never derives a
 * boundary, it only stops when the page comes back under the asked size).
 */
export const WEB_EXPORT_PAGE_SIZE = 1000;

/**
 * The export reader over the web facade's `getTableData` (WR-E3). The cursor
 * is the reader's own `{ page, afterKey }` state: page 1 first, then the next
 * page — forwarding the backend's `nextAfterKey` when keyset paging reported
 * one, verbatim (the caller never derives a boundary).
 */
function createWebTablePageReader(client: BackendClient): TablePageReader {
  return async (tableName, cursor, context) => {
    const state = (cursor ?? null) as { page: number; afterKey?: CursorValue[] } | null;
    const page = state?.page ?? 1;
    const result = await client.getTableData({
      context: {
        connectionId: null,
        dbSessionId: context.dbSessionId,
        driverType: context.driverType,
        database: context.database,
        schema: context.schema,
        table: tableName,
      },
      page,
      pageSize: WEB_EXPORT_PAGE_SIZE,
      skipCount: true,
      ...(state?.afterKey ? { afterKey: state.afterKey } : {}),
    });
    const afterKey = result.paging?.nextAfterKey;
    return {
      columns: result.columns.map((column) => column.name),
      rows: result.rows,
      // Short page ⇒ exhausted; a full page continues (with the backend's
      // keyset boundary when it reported one).
      nextCursor:
        result.rows.length < WEB_EXPORT_PAGE_SIZE
          ? null
          : { page: page + 1, ...(afterKey ? { afterKey } : {}) },
    };
  };
}

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
    // WR-E3: the export half — the same `exportTables(request)` signature the
    // desktop answers with the Rust-streaming dialog writer, assembled here
    // through paged `getTableData` reads and delivered as a Blob download.
    tablesExporter: createBlobTablesExporter(createWebTablePageReader(client)),
  });

  // Form-split F4 (R8 retirement): driver UI listens to host broadcasts
  // through the EventChannel port via the driver-sdk bridge — the same
  // wiring the desktop assembly performs; the driver layer never imports
  // the host runtime.
  bindHostEventListener((event, handler) =>
    requirePlatform().events.listenCrossWindow(event, handler),
  );
}
