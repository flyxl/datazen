/**
 * AI 工具循环的消费者端口 —— web 形态（WR-F1）。
 *
 * 这是 `crates/datazen/src/ai/application/ports.rs` 的 TS 同形转写（web 装配侧的
 * 消费面）：`McpToolBroker`（M3）、`DatabaseToolHost` 的最小读面与 claim
 * 归因（P6 §14.4 的 `AiRequestHost` 语义）、`SecretProvider` 的 web 语义
 * （服务端密钥管理）。端口定义在使用它的层，宿主（web 装配）提供实现——
 * 工具循环只认识这里的形状，**零形态感知**（M3 验收的延伸）：
 * 桌面绑定 stdio 子进程管理器，web 绑定 mcp 域客户端（wire 直连 datazen
 * 服务端 /mcp 面，WR-F1#1 闭合，serverId 固定 `"datazen"`，见
 * `packages/backend-client/src/mcpClient.ts` 的 v1 语义登记），
 * 循环体一行不改。
 */

/** MCP 服务器暴露的一个工具（`McpBrokeredTool` 的 TS 同形）。 */
export interface McpBrokeredTool {
  readonly serverId: string;
  readonly toolName: string;
  /** 模型看到的路由名：`mcp/{serverId}/{toolName}`。 */
  readonly qualifiedName: string;
  readonly description?: string;
  readonly inputSchema: unknown;
}

/**
 * AI 工具循环眼中的 MCP（`McpToolBroker` 的 TS 同形）。
 * `callTool` 成功值即预格式化文本（结果格式化随实现走）。
 */
export interface McpToolBroker {
  toolsForAi(): Promise<readonly McpBrokeredTool[]>;
  callTool(serverId: string, toolName: string, args: unknown): Promise<string>;
}

/**
 * 密钥引用（`SecretRef` 的最小 web 形状）：AI 供应商密钥不再存浏览器——
 * web 无钥匙串，语义是**服务端密钥管理**（密钥只以引用形式出现在前端）。
 */
export interface SecretRef {
  readonly domain: 'ai-provider';
  readonly key: string;
}

/**
 * web SecretProvider：按引用向服务端安全域取用密钥。
 * 实现走 wire-direct 的 SecurityClient（`POST /api/v1/secrets/resolve`，
 * WR-F1#2 已闭合；红线与路由形状见 `packages/backend-client/src/securityClient.ts`）。
 */
export interface WebSecretProvider {
  resolveSecret(ref: SecretRef): Promise<string>;
}

/** 目标执行的身份归因（`AiTargetClaim` 的 TS 同形，简化双态）。 */
export type AiTargetClaim =
  | { readonly kind: 'independent' }
  | { readonly kind: 'editorGrant'; readonly grantId: string };

/**
 * `aiHost.getSessionConfig` 的请求体（轨 4-e 冻结契约：payload 原样透传）。
 * 响应为 JSON（会话配置对象），由实现归一为 `Record<string, unknown>`。
 */
export interface AiHostSessionConfigRequest {
  readonly sessionId: string;
}

/**
 * `aiHost.executeTarget` 的请求体（轨 4-e 冻结契约：payload 原样透传，
 * `claim` 进体 —— claim 决定资源归属的归因语义与桌面同构）。
 */
export interface AiHostExecuteTargetRequest {
  readonly id: string;
  readonly command: string;
  readonly input: unknown;
  readonly claim: AiTargetClaim;
}

/**
 * web 形态的请求级工具宿主（`DatabaseToolHost` + claim 归因的最小读面）。
 * 「claim 决定工具可触哪个资源」的语义与桌面 `AiRequestHost` 一致。
 */
export interface WebAiRequestHost {
  /** 运行时会话的连接配置。 */
  getSessionConfig(sessionId: string): Promise<Record<string, unknown>>;
  /** 执行设置（安全门读取；与设置域同一底层事实）。 */
  executionSettings(): Promise<Record<string, unknown>>;
  /** 单一授权入口：AI/工具的每条数据库命令都从这里走。 */
  executeTarget(
    id: string,
    command: string,
    input: unknown,
    claim: AiTargetClaim,
  ): Promise<unknown>;
}

/** 工具循环宿主（`AiToolLoopHost` 的 TS 同形）：循环只经此三问。 */
export interface AiToolLoopHost {
  egressSummary(): Promise<string>;
  executeDbTool(call: AiToolCall): Promise<string>;
  executeMcpTool(server: string, tool: string, args: string): Promise<string>;
}

/** 一轮模型回复里的工具调用。 */
export interface AiToolCall {
  readonly name: string;
  readonly arguments: string;
}

/** 一轮模型回复（provider 端口的最小形状，循环测试用假实现即可）。 */
export interface AiRoundResult {
  readonly content?: string;
  readonly toolCalls?: readonly AiToolCall[];
}

/** AI 供应商客户端端口（providers 已是 modules 内 HTTP 客户端；形态无关）。 */
export interface AiProviderClient {
  completeRound(messages: readonly unknown[]): Promise<AiRoundResult>;
}

export const MCP_QUALIFIED_PREFIX = 'mcp/';

/** `mcp/{serverId}/{toolName}` 的解析（与 Rust `qualified_tool_name` 同规则）。 */
export function splitQualifiedToolName(
  qualifiedName: string,
): { serverId: string; toolName: string } | null {
  if (!qualifiedName.startsWith(MCP_QUALIFIED_PREFIX)) return null;
  const rest = qualifiedName.slice(MCP_QUALIFIED_PREFIX.length);
  const separator = rest.indexOf('/');
  if (separator <= 0 || separator === rest.length - 1) return null;
  return { serverId: rest.slice(0, separator), toolName: rest.slice(separator + 1) };
}

/**
 * 有界工具循环（`tool_loop.rs` 的 TS 同形骨架）：模型回工具调用 → 按
 * `mcp/` 前缀路由到宿主的 MCP 半边或 DB 半边 → 结果回灌 → 直到纯文本轮
 * 或轮数上限。宿主错误折成 `Error: …` 文本继续（循环永不向调用方抛形态
 * 错误）；路由只看 `qualifiedName`——循环不感知任何形态。
 */
export async function runAiToolLoop(
  provider: AiProviderClient,
  host: AiToolLoopHost,
  maxRounds: number,
  messages: readonly unknown[],
): Promise<string> {
  const transcript: unknown[] = [...messages];
  for (let round = 0; round < maxRounds; round += 1) {
    const result = await provider.completeRound(transcript);
    const calls = result.toolCalls ?? [];
    if (calls.length === 0) {
      return result.content ?? '';
    }
    for (const call of calls) {
      const mcp = splitQualifiedToolName(call.name);
      const text = mcp
        ? await safeTool(() => host.executeMcpTool(mcp.serverId, mcp.toolName, call.arguments))
        : await safeTool(() => host.executeDbTool(call));
      transcript.push({ tool: call.name, result: text });
    }
  }
  return `Error: tool loop exhausted ${maxRounds} rounds without a final answer.`;
}

/** 宿主半边的错误折算：失败也是文本，循环继续（桌面侧同语义）。 */
async function safeTool(run: () => Promise<string>): Promise<string> {
  try {
    return await run();
  } catch (error) {
    return `Error: ${error instanceof Error ? error.message : String(error)}`;
  }
}
