/**
 * MCP Client, web form (WR-F3) — the browser counterpart of the desktop
 * `McpClientManager` (`apps/desktop/src-tauri/src/mcp/client.rs`): browsers
 * reach external MCP servers directly over streamable HTTP instead of stdio
 * subprocesses.
 *
 * 与桌面 `McpClientManager` 的差异登记（本刀边界，WR-F3）：
 * 1. 仅传输与进程模型不同：桌面经 stdio 子进程（rmcp `TokioChildProcess`），
 *    这里经 streamable HTTP（POST JSON-RPC；initialize 响应的 `Mcp-Session-Id`
 *    会话头保存在客户端并在后续请求回传；DELETE 结束会话）。握手语义同构：
 *    initialize → notifications/initialized → tools/list 缓存。
 * 2. 服务端身份校验与 stdio spawn 白名单（`validate_mcp_spawn_command`、
 *    env/token 校验）留在桌面；本文件的 `WebMcpServerConfig` 类型上就没有
 *    command/args/env/token 字段，只有 `serverId`/`url`/`headers?`。
 * 3. 两形态共用同一个消费者端口 `McpToolBroker`（modules/src/ai/application/
 *    ports.rs：tools_for_ai / call_tool）。桌面半边由 host `McpClientManager`
 *    实现；web 半边的 broker 适配由 apps/web 装配层完成（收口刀），本文件只
 *    提供 client 层，不改任何装配。
 *
 * 所有 fetch 由构造注入（`McpFetch`）：测试零网络；web 装配层传
 * `globalThis.fetch`。与桌面相反的另两处小差异：连接超时（桌面 30s）留给
 * 装配层经 `AbortSignal` 处理；`serverName` 来自 initialize 响应的
 * `serverInfo.name`（桌面取自配置里的 name 字段，web 配置无此来源）。
 */

/** One external MCP server's HTTP endpoint — no stdio/token fields by construction. */
export interface WebMcpServerConfig {
  /** Registry key + AI routing segment (`mcp/{serverId}/{toolName}`). */
  serverId: string;
  /** Streamable-HTTP endpoint of the MCP server. */
  url: string;
  /** Extra request headers (auth etc.), sent on every request to this server. */
  headers?: Record<string, string>;
}

/** Mirror of the desktop `McpToolInfo` (serde camelCase) — same field names. */
export interface WebMcpToolInfo {
  serverId: string;
  serverName: string;
  toolName: string;
  /** Routing name the model sees: `mcp/{serverId}/{toolName}` — same rule as `datazen::ai::mcp_tools::qualified_tool_name`. */
  qualifiedName: string;
  description: string | null;
  inputSchema: Record<string, unknown>;
}

/** Structural subset of `Response` the client relies on — keeps tests dependency-free. */
export interface McpHttpResponse {
  readonly ok: boolean;
  readonly status: number;
  readonly statusText: string;
  readonly headers: { get(name: string): string | null };
  text(): Promise<string>;
}

/** The injected transport. Web assembly passes `globalThis.fetch` directly. */
export type McpFetch = (url: string, init: RequestInit) => Promise<McpHttpResponse>;

/** One MCP tool result content block (only `type`/`text` are consumed). */
export interface McpContentBlock {
  type: string;
  text?: string;
}

/** Shape of the MCP `tools/call` result the formatter consumes. */
export interface McpToolCallResult {
  content?: McpContentBlock[];
  isError?: boolean;
}

/** JSON-RPC error answered by the server (protocol level), wrapped for callers. */
export class McpJsonRpcError extends Error {
  constructor(
    readonly code: number | undefined,
    message: string,
  ) {
    super(code === undefined ? message : `MCP error ${code}: ${message}`);
    this.name = 'McpJsonRpcError';
  }
}

/** Routing name for AI — same rule as the desktop's shared-layer helper. */
export function mcpQualifiedName(serverId: string, toolName: string): string {
  return `mcp/${serverId}/${toolName}`;
}

const MCP_SERVER_ID_PATTERN = /^[a-zA-Z0-9_-]+$/;

function validateServerId(serverId: string): void {
  if (serverId.length === 0 || !MCP_SERVER_ID_PATTERN.test(serverId)) {
    throw new Error(`Invalid MCP server id '${serverId}': must match ^[a-zA-Z0-9_-]+$`);
  }
}

