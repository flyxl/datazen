/**
 * web AI 装配（WR-F1）：`AiRequestHost` / `McpToolBroker` / SecretProvider
 * 的 web 形态实现，全部经 WebClient transport（WR-E2 的 HTTP 面）。
 *
 * 端口形状见 `./aiPorts`（`modules/src/ai/application/ports.rs` 的 TS 同形）。
 * 桌面侧对照（`apps/desktop/src-tauri/src/services/feature_ports.rs`）：
 * `AppState` 实现 `DatabaseToolHost` + `McpToolBroker`（stdio 子进程管理器），
 * `AiRequestHost` 按 claim 分流；web 侧同一批端口，实现换成 HTTP 面：
 *
 * - `executionSettings`：settings 域 `getSettings` 的 `execution` 切片——
 *   与桌面同源（宿主持久化的执行设置），零新端口；
 * - `getSessionConfig` / `executeTarget`：web 的 HTTP 总账（契约 §4）尚无
 *   AI 工具宿主域 → 如实抛 `CapabilityUnsupported`（kind `capability`），
 *   claim 归因语义保留在错误口径里；**绝不静默空答**（§13 / §7.4）；
 * - `McpToolBroker`：意图是 WebClient transport 的 mcp 域方法，但
 *   backend-client 现无 mcp 域客户端（`mcp_*` 是 legacy 宿主命令轨，契约
 *   §8 明文不上 HTTP）→ 两个方法都抛同一缺口错误（WR-F1 接线缺口 #1）；
 * - SecretProvider：web 无钥匙串，语义是服务端密钥管理——应走 backend
 *   transport 的安全域方法，但 MethodMap 现无安全域客户端（WR-F1 接线缺口
 *   #2）→ `resolveSecret` 抛缺口错误；`SecretRef` 的解析/校验半边落地
 *   （最小 stub），引用格式错误在离开装配前就被拒绝。
 *
 * 两处缺口都登记为 `capability` 口径：使用时才失败（§7），与
 * `unsupportedMethods` 的声明同构，补上对应域客户端后本文件只改实现、
 * 不改端口。
 */

import { ApiError, type BackendClient, type BackendTransport } from '@datazen/backend-client';

import {
  type AiTargetClaim,
  type McpBrokeredTool,
  type McpToolBroker,
  type SecretRef,
  type WebAiRequestHost,
  type WebSecretProvider,
} from './aiPorts';

/** WR-F1 接线缺口 #1：web 的 mcp 域客户端与 HTTP 路由（契约 §4）尚未建立。 */
export const WEB_MCP_BROKER_GAP =
  'McpToolBroker has no web route yet: backend-client has no mcp domain client (the mcp_* legacy host commands are HTTP-off, contract §8). Wiring gap WR-F1#1.';

/** WR-F1 接线缺口 #2：web 的安全域（服务端密钥管理）方法尚未进入 MethodMap。 */
export const WEB_SECRET_PROVIDER_GAP =
  'SecretProvider has no web route yet: backend transport has no security-domain method for server-side secret management. Wiring gap WR-F1#2.';

/** web 的 AI 工具宿主域缺口：目标执行/会话配置无 HTTP 路由（契约 §4）。 */
export const WEB_AI_HOST_GAP =
  'WebAiRequestHost has no AI tool-host domain on the web HTTP ledger yet (getSessionConfig/executeTarget). Wiring gap WR-F1#3.';

function capabilityGap(message: string): ApiError {
  return new ApiError('CapabilityUnsupported', message, { unsupportedKind: 'capability' });
}

/**
 * SecretRef 的解析半边（最小 stub）：只收 `ai-provider` 域的非空 key。
 * 解析失败的引用在装配边界就被拒绝——坏引用永远不会变成一次出站请求。
 */
export function parseSecretRef(input: unknown): SecretRef {
  const candidate = input as Partial<SecretRef> | null;
  if (
    candidate !== null &&
    typeof candidate === 'object' &&
    candidate.domain === 'ai-provider' &&
    typeof candidate.key === 'string' &&
    candidate.key.length > 0
  ) {
    return { domain: 'ai-provider', key: candidate.key };
  }
  throw new Error(
    'parseSecretRef: expected { domain: "ai-provider", key: string } — secrets cross the wire as references, never as values.',
  );
}

/**
 * web SecretProvider：服务端密钥管理语义下的按引用取用。
 * 密钥本体永不落浏览器存储；安全域客户端补齐后，此处只把缺口错误换成
 * `transport.call(<security-domain>)`。
 */
export function createWebSecretProvider(_transport: BackendTransport): WebSecretProvider {
  return {
    async resolveSecret(ref: SecretRef): Promise<string> {
      // 解析半边先行：坏引用是装配错误，不是密钥缺失。
      parseSecretRef(ref);
      throw capabilityGap(`${WEB_SECRET_PROVIDER_GAP} (ref: ${ref.domain}/${ref.key})`);
    },
  };
}

/**
 * web `McpToolBroker`：web 表单的 MCP 半边应走 HTTP/SSE 客户端
 * （ports.rs 的原话："a web form wires an HTTP/SSE client"）——在 mcp 域
 * 客户端存在之前，这里保持**显式缺口**：`toolsForAi` 不返回看似成功的
 * 空表，`callTool` 不返回看似成功的错误文本。
 */
export function createWebMcpToolBroker(_transport: BackendTransport): McpToolBroker {
  return {
    async toolsForAi(): Promise<readonly McpBrokeredTool[]> {
      throw capabilityGap(WEB_MCP_BROKER_GAP);
    },
    async callTool(): Promise<string> {
      throw capabilityGap(WEB_MCP_BROKER_GAP);
    },
  };
}

/**
 * web 请求级工具宿主：claim 决定资源归属的语义与桌面 `AiRequestHost`
 * 同构。已接的读面走设置域（执行设置）；无 HTTP 路由的面如实抛缺口。
 */
export function createWebAiRequestHost(
  client: BackendClient,
  _secrets: WebSecretProvider,
  _transport: BackendTransport,
): WebAiRequestHost {
  return {
    // 安全门与设置域同源：桌面读宿主持久化，web 读服务端设置域的同一切片。
    async executionSettings() {
      const settings = await client.getSettings();
      return settings.execution as unknown as Record<string, unknown>;
    },
    async getSessionConfig(_sessionId: string) {
      throw capabilityGap(WEB_AI_HOST_GAP);
    },
    async executeTarget(_id: string, _command: string, _input: unknown, claim: AiTargetClaim) {
      throw capabilityGap(`${WEB_AI_HOST_GAP} (claim: ${claim.kind})`);
    },
  };
}

/** web AI 装配产物：三端口共用一条 WebClient transport。 */
export interface WebAiBindings {
  readonly requestHost: WebAiRequestHost;
  readonly mcpBroker: McpToolBroker;
  readonly secrets: WebSecretProvider;
}

/**
 * 装配入口：给定 backend facade 与 transport，铸出 AI 三端口的 web 实现。
 * `assembleWebPlatform()` 在装配期调用一次；测试直接以假 transport 调用。
 */
export function createWebAiBindings(client: BackendClient, transport: BackendTransport): WebAiBindings {
  const secrets = createWebSecretProvider(transport);
  return {
    requestHost: createWebAiRequestHost(client, secrets, transport),
    mcpBroker: createWebMcpToolBroker(transport),
    secrets,
  };
}
