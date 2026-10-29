/**
 * `BackendClient` — the typed facade every frontend consumer should hold.
 *
 * Form-split F3 turned this file from *the implementation* of the backend
 * surface into the *composition* of it: every domain method now lives in its
 * domain client (`settingsClient.ts`, `sessionClient.ts`, ...), built through
 * `createClients`, and this facade is those domains spread onto one object —
 * exactly what a consumer held before, byte-identical in behavior.
 *
 * The facade stays for the transition (revert-safety, plan §5.3: "门面再导出
 * 保兼容，revert 不触碰消费方"). New consumers should prefer the domain
 * clients — `BackendClient` is deprecated as the *primary* import, not
 * removed: existing call sites keep compiling, and F4/F5 migrate them.
 *
 * Backend selection happens *here*, by binding a different instance under a
 * different `backendId` (§7.3). It never happens by passing an id to a method.
 */

import { createBackendGuard } from './guard';
import type { BackendTransport } from './transport';
import { createClients } from './clients';
import type { SettingsClient } from './settingsClient';
import type { IdentityClient } from './identityClient';
import type { ConnectionClient } from './connectionClient';
import type { SessionClient } from './sessionClient';
import type { ExecutionClient } from './executionClient';
import type { JobClient, JobWatchHandle, JobWatchOptions, SubmitJobOptions } from './jobClient';
import type { ArtifactClient } from './artifactClient';
import type { TableDataClient } from './tableDataClient';
import type { EventClient } from './streams';

export type { JobWatchHandle, JobWatchOptions, SubmitJobOptions };
export { parseJobView, parseCancelReceipt } from './jobClient';

/**
 * The facade: every domain client plus the binding identity.
 *
 * @deprecated as the *primary* import — hold the domain client you need
 * (`SettingsClient`, `TableDataClient`, ...) instead. This interface remains
 * the exact composition of all domains for the transition and stays
 * revert-safe.
 */
export interface BackendClient
  extends SettingsClient,
    IdentityClient,
    ConnectionClient,
    SessionClient,
    ExecutionClient,
    EventClient,
    JobClient,
    ArtifactClient,
    TableDataClient {
  /** Which backend this facade is bound to. Never a business-method argument. */
  readonly backendId: string;

  /**
   * The binding this facade delegates to.
   *
   * Exposed only for the transitional `packages/driver-sdk` re-exports, which
   * still speak the legacy driver-command gateway. Consumers call the methods
   * above; this goes away with that gateway.
   */
  readonly transport: BackendTransport;
}

/**
 * Build a facade over a transport.
 *
 * Every method delegates to `transport.call` with its `MethodMap` key through
 * the shared guard, so a capability gap and a malformed rejection behave the
 * same on the facade and on a bare domain client.
 */
export function createBackendClient(backendId: string, transport: BackendTransport): BackendClient {
  const guard = createBackendGuard(backendId, transport.unsupportedMethods ?? {});
  const domains = createClients(transport, guard);

  return {
    backendId,
    transport,
    ...domains.settings,
    ...domains.identity,
    ...domains.connection,
    ...domains.session,
    ...domains.execution,
    ...domains.events,
    ...domains.job,
    ...domains.artifact,
    ...domains.tableData,
  };
}

/**
 * The binding registry (§7.3, §7.4).
 *
 * `backendId` selects a facade; it is never a business-method argument. The app
 * entry binds before first paint and afterwards only swaps the selection.
 */
const clientsByBackend = new Map<string, BackendClient>();
let selectedBackendId: string | null = null;

/**
 * Bind (or rebind) one backend's facade.
 *
 * The *first* binding also selects it, because the single-backend app entry
 * calls this once and then expects lookups to resolve. Later bindings only
 * register: switching between already-bound backends is `selectBackend`'s job,
 * so binding a second facade must not silently steal the current selection.
 */
export function setBackendClient(backendId: string, client: BackendClient): void {
  clientsByBackend.set(backendId, client);
  if (selectedBackendId === null) selectedBackendId = backendId;
}

/** Choose which bound facade subsequent lookups resolve to. */
export function selectBackend(backendId: string): void {
  selectedBackendId = backendId;
}

/** The selected facade, or `null` when nothing resolves. */
export function getBackendClient(): BackendClient | null {
  if (selectedBackendId === null) return null;
  return clientsByBackend.get(selectedBackendId) ?? null;
}

/**
 * The selected facade, or a loud failure (§7.4 semantics).
 *
 * Shared code must never *bind* a host adapter as a fallback — an unbound
 * backend at a data-plane call site is a wiring gap (the assembly root did
 * not run, or ran after the first data-plane consumer), and the §7.4 ruling
 * makes that throw a locatable error instead of silently degrading into a
 * direct host connection. Added by the form-split F6 prep
 * (queryPanelSessionRoot's lazy `bindDesktopBackend()` fallback burn).
 */
export function requireBackendClient(): BackendClient {
  const client = getBackendClient();
  if (!client) {
    throw new Error(
      'BackendClient has not been bound; check that the app assembly root ran before the first data-plane call.',
    );
  }
  return client;
}

/** Which backend is selected, or `null`. */
export function getSelectedBackendId(): string | null {
  return selectedBackendId;
}

/** Whether a facade is currently resolvable. */
export function isBackendClientBound(): boolean {
  return getBackendClient() !== null;
}

/** Drop every binding. Exists so tests can exercise the unbound path. */
export function clearBackendClients(): void {
  clientsByBackend.clear();
  selectedBackendId = null;
}

/**
 * Resolve the selected facade, or throw a locatable error.
 *
 * §7.4's rule: never fall back to a direct connection when nothing is bound. A
 * silent fallback is exactly how a desktop-only path survives a failed
 * injection and fails later, somewhere unrelated. Throwing names the missing
 * binding instead.
 */
export function useBackendClient(): BackendClient {
  const client = getBackendClient();
  if (!client) {
    throw new Error('BackendClient has not been bound; check that the platform adapter ran.');
  }
  return client;
}
