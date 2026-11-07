/**
 * The mcp domain client — WR-F1#1. Speaks the datazen server's MCP
 * streamable HTTP face (`crates/datazen-server/src/mcp/http.rs`, MCP 规范 2025-03-26 单
 * JSON 响应模式) directly over the injected HTTP wire.
 *
 * ## v1 semantics (registered, 如实差异)
 *
 * The **desktop** broker faces multiple external MCP servers (stdio
 * subprocess manager). The **web** broker takes its tool surface from the
 * datazen server's own `/mcp` face (WR-F2 的 streamable HTTP 传输；工具集 =
 * datazen 自有 13 工具面) — one server only, so the web implementation fixes
 * the broker port's `serverId` parameter to the constant
 * {@link WEB_MCP_SERVER_ID} (`"datazen"`). The `McpToolBroker` port shape is
 * unchanged (`crates/datazen/src/ai/application/ports.rs` line 108); this
 * form-factor difference is asserted by name in the web assembly tests.
 *
 * ## Wire semantics (frozen with crates/datazen-server/src/mcp/{http,wire}.rs)
 *
 * - `mcp.initialize` — `POST /mcp`, body = JSON-RPC 2.0 `initialize` request
 *   (params `{ protocolVersion, capabilities, clientInfo }` — the desktop
 *   rmcp shape; `protocolVersion` = wire.rs `PROTOCOL_VERSION` =
 *   `"2025-03-26"`). The response's `Mcp-Session-Id` **header** names the new
 *   transport session and is stored for every later request. Identity is the
 *   L2 cookie session (`credentials: 'include'`), never the MCP header
 *   (INV-S1: the transport session is not an identity).
 * - `mcp.listTools` — `POST /mcp` `tools/list` with the session header.
 * - `mcp.callTool` — `POST /mcp` `tools/call` with the session header;
 *   params `{ name, arguments }`.
 * - `mcp.closeSession` — `DELETE /mcp` with the session header; the server
 *   answers `202` and destroys the session (the desktop
 *   `release_sessions` counterpart). Closing without a session is an
 *   idempotent no-op.
 * - JSON-RPC errors map onto §13 `ApiError` codes with the server message
 *   carried verbatim: `-32601` method_not_found → `CapabilityUnsupported`,
 *   `-32602` invalid_params / `-32600` invalid_request / `-32700`
 *   parse_error → `InvalidArgument`, `-32603` internal_error (and unknown
 *   codes) → `ServiceUnavailable`. Tool-level failures arrive as JSON-RPC
 *   errors with HTTP 200 (wire.rs `tool_call_error`) and take the same path.
 *
 * ## Why this client rides an injected HTTP wire instead of `transport.call`
 * (registered)
 *
 * Three facts the frozen files cannot express: (1) `MethodMap` is type-locked
 * for this change — the intersection lives in `transport.ts` and the facade
 * coverage proof in `client.ts`, both outside the whitelist; (2) `/mcp` is a
 * sibling of `/api/v1`, not under it, so the `HTTP_ROUTES` base-URL rule does
 * not apply; (3) the transport session lives in a **response header**
 * (`Mcp-Session-Id`), which the `call()` pipeline never reads. The route
 * shapes are registered as the WR-F1 extension ledger in the web side's
 * `httpRoutes.ts` (`WR_F1_HTTP_ROUTES`, keys `mcp.*`, base `/`), the
 * executable truth lives here, and `BackendTransport`'s main face is
 * untouched. Architecture guard F-07 holds: no network primitive literal in
 * this package — the fetch implementation is always injected.
 */

import { ApiError, type ApiErrorCode } from './errors';
import {
  apiErrorFromWireResponse,
  createHttpWire,
  jsonBodyOfWireResponse,
  type HttpWireOptions,
} from './types/httpWire';

/** 本实现协商的 MCP 协议版本（crates/datazen-server/src/mcp/wire.rs 的 PROTOCOL_VERSION 同值）。 */
export const MCP_PROTOCOL_VERSION = '2025-03-26';