/**
 * Web twin of the desktop `format_call_tool_result`: text blocks flatten and
 * join with '\n'; `isError` renders the failure channel
 * `MCP tool error ({qualified}): {output}`; text-absent answers the non-text
 * summary. Throwing vs returning is the caller's choice.
 */
export function formatCallToolResult(result: McpToolCallResult, qualifiedName: string): string {
  const textParts: string[] = [];
  let nonTextCount = 0;
  for (const block of result.content ?? []) {
    if (block?.type === 'text' && typeof block.text === 'string') {
      textParts.push(block.text);
    } else {
      nonTextCount += 1;
    }
  }
  const output = textParts.join('\n');
  if (result.isError === true) {
    return `MCP tool error (${qualifiedName}): ${output}`;
  }
  if (output === '' && nonTextCount > 0) {
    return `MCP tool (${qualifiedName}) returned ${nonTextCount} non-text content block(s); no text output available.`;
  }
  return output;
}

interface JsonRpcResponse {
  id?: number | string | null;
  result?: Record<string, unknown>;
  error?: { code?: number; message?: string };
}

interface WireTool {
  name: string;
  description?: string | null;
  inputSchema?: Record<string, unknown>;
}

/** Extract JSON-RPC messages from an SSE body (`data:` lines), skipping non-JSON payloads. */
function parseSseData(body: string): JsonRpcResponse[] {
  const messages: JsonRpcResponse[] = [];
  for (const line of body.split('\n')) {
    const trimmed = line.trim();
    if (!trimmed.startsWith('data:')) continue;
    const payload = trimmed.slice('data:'.length).trim();
    if (payload === '' || payload === '[DONE]') continue;
    try {
      messages.push(JSON.parse(payload) as JsonRpcResponse);
    } catch {
      // Not a JSON-RPC message — ignore and keep scanning.
    }
  }
  return messages;
}

async function readJsonRpcResponse(
  res: McpHttpResponse,
  method: string,
  id: number,
): Promise<JsonRpcResponse> {
  const raw = await res.text();
  let message: JsonRpcResponse | undefined;
  if ((res.headers.get('content-type') ?? '').includes('text/event-stream')) {
    message = parseSseData(raw).find((m) => m.id === id);
  } else {
    try {
      message = JSON.parse(raw) as JsonRpcResponse;
    } catch {
      message = undefined;
    }
  }
  if (message?.error) {
    const err = message.error;
    throw new McpJsonRpcError(
      err.code,
      typeof err.message === 'string' ? err.message : JSON.stringify(err),
    );
  }
  // HTTP-level failure wins over any non-RPC body (a plain JSON error page
  // must not be mistaken for a JSON-RPC result).
  if (!res.ok) throw new Error(`HTTP ${res.status} ${res.statusText}`.trim());
  if (!message) throw new Error(`no JSON-RPC reply for '${method}'`);
  return message;
}

const ACCEPT_STREAMABLE_HTTP = 'application/json, text/event-stream';
const MCP_PROTOCOL_VERSION = '2025-06-18';
const SESSION_HEADER = 'Mcp-Session-Id';

/**
 * One external MCP server's HTTP connection. Built through
 * {@link WebMcpClient.connect} — the constructor is private so a client cannot
 * exist without its initialize handshake.
 */
export class WebMcpClient {
  private sessionHeaders: Record<string, string> = {};
  private toolsCache: WebMcpToolInfo[] | null = null;
  private closed = false;
  private nextRequestId = 1;
  private _serverName: string;

  private constructor(
    private readonly config: WebMcpServerConfig,
    private readonly fetchFn: McpFetch,
  ) {
    this._serverName = config.serverId;
  }

  get serverId(): string {
    return this.config.serverId;
  }

  /** From the initialize response's `serverInfo.name`; falls back to `serverId`. */
  get serverName(): string {
    return this._serverName;
  }

