/**
 * The domain-client composition root (form-split F3).
 *
 * `createClients(transport, guard)` builds every domain client over one
 * transport and hands back the collection. It is what the facade
 * (`client.ts`) composes, and what a consumer that wants *only* a slice of
 * the backend composes itself:
 *
 * ```ts
 * const { tableData } = createClients(transport, guard); // the grid, nothing else
 * ```
 *
 * The point of the split is not file hygiene — it is that the dependency of
 * a window, a store, or a feature module becomes *its domain* rather than
 * the whole backend surface, so the blast radius of a signature change is a
 * domain instead of the app.
 *
 * Domain clients are built through the same `guard` the facade always used,
 * so a capability gap and a malformed rejection behave identically whether
 * the caller holds the facade or the bare domain client.
 */

import type { BackendGuard } from './guard';
import type { BackendTransport, MethodMap } from './transport';
import { createSettingsMethods, type SettingsClient } from './settingsClient';
import { createIdentityMethods, type IdentityClient } from './identityClient';
import { createConnectionMethods, type ConnectionClient } from './connectionClient';
import { createSessionMethods, type SessionClient } from './sessionClient';
import { createExecutionMethods, type ExecutionClient } from './executionClient';
import { createJobMethods, type JobClient } from './jobClient';
import { createArtifactMethods, type ArtifactClient } from './artifactClient';
import { createTableDataMethods, type TableDataClient } from './tableDataClient';
import { createDriverGatewayMethods, type DriverGatewayClient } from './driverGatewayClient';
import { createEventMethods, type EventClient } from './streams';

/** The domain clients of one bound backend. */
export interface DomainClients {
  settings: SettingsClient;
  identity: IdentityClient;
  connection: ConnectionClient;
  session: SessionClient;
  execution: ExecutionClient;
  /** The stream domain: lives in `./streams` per the plan, same shape as the rest. */
  events: EventClient;
  /** The one stateful domain — see `./jobClient` for what it remembers and why. */
  job: JobClient;
  artifact: ArtifactClient;
  tableData: TableDataClient;
  /**
   * The transitional driver-command escape hatch. The facade does not spread
   * it (its snake_case keys are host command names, not future kernel
   * surface); `packages/driver-sdk` migrates onto it directly and the whole
   * domain is deleted once the kernel implements those methods.
   */
  driverGateway: DriverGatewayClient;
}

/** Build every domain client over one transport, sharing one guard. */
export function createClients(transport: BackendTransport, guard: BackendGuard): DomainClients {
  return {
    settings: createSettingsMethods(transport, guard),
    identity: createIdentityMethods(transport, guard),
    connection: createConnectionMethods(transport, guard),
    session: createSessionMethods(transport, guard),
    execution: createExecutionMethods(transport, guard),
    events: createEventMethods(transport),
    job: createJobMethods(transport, guard),
    artifact: createArtifactMethods(transport, guard),
    tableData: createTableDataMethods(transport, guard),
    driverGateway: createDriverGatewayMethods(transport, guard),
  };
}

/* ------------------------------------------------------------------ */
/* Compile-time key-consistency assertions (plan §5.3 exit criterion). */
/* ------------------------------------------------------------------ */

type Assert<T extends true> = T;

/**
 * Every non-gateway `MethodMap` key is a method of the facade.
 *
 * The legacy gateway keys are excluded on purpose: they are addressed by the
 * `driverGateway` domain client, not spread onto the facade. If a domain
 * slice gains a key that no facade method serves, this alias stops
 * satisfying `true` and the build fails — the exact drift the plan's
 * "键一致性编译期断言" asks for.
 */
export type FacadeCoversMethodMap = Assert<
  Exclude<
    keyof MethodMap,
    keyof import('./driverGatewayClient').DriverGatewayMethodMap
  > extends keyof Omit<import('./client').BackendClient, 'backendId' | 'transport'>
    ? true
    : false
>;