/** streamable HTTP 传输的会话响应头（wire.rs 的 MCP_SESSION_ID_HEADER 同名）。 */
export const MCP_SESSION_ID_HEADER = 'Mcp-Session-Id';

/**
 * web v1 固定的 serverId：web broker 只面向 datazen 服务端自有工具面
 * （见模块头注的 v1 语义登记）。
 */
export const WEB_MCP_SERVER_ID = 'datazen';

/** web 形态 MCP 客户端的 clientInfo（rmcp `InitializeRequest.clientInfo` 同形状）。 */
const CLIENT_NAME = 'datazen-web';
const CLIENT_VERSION = '0.1.0';

/** `initialize` 结果（wire.rs `initialize_result` 的投影）。 */
export interface McpInitializeResult {
  readonly protocolVersion: string;
  readonly capabilities: Record<string, unknown>;
  readonly serverInfo: { readonly name: string; readonly version: string };
  readonly instructions?: string;
}

/** 服务器暴露的一个工具（`tools/list` 条目；wire.rs `list_tools_result`）。 */
export interface McpToolDescriptor {
  readonly name: string;
  readonly description?: string;
  readonly inputSchema: unknown;
}

/** `tools/list` 结果（无游标分页：nextCursor 恒 null）。 */
export interface McpListToolsResult {
  readonly tools: readonly McpToolDescriptor[];
  readonly nextCursor: string | null;
}

/** JSON-RPC 内容块（server v1 只发 text 块）。 */
export interface McpContentBlock {
  readonly type: string;
  readonly text?: string;
}

/** `tools/call` 结果（rmcp `CallToolResult` 同形状）。 */
export interface McpToolCallResult {
  readonly content: readonly McpContentBlock[];
  readonly isError?: boolean;
}

/** `initialize` 的客户端可调参数（缺省 = web 客户端常量）。 */
export interface McpInitializeOptions {
  readonly clientName?: string;
  readonly clientVersion?: string;
}

/** `mcp.callTool` 的请求。 */
export interface McpToolCallRequest {
  readonly name: string;
  readonly arguments: unknown;
}

/** The mcp domain client surface. */
export interface McpDomainClient {
  initialize(options?: McpInitializeOptions): Promise<McpInitializeResult>;
  listTools(): Promise<McpListToolsResult>;
  callTool(request: McpToolCallRequest): Promise<McpToolCallResult>;
  closeSession(): Promise<void>;
}

/** JSON-RPC 2.0 error 对象（wire.rs `wire_error`：code/message/可选 data）。 */
interface JsonRpcError {
  readonly code: number;
  readonly message: string;
}

const METHOD_NOT_FOUND = -32601;
const INVALID_PARAMS = -32602;
const INVALID_REQUEST = -32600;
const PARSE_ERROR = -32700;

/**
 * JSON-RPC error code → §13 ApiErrorCode（映射表见模块头注；未知码归
 * ServiceUnavailable——「无法分类」的操作语义与后端不可用一致）。
 */
function apiErrorCodeForJsonRpc(code: number): ApiErrorCode {
  if (code === METHOD_NOT_FOUND) return 'CapabilityUnsupported';
  if (code === INVALID_PARAMS || code === INVALID_REQUEST || code === PARSE_ERROR) {
    return 'InvalidArgument';
  }
  return 'ServiceUnavailable';
}

function isJsonRpcError(value: unknown): value is JsonRpcError {
  return (
    typeof value === 'object' &&
    value !== null &&
    typeof (value as Record<string, unknown>)['code'] === 'number' &&
    typeof (value as Record<string, unknown>)['message'] === 'string'
  );
}

function requireRecord(value: unknown, methodLabel: string): Record<string, unknown> {
  if (typeof value !== 'object' || value === null || Array.isArray(value)) {
    throw new ApiError('ServiceUnavailable', `${methodLabel} returned an unreadable response.`);
  }
  return value as Record<string, unknown>;
}