  /**
   * Desktop connect parity: validate id → initialize handshake →
   * notifications/initialized → tools/list cache. Any handshake failure
   * leaves nothing registered and (best-effort) terminates the session.
   */
  static async connect(config: WebMcpServerConfig, fetchFn: McpFetch): Promise<WebMcpClient> {
    validateServerId(config.serverId);
    const client = new WebMcpClient(config, fetchFn);

    const initId = client.nextRequestId++;
    let res: McpHttpResponse;
    let initMessage: JsonRpcResponse;
    try {
      res = await client.post({
        jsonrpc: '2.0',
        id: initId,
        method: 'initialize',
        params: {
          protocolVersion: MCP_PROTOCOL_VERSION,
          capabilities: {},
          clientInfo: { name: 'datazen-web', version: '0.1.0' },
        },
      });
      initMessage = await readJsonRpcResponse(res, 'initialize', initId);
    } catch (e) {
      const message = e instanceof Error ? e.message : String(e);
      throw new Error(`Failed to initialize MCP client: ${message}`);
    }

    const sessionId = res.headers.get(SESSION_HEADER);
    if (sessionId) client.sessionHeaders[SESSION_HEADER] = sessionId;
    const serverInfo = initMessage.result?.['serverInfo'] as { name?: unknown } | undefined;
    if (typeof serverInfo?.name === 'string' && serverInfo.name !== '') {
      client._serverName = serverInfo.name;
    }

    try {
      await client.notify('notifications/initialized');
    } catch (e) {
      await client.close();
      const message = e instanceof Error ? e.message : String(e);
      throw new Error(`Failed to initialize MCP client: ${message}`);
    }
    try {
      await client.fetchTools();
    } catch (e) {
      await client.close(); // parity: desktop cancels the session when tools/list fails
      const message = e instanceof Error ? e.message : String(e);
      throw new Error(`Failed to list tools: ${message}`);
    }
    return client;
  }

  /** Cached tool list, or a fresh `tools/list` when the cache is cold. */
  async tools(): Promise<WebMcpToolInfo[]> {
    if (this.toolsCache) return this.toolsCache;
    return this.fetchTools();
  }

  /** Cache-only view (populated at connect) — used for counts, never fetches. */
  get cachedTools(): WebMcpToolInfo[] {
    return this.toolsCache ?? [];
  }

  /**
   * Desktop `call_tool` parity: non-object arguments are the same error text;
   * a JSON-RPC error throws `Tool call failed: …`; an `isError` result throws
   * the `format_call_tool_result` failure-channel text; success returns the
   * flattened result text (the `McpToolBroker.call_tool` contract:
   * pre-formatted text).
   */
  async callTool(toolName: string, args: unknown): Promise<string> {
    if (args === null || typeof args !== 'object' || Array.isArray(args)) {
      throw new Error('Tool arguments must be a JSON object');
    }
    const qualified = mcpQualifiedName(this.serverId, toolName);
    let result: Record<string, unknown>;
    try {
      result = await this.request('tools/call', {
        name: toolName,
        arguments: args as Record<string, unknown>,
      });
    } catch (e) {
      throw new Error(`Tool call failed: ${e instanceof Error ? e.message : String(e)}`);
    }
    const callResult: McpToolCallResult = {
      content: Array.isArray(result['content']) ? (result['content'] as McpContentBlock[]) : [],
      isError: result['isError'] === true,
    };
    const text = formatCallToolResult(callResult, qualified);
    if (callResult.isError) throw new Error(text);
    return text;
  }

  /**
   * Terminate the HTTP session with DELETE (session header attached) and drop
   * the cache. Best-effort — network failures are swallowed, mirroring the
   * desktop's `let _ = entry.service.cancel()`. Idempotent.
   */
  async close(): Promise<void> {
    if (this.closed) return;
    this.closed = true;
    this.toolsCache = null;
    const sessionId = this.sessionHeaders[SESSION_HEADER];
    this.sessionHeaders = {};
    if (!sessionId) return; // stateless server: nothing to terminate
    try {
      await this.fetchFn(this.config.url, {
        method: 'DELETE',
        headers: { ...this.config.headers, [SESSION_HEADER]: sessionId },
      });
    } catch {
      // Best-effort teardown — never surface.
    }
  }

