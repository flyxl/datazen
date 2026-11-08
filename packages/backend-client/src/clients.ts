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
import { createQueryAssetsMethods, type QueryAssetsClient } from './queryAssetsClient';
import { createSchemaDiffMethods, type SchemaDiffClient } from './schemaDiffClient';
import { createEventMethods, type EventClient } from './streams';
import type { McpDomainClient } from './mcpClient';
import type { SecurityClient } from './securityClient';

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
  /**
   * 查询资产域（W8 刀3a 服务端半边的 web 收口）：收藏与查询历史
   * （契约 §6.12）。桌面专属的文件系统语义（favorites root / refresh）
   * 不在其中——它们留在 legacy 轨。
   */
  queryAssets: QueryAssetsClient;
  /**
   * Schema Diff 域（W8 刀3b 服务端半边的 web 收口；契约 §6.13）。迁移
   * 历史读面（get/list migration runs）留在 sync 域待收敛。
   */
  schemaDiff: SchemaDiffClient;
  /**
   * WR-F1#2：安全域（服务端密钥管理）。**wire-direct 域**（不进 `MethodMap`，
   * 见 `./securityClient` 头注的登记）：调用方持有 HTTP wire 时经 `extras`
   * 注入，缺省缺席 —— 过渡门面（`client.ts`）不 spread，web 装配直接持有。
   */
  readonly security?: SecurityClient;
  /**
   * WR-F1#1：mcp 域（datazen 服务端 /mcp 面）。同上 wire-direct（见
   * `./mcpClient` 头注），注入或缺省缺席。
   */
  readonly mcp?: McpDomainClient;
}

/** wire-direct 两域的注入面：`createClients` 的调用方持有 wire 时传入。 */
export interface WireDirectDomains {
  readonly security?: SecurityClient;
  readonly mcp?: McpDomainClient;
}

/** Build every domain client over one transport, sharing one guard. */
export function createClients(
  transport: BackendTransport,
  guard: BackendGuard,
  extras: WireDirectDomains = {},
): DomainClients {
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
    queryAssets: createQueryAssetsMethods(transport, guard),
    schemaDiff: createSchemaDiffMethods(transport, guard),
    // wire-direct 两域只在调用方注入时出现（无 wire 时缺席，绝不铸造
    // 一个「会在使用时才发现没有 fetch 实现」的假成员）。
    ...(extras.security !== undefined ? { security: extras.security } : {}),
    ...(extras.mcp !== undefined ? { mcp: extras.mcp } : {}),
  };
}

/* ------------------------------------------------------------------ */
/* Compile-time key-consistency assertions (plan §5.3 exit criterion). */
/* ------------------------------------------------------------------ */

type Assert<T extends true> = T;

/**
 * Every non-gateway `MethodMap` key is a method of the facade.
 *
 * The legacy gateway keys and the legacy host-command rail keys are excluded
 * on purpose: the former are addressed by the `driverGateway` domain client,
 * the latter by the `legacyInvoke` escape hatch, and neither is spread onto
 * the facade. If a domain slice gains a key that no facade method serves,
 * this alias stops satisfying `true` and the build fails — the exact drift
 * the plan's "键一致性编译期断言" asks for.
 */
export type FacadeCoversMethodMap = Assert<
  Exclude<
    keyof MethodMap,
    | keyof import('./driverGatewayClient').DriverGatewayMethodMap
    | keyof import('./legacyHostCommands').LegacyHostMethodMap
  > extends keyof Omit<import('./client').BackendClient, 'backendId' | 'transport'>
    ? true
    : false
>;