/**
 * Build the mcp domain client over the injected wire options. The base
 * defaults to the **server root** (`''`, same-origin) — `/mcp` is a sibling
 * of `/api/v1`; pass an explicit origin via `HttpWireOptions.baseUrl` for a
 * remote deployment.
 */
export function createMcpClient(wireOptions: HttpWireOptions = {}): McpDomainClient {
  const wire = createHttpWire({ ...wireOptions, baseUrl: wireOptions.baseUrl ?? '' });

  // 传输层会话（Mcp-Session-Id）：initialize 颁发，closeSession 回收。
  let sessionId: string | null = null;
  // JSON-RPC 请求 id：单客户端自增；服务端原样回显。
  let nextRequestId = 1;

  function requireSession(methodLabel: string): string {
    if (sessionId === null) {
      throw new ApiError(
        'InvalidArgument',
        `${methodLabel} requires an established MCP session; call mcp.initialize first.`,
      );
    }
    return sessionId;
  }

  /** 一条 JSON-RPC 请求的完整往返（含会话头与非 2xx 的 §13 回退映射）。 */
  async function rpc(
    methodLabel: string,
    jsonRpcMethod: string,
    params: Record<string, unknown>,
  ): Promise<{ result: unknown; response: Response }> {
    const id = nextRequestId;
    nextRequestId += 1;
    const headers: Record<string, string> = {};
    if (sessionId !== null) headers[MCP_SESSION_ID_HEADER] = sessionId;

    const response = await wire({
      method: 'POST',
      path: '/mcp',
      body: { jsonrpc: '2.0', id, method: jsonRpcMethod, params },
      headers,
    });

    const parsed = (await jsonBodyOfWireResponse(response, methodLabel)) as
      | Record<string, unknown>
      | undefined;
    if (parsed === undefined) {
      // 无体非 2xx（或反常的 2xx 空体）：走 §13 归一口径。
      if (!response.ok) throw await apiErrorFromWireResponse(response, methodLabel);
      throw new ApiError('ServiceUnavailable', `${methodLabel} returned an empty response.`);
    }

    const error = parsed['error'];
    if (isJsonRpcError(error)) {
      // 工具级失败以 HTTP 200 + JSON-RPC error 到达（wire.rs tool_call_error），
      // 非 2xx 的 JSON-RPC 错误体（401/404/400）同路径。
      throw new ApiError(
        apiErrorCodeForJsonRpc(error.code),
        `${methodLabel} failed: ${error.message}`,
        {
          cause: error,
        },
      );
    }
    if (!response.ok) {
      // 服务端骨架 fallback（§13 四字段体）或不可读体。
      throw await apiErrorFromWireResponse(response, methodLabel);
    }
    if (parsed['id'] !== undefined && parsed['id'] !== id) {
      throw new ApiError(
        'ServiceUnavailable',
        `${methodLabel} response id does not match the request id.`,
      );
    }
    if (parsed['jsonrpc'] !== '2.0') {
      throw new ApiError(
        'ServiceUnavailable',
        `${methodLabel} response is not a JSON-RPC 2.0 envelope.`,
      );
    }
    return { result: parsed['result'], response };
  }

  return {
    async initialize(options: McpInitializeOptions = {}): Promise<McpInitializeResult> {
      const methodLabel = 'mcp.initialize';
      const { result, response } = await rpc(methodLabel, 'initialize', {
        protocolVersion: MCP_PROTOCOL_VERSION,
        capabilities: {},
        clientInfo: {
          name: options.clientName ?? CLIENT_NAME,
          version: options.clientVersion ?? CLIENT_VERSION,
        },
      });

      // 会话头只在 initialize 的响应上颁发（http.rs：唯一无需既有会话的请求）。
      const issued = response.headers.get(MCP_SESSION_ID_HEADER);
      if (issued === null || issued.length === 0) {
        throw new ApiError(
          'ServiceUnavailable',
          `${methodLabel} response carried no ${MCP_SESSION_ID_HEADER} header.`,
        );
      }
      sessionId = issued;

      const record = requireRecord(result, methodLabel);
      const serverInfo = requireRecord(record['serverInfo'], methodLabel);
      if (
        typeof record['protocolVersion'] !== 'string' ||
        typeof serverInfo['name'] !== 'string' ||
        typeof serverInfo['version'] !== 'string'
      ) {
        throw new ApiError('ServiceUnavailable', `${methodLabel} returned an unreadable result.`);
      }
      return {
        protocolVersion: record['protocolVersion'],
        capabilities: requireRecord(record['capabilities'], methodLabel),
        serverInfo: { name: serverInfo['name'], version: serverInfo['version'] },
        ...(typeof record['instructions'] === 'string'
          ? { instructions: record['instructions'] }
          : {}),
      };
    },

    async listTools(): Promise<McpListToolsResult> {
      const methodLabel = 'mcp.listTools';
      requireSession(methodLabel);
      const { result } = await rpc(methodLabel, 'tools/list', {});

      const record = requireRecord(result, methodLabel);
      if (!Array.isArray(record['tools'])) {
        throw new ApiError('ServiceUnavailable', `${methodLabel} returned no tools array.`);
      }
      const tools = (record['tools'] as unknown[]).map((entry): McpToolDescriptor => {
        const tool = requireRecord(entry, methodLabel);
        if (typeof tool['name'] !== 'string') {
          throw new ApiError(
            'ServiceUnavailable',
            `${methodLabel} returned a tool without a name.`,
          );
        }
        return {
          name: tool['name'],
          ...(typeof tool['description'] === 'string' ? { description: tool['description'] } : {}),
          inputSchema: tool['inputSchema'],
        };
      });
      const nextCursor = record['nextCursor'];
      return {
        tools,
        nextCursor: typeof nextCursor === 'string' ? nextCursor : null,
      };
    },

    async callTool(request: McpToolCallRequest): Promise<McpToolCallResult> {
      const methodLabel = 'mcp.callTool';
      if (typeof request.name !== 'string' || request.name.length === 0) {
        throw new ApiError('InvalidArgument', `${methodLabel} requires a non-empty tool name.`);
      }
      requireSession(methodLabel);
      const { result } = await rpc(methodLabel, 'tools/call', {
        name: request.name,
        arguments: request.arguments,
      });

      const record = requireRecord(result, methodLabel);
      if (!Array.isArray(record['content'])) {
        throw new ApiError('ServiceUnavailable', `${methodLabel} returned no content array.`);
      }
      const content = (record['content'] as unknown[]).map((entry): McpContentBlock => {
        const block = requireRecord(entry, methodLabel);
        return {
          type: typeof block['type'] === 'string' ? block['type'] : '',
          ...(typeof block['text'] === 'string' ? { text: block['text'] } : {}),
        };
      });
      return {
        content,
        ...(record['isError'] === true ? { isError: true } : {}),
      };
    },

    async closeSession(): Promise<void> {
      // 幂等无操作：会话从未建立（或已回收）时无事可做。
      if (sessionId === null) return;
      const methodLabel = 'mcp.closeSession';
      const headers = { [MCP_SESSION_ID_HEADER]: sessionId };
      const response = await wire({ method: 'DELETE', path: '/mcp', headers });
      sessionId = null;
      if (!response.ok) {
        throw await apiErrorFromWireResponse(response, methodLabel);
      }
      // 202 Accepted，空体：服务端已销毁会话状态。
    },
  };
}

/** Compile-time: the client serves exactly its four contract methods. */
type Assert<T extends true> = T;
export type McpClientCoversContract = Assert<
  'initialize' | 'listTools' | 'callTool' | 'closeSession' extends keyof McpDomainClient
    ? true
    : false
>;