  private async post(body: unknown): Promise<McpHttpResponse> {
    if (this.closed) {
      throw new Error(`MCP server '${this.serverId}' is disconnected`);
    }
    return this.fetchFn(this.config.url, {
      method: 'POST',
      headers: {
        'Content-Type': 'application/json',
        Accept: ACCEPT_STREAMABLE_HTTP,
        ...this.config.headers,
        ...this.sessionHeaders,
      },
      body: JSON.stringify(body),
    });
  }

  private async request(
    method: string,
    params: Record<string, unknown>,
  ): Promise<Record<string, unknown>> {
    const id = this.nextRequestId++;
    const res = await this.post({ jsonrpc: '2.0', id, method, params });
    const message = await readJsonRpcResponse(res, method, id);
    return message.result ?? {};
  }

  private async notify(method: string): Promise<void> {
    const res = await this.post({ jsonrpc: '2.0', method });
    if (!res.ok) throw new Error(`HTTP ${res.status} ${res.statusText}`.trim());
  }

  private async fetchTools(): Promise<WebMcpToolInfo[]> {
    const collected: WireTool[] = [];
    let cursor: string | null = null;
    do {
      const result = await this.request('tools/list', cursor ? { cursor } : {});
      const tools = Array.isArray(result['tools']) ? (result['tools'] as WireTool[]) : [];
      collected.push(...tools);
      const next = result['nextCursor'];
      cursor = typeof next === 'string' && next !== '' ? next : null;
    } while (cursor !== null);
    const mapped = collected.map((tool) => ({
      serverId: this.serverId,
      serverName: this._serverName,
      toolName: tool.name,
      qualifiedName: mcpQualifiedName(this.serverId, tool.name),
      description: typeof tool.description === 'string' ? tool.description : null,
      inputSchema:
        tool.inputSchema && typeof tool.inputSchema === 'object' ? tool.inputSchema : {},
    }));
    this.toolsCache = mapped;
    return mapped;
  }
}

/**
 * Multi-server registry — web twin of the desktop `McpClientManager`.
 * Reconnects replace the previous entry (closing its session first), unknown
 * ids on `disconnect` are a noop, and `tools`/`callTool` carry the same
 * "not connected" error text as the desktop.
 */
export class WebMcpClientManager {
  private readonly clients = new Map<string, WebMcpClient>();

  constructor(private readonly fetchFn: McpFetch) {}

  /** Register one external MCP server (handshake included). Replaces an existing same-id entry. */
  async connect(config: WebMcpServerConfig): Promise<WebMcpClient> {
    const client = await WebMcpClient.connect(config, this.fetchFn);
    const old = this.clients.get(config.serverId);
    if (old) await old.close();
    this.clients.set(config.serverId, client);
    return client;
  }

  /** Desktop parity: unknown server is a noop; otherwise DELETE the session and clear the registry. */
  async disconnect(serverId: string): Promise<void> {
    const entry = this.clients.get(serverId);
    if (!entry) return;
    this.clients.delete(serverId);
    await entry.close();
  }

  async disconnectAll(): Promise<void> {
    const entries = [...this.clients.values()];
    this.clients.clear();
    await Promise.all(entries.map((client) => client.close()));
  }

  /** All servers' tools flattened (`all_tools` parity), or one server's list. */
  async tools(serverId?: string): Promise<WebMcpToolInfo[]> {
    if (serverId === undefined) {
      const all = await Promise.all([...this.clients.values()].map((client) => client.tools()));
      return all.flat();
    }
    return this.requireClient(serverId).tools();
  }

  /** Pre-formatted result text — the shape `McpToolBroker.call_tool` returns. */
  async callTool(serverId: string, toolName: string, args: unknown): Promise<string> {
    return this.requireClient(serverId).callTool(toolName, args);
  }

  /** Desktop `connected_servers` parity: (id, name, tool count), typed. */
  connectedServers(): Array<{ serverId: string; serverName: string; toolCount: number }> {
    return [...this.clients.values()].map((client) => ({
      serverId: client.serverId,
      serverName: client.serverName,
      toolCount: client.cachedTools.length,
    }));
  }

  private requireClient(serverId: string): WebMcpClient {
    const entry = this.clients.get(serverId);
    if (!entry) throw new Error(`MCP server '${serverId}' not connected`);
    return entry;
  }
}
