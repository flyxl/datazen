/**
 * web AI 装配（WR-F1）：`AiRequestHost` / `McpToolBroker` / SecretProvider
 * 的 web 形态实现。
 *
 * 端口形状见 `./aiPorts`（`modules/src/ai/application/ports.rs` 的 TS 同形）。
 * 桌面侧对照（`apps/desktop/src-tauri/src/services/feature_ports.rs`）：
 * `AppState` 实现 `DatabaseToolHost` + `McpToolBroker`（stdio 子进程管理器），
 * `AiRequestHost` 按 claim 分流；web 侧同一批端口，实现换成 HTTP 面。
 *
 * ## WR-F1 三缺口闭合记录（原缺口常量的历史登记）
 *
 * - **F1#1（McpToolBroker）已闭合**：web broker 经 mcp 域客户端
 *   （`packages/backend-client/src/mcpClient.ts`）直连 datazen 服务端
 *   `/mcp` 面（WR-F2 的 streamable HTTP）。v1 语义：桌面 broker 面向多个
 *   外部 MCP 服务器；web v1 聚焦 datazen 自有 13 工具面，broker 端口的
 *   `serverId` 参数固定为 `WEB_MCP_SERVER_ID`（`"datazen"`），端口形状不变
 *   （ports.rs 108 行）；非 datazen 的 serverId 在装配边界如实抛
 *   `InvalidArgument`（测试名钉住该差异）。
 * - **F1#2（SecretProvider）已闭合**：web 无钥匙串，密钥语义是服务端
 *   密钥管理 —— `resolveSecret` 走 SecurityClient
 *   （`POST /api/v1/secrets/resolve`，轨 4-e 冻结契约；红线见
 *   securityClient.ts：明文只出现在认证主体本人的请求里，无列表无枚举面）。
 *   `parseSecretRef` 的装配期校验半边保留：坏引用在离开装配前被拒，
 *   绝不变成一次出站请求。
 * - **F1#3（AI 工具宿主域，TS 半边）已闭合**：`getSessionConfig` /
 *   `executeTarget` 走共享 HTTP wire 打向 `aiHost.*` 两键
 *   （`POST /api/v1/ai-host/session-config` / `POST /api/v1/ai-host/execute-target`，
 *   轨 4-e 冻结契约：体 = payload 原样、claim 进体、响应 JSON）。
 *   服务端路由随轨 4-e 落地；路形状登记在 `../httpRoutes.ts` 的
 *   `WR_F1_HTTP_ROUTES`。
 *
 * 三个域都不经 `transport.call`：`MethodMap` 对本刀冻结（交集在
 * transport.ts、门面覆盖证明在 client.ts，均在白名单之外；/mcp 面还在
 * /api/v1 之外且需读响应头）。wire-direct 域客户端由本装配注入
 * `fetchImpl`（生产 = 同源全局 fetch；测试 = 假体，注入纪律与
 * backendTransport 一致），`F5#1` 的隧道凭据解析同走 SecurityClient
 * （见 `../tunnelAssembly.ts`）。
 */

import {
  ApiError,
  apiErrorFromWireResponse,
  createHttpWire,
  createMcpClient,
  createSecurityClient,
  jsonBodyOfWireResponse,
  WEB_MCP_SERVER_ID,
  type BackendClient,
  type BackendTransport,
  type HttpWire,
  type HttpWireOptions,
  type McpDomainClient,
  type McpToolDescriptor,
  type SecurityClient,
} from '@datazen/backend-client';

import {
  MCP_QUALIFIED_PREFIX,
  type AiTargetClaim,
  type McpBrokeredTool,
  type McpToolBroker,
  type SecretRef,
  type WebAiRequestHost,
  type WebSecretProvider,
} from './aiPorts';

/**
 * 装配注入点：预铸的 wire-direct 客户端（测试注入假体）或自建 wire 的
 * 选项（生产缺省 = 同源 `/api/v1` + 全局 fetch + CSRF cookie 半边）。
 * `mcpWire` 只影响 /mcp 面（基 = 服务端根；`baseUrl` 缺省 `''`）。
 */
