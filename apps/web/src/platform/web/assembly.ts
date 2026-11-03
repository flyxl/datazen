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
 *   CSV pages and downloads through the anchor half (`downloadBlob`);
 * - WR-E4: the static table drives the assembly's downgrades — `updater: false`
 *   binds **nothing** (no updater slot exists here; the desktop-only item is
 *   structurally absent, `WEB_CAPABILITY_BINDINGS` is the auditable record)
 *   and `versionSource: 'build-constant'` resolves the build constant. The
 *   decision is compile-time table → selector; there is no form probe;
 * - WR-F1: the AI trio (`WebAiRequestHost` / `McpToolBroker` /
 *   SecretProvider, `./ai/webAiAssembly`) assembles over the same
 *   `WebClient` transport; named gaps surface as `CapabilityUnsupported`;
 * - WR-F6: the backup/restore pair (`createWebBackupTransfer`,
 *   `./backupTransfer`) rides the same facade's artifact half — export lands
 *   the payload in the server artifact library through the §6.5 uploads
 *   envelope, restore reads it back through the three §6.5 read modes, and
 *   the browser Blob halves (anchor save / FileReader import) stay local.
 * - WR-F4: the wapp asset service swaps the `datazen://` custom scheme for
 *   the `/wapps/{id}/…` real-origin static convention — the web resolver is
 *   bound here (single-source CSP `WAPP_ASSET_CSP`; the postMessage bridge
 *   protocol itself is unchanged).
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
  configureWappAssetResolver,
  createBlobTablesExporter,
  createBrowserEventChannel,
  inertHostSettingsPort,
  requirePlatform,
  WEB_CAPABILITIES,
  WEB_WAPP_ASSET_RESOLVER,
  type TablePageReader,
} from '@datazen/frontend';
import { createWebBackendTransport, WEB_BACKEND_ID } from './backendTransport';
import { createWebPlatformServices } from './platformServices';
import { createWebWindowRouter } from './windowRouter';
import {
  createWebAiBindings,
  type WebAiBindings,
} from './ai/webAiAssembly';
import {
  createWebBackupTransfer,
  type WebBackupTransfer,
} from './backupTransfer';

export { WEB_CAPABILITY_BINDINGS } from './capabilityBindings';

/** WR-F1 装配产物：AI 三端口（装配期创建一次；测试直接走 `createWebAiBindings`）。 */
export let WEB_AI_BINDINGS: WebAiBindings;

/** WR-F6 装配产物：备份/恢复端口（装配期创建一次；测试直接走 `createWebBackupTransfer`）。 */
export let WEB_BACKUP_TRANSFER: WebBackupTransfer;

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
  // Kernel binding: the facade over the WebClient transport. Every domain
  // call outside the HTTP route map surfaces as an explicit
  // `CapabilityUnsupported` ApiError (contract §4 ledger / §8 exclusions).
  const transport = createWebBackendTransport();
  const client = createBackendClient(WEB_BACKEND_ID, transport);
  setBackendClient(WEB_BACKEND_ID, client);

  // WR-F1: the AI trio rides the same transport — provider HTTP clients run
  // server-side; the browser holds ports and secret *references* only.
  WEB_AI_BINDINGS = createWebAiBindings(client, transport);

  // WR-F6: backup/restore over the same facade's artifact half — the §6.5
  // uploads envelope lands backup payloads in the server artifact library,
  // the three read modes bring them back, and the Blob halves stay local.
  WEB_BACKUP_TRANSFER = createWebBackupTransfer(client);

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
    // Form-split F5 + WR-E4: the static capability table — sections, updater
    // and version source branch on this, never on a form probe. The table's
    // `false` rows have already acted at assembly time (see
    // `WEB_CAPABILITY_BINDINGS`): updater binds nothing, the version is the
    // build constant, and desktop-only items are structurally absent.
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

  // WR-F4: the wapp asset service rides the real origin — bind the
  // `/wapps/{id}/…` origin-relative resolver before the first workspace
  // paint. The desktop assembly performs no such binding: its default IS
  // the `datazen://` custom scheme (desktop path unchanged).
  configureWappAssetResolver(WEB_WAPP_ASSET_RESOLVER);
}