export interface WebAiBindingsOptions {
  /** 预铸安全域客户端；缺省按 `wire` 选项自建。 */
  readonly security?: SecurityClient;
  /** 预铸 mcp 域客户端；缺省按 `mcpWire` 选项自建。 */
  readonly mcp?: McpDomainClient;
  /** 自建 security/aiHost wire 的选项（基缺省 `/api/v1`）。 */
  readonly wire?: HttpWireOptions;
  /** 自建 mcp wire 的选项（基缺省 = 服务端根 `''`）。 */
  readonly mcpWire?: HttpWireOptions;
}

/**
 * 生产缺省的出站 fetch 实现（apps/web 不在 F-07 扫描面，可以引用全局
 * fetch；node 测试环境一律经 `options.wire.fetchImpl` 注入假体）。
 */
function defaultBrowserFetch(): typeof fetch {
  if (typeof globalThis.fetch !== 'function') {
    throw new Error(
      'createWebAiBindings: no fetch implementation available; inject one via options.wire.fetchImpl (tests) or run in a browser.',
    );
  }
  return globalThis.fetch.bind(globalThis);
}

/** 装配期把测试/生产缺省折进 wire 选项（缺省永远非 undefined）。 */
function withBrowserDefaults(options: HttpWireOptions | undefined): HttpWireOptions {
  return {
    baseUrl: options?.baseUrl,
    csrfTokenProvider: options?.csrfTokenProvider,
    fetchImpl: options?.fetchImpl ?? defaultBrowserFetch(),
  };
}

/**
 * SecretRef 的解析半边：只收 `ai-provider` 域的非空 key。
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
 * web SecretProvider（F1#2 闭合）：装配期校验半边 + SecurityClient 的
 * 取值半边。密钥本体永不落浏览器存储，也只在「认证主体本人的请求」的
 * 响应里出现一次。
 */
export function createWebSecretProvider(security: SecurityClient): WebSecretProvider {
  return {
    async resolveSecret(ref: SecretRef): Promise<string> {
      // 解析半边先行：坏引用是装配错误，不是密钥缺失。
      const parsed = parseSecretRef(ref);
      const response = await security.resolveSecret({ domain: parsed.domain, key: parsed.key });
      return response.value;
    },
  };
}

/**
 * `tools/list` 条目 → 端口的 `McpBrokeredTool`（v1 语义：单服务面，
 * serverId 恒为 `WEB_MCP_SERVER_ID`，qualifiedName 沿
 * `mcp/{serverId}/{toolName}` 路由规则）。
 */
function toBrokeredTool(tool: McpToolDescriptor): McpBrokeredTool {
  return {
    serverId: WEB_MCP_SERVER_ID,
    toolName: tool.name,
    qualifiedName: `${MCP_QUALIFIED_PREFIX}${WEB_MCP_SERVER_ID}/${tool.name}`,
    ...(tool.description !== undefined ? { description: tool.description } : {}),
    inputSchema: tool.inputSchema,
  };
}

/**
 * web `McpToolBroker`（F1#1 闭合）：经 mcp 域客户端取 datazen 服务端
 * `/mcp` 面的工具与调用。`callTool` 的成功值 = 文本内容块的拼接
 * （server v1 只发 text 块；块间以换行相连，与 rmcp 文本投影同观感）。
 */
export function createWebMcpToolBroker(mcp: McpDomainClient): McpToolBroker {
  return {
    async toolsForAi(): Promise<readonly McpBrokeredTool[]> {
      const { tools } = await mcp.listTools();
      return tools.map(toBrokeredTool);
    },
    async callTool(serverId: string, toolName: string, args: unknown): Promise<string> {
      if (serverId !== WEB_MCP_SERVER_ID) {
        // v1 语义登记：web broker 只面向 datazen 服务端自有工具面；
        // 其余 serverId 如实拒绝，绝不静默改道。
        throw new ApiError(
          'InvalidArgument',
          `callTool: the web broker v1 serves only the '${WEB_MCP_SERVER_ID}' server surface ` +
            `(datazen's own tools via the server /mcp face); requested '${serverId}'.`,
        );
      }
      const result = await mcp.callTool({ name: toolName, arguments: args });
      return result.content
        .filter((block) => block.type === 'text' && typeof block.text === 'string')
        .map((block) => block.text as string)
        .join('\n');
    },
  };
}

/** 一条 aiHost 契约调用：体 = payload 原样，2xx → JSON，非 2xx → §13 归一。 */
async function aiHostPost(
  wire: HttpWire,
  methodLabel: string,
  path: string,
  payload: unknown,
): Promise<unknown> {
  const response = await wire({ method: 'POST', path, body: payload });
  if (!response.ok) {
    // §13 四字段体 → ApiError（字段保真）；体不可读 → 带状态码的
    // ServiceUnavailable —— 与 web transport 的 rejectionOf 同语义，
    // 归一口径由 backend-client 的共享 helper 承担。
    throw await apiErrorFromWireResponse(response, methodLabel);
  }
  return jsonBodyOfWireResponse(response, methodLabel);
}

/**
 * web 请求级工具宿主（F1#3 的 TS 半边闭合）：claim 决定资源归属的语义与
 * 桌面 `AiRequestHost` 同构。设置面与桌面同源（服务端设置域的同一切片）；
 * AI 工具宿主域走 `aiHost.*` 两键的冻结契约。
 */
export function createWebAiRequestHost(
  client: BackendClient,
  _secrets: WebSecretProvider,
  aiHostWire: HttpWire,
): WebAiRequestHost {
  // `_secrets` 保留端口位置：桌面侧的宿主持有 SecretProvider 做 claim 级
  // 密钥门控；web v1 的取值面在 provider 半边（调用方先 resolve 再入参），
  // 宿主侧门控随桌面语义对齐时在此启用，签名不动。
  return {
    // 安全门与设置域同源：桌面读宿主持久化，web 读服务端设置域的同一切片。
    async executionSettings() {
      const settings = await client.getSettings();
      return settings.execution as unknown as Record<string, unknown>;
    },
    async getSessionConfig(sessionId: string) {
      const payload = { sessionId };
      return (await aiHostPost(
        aiHostWire,
        'aiHost.getSessionConfig',
        '/ai-host/session-config',
        payload,
      )) as Record<string, unknown>;
    },
    async executeTarget(id: string, command: string, input: unknown, claim: AiTargetClaim) {
      // claim 进体：归因语义随请求走（轨 4-e 冻结契约）。
      const payload = { id, command, input, claim };
      return aiHostPost(aiHostWire, 'aiHost.executeTarget', '/ai-host/execute-target', payload);
    },
  };
}

/** web AI 装配产物：三端口 + 铸出它们的 wire-direct 客户端（透出给隧道装配复用）。 */
export interface WebAiBindings {
  readonly requestHost: WebAiRequestHost;
  readonly mcpBroker: McpToolBroker;
  readonly secrets: WebSecretProvider;
  /** 本装配实际持有的安全域客户端（注入或缺省自建）。 */
  readonly security: SecurityClient;
  /** 本装配实际持有的 mcp 域客户端。 */
  readonly mcp: McpDomainClient;
}

/**
 * 装配入口：给定 backend facade 与 transport，铸出 AI 三端口的 web 实现。
 * `assembleWebPlatform()` 在装配期调用一次（两参形态向后兼容——wire-direct
 * 客户端按缺省自建）；测试经 `options` 注入假客户端或假 fetch。
 * `_transport` 参数保留装配面兼容：AI 三端口现走 wire-direct 客户端，
 * transport 仍是 facade 的既有底座（执行设置经 facade 设置域读取）。
 */
export function createWebAiBindings(
  client: BackendClient,
  _transport: BackendTransport,
  options: WebAiBindingsOptions = {},
): WebAiBindings {
  // `_transport` 保留装配面兼容（assembleWebPlatform 的两参调用不动）：
  // AI 三端口现走 wire-direct 客户端；facade 的既有底座仍服务执行设置。
  const security =
    options.security ??
    createSecurityClient(withBrowserDefaults(options.wire));
  // mcp 的基缺省 = 服务端根：createMcpClient 内部把 undefined baseUrl 归 ''。
  const mcp = options.mcp ?? createMcpClient(withBrowserDefaults(options.mcpWire));
  const secrets = createWebSecretProvider(security);
  const aiHostWire = createHttpWire(withBrowserDefaults(options.wire));
  return {
    requestHost: createWebAiRequestHost(client, secrets, aiHostWire),
    mcpBroker: createWebMcpToolBroker(mcp),
    secrets,
    security,
    mcp,
  };
}
