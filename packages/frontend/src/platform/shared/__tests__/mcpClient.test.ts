import { describe, expect, it, vi } from 'vitest';

import { formatCallToolResult, mcpQualifiedName, WebMcpClientManager } from '../mcpClient';
import type { McpHttpResponse, WebMcpServerConfig } from '../mcpClient';

/**
 * WR-F3 web MCP client 层：与桌面 `McpClientManager`（stdio 子进程）语义同构，
 * 传输换成 streamable HTTP。所有 fetch 注入假传输 —— 零网络。
 */

const MCP_URL = 'https://mcp.example.com/mcp';
const OTHER_URL = 'https://other.example.com/mcp';
const SESSION = 'sess-42';

const CONFIG: WebMcpServerConfig = {
  serverId: 'example',
  url: MCP_URL,
  headers: { Authorization: 'Bearer t' },
};

type RpcBody = { id?: number; method: string; params?: Record<string, unknown> };
type Responder = (body: RpcBody) => McpHttpResponse;

interface SentRequest {
  url: string;
  method: string;
  headers: Record<string, string>;
  body: RpcBody | null;
}

function rawResponse(
  status: number,
  text: string,
  headers: Record<string, string> = {},
): McpHttpResponse {
  return {
    ok: status >= 200 && status < 300,
    status,
    statusText: `status-${status}`,
    headers: { get: (name) => headers[name.toLowerCase()] ?? null },
    text: async () => text,
  };
}

function jsonResponse(
  status: number,
  body: unknown,
  headers: Record<string, string> = {},
): McpHttpResponse {
  return rawResponse(status, JSON.stringify(body), headers);
}

function rpcResult(
  id: number,
  result: unknown,
  headers: Record<string, string> = {},
): McpHttpResponse {
  return jsonResponse(200, { jsonrpc: '2.0', id, result }, headers);
}

function rpcError(id: number, code: number, message: string): McpHttpResponse {
  return jsonResponse(200, { jsonrpc: '2.0', id, error: { code, message } });
}

function sseResponse(messages: unknown[]): McpHttpResponse {
  const body = messages.map((m) => `data: ${JSON.stringify(m)}\n\n`).join('');
  return rawResponse(200, body, { 'content-type': 'text/event-stream' });
}

const TOOLS = [
  {
    name: 'echo',
    description: 'Echoes input',
    inputSchema: { type: 'object', properties: { text: { type: 'string' } } },
  },
  { name: 'ping', inputSchema: {} },
];

/** Happy-path fake server: initialize → initialized → tools/list (+ tools/call, DELETE). */
function fakeMcpServer(overrides: Record<string, Responder> = {}) {
  const responders: Record<string, Responder> = {
    initialize: (b) =>
      rpcResult(
        b.id ?? 0,
        { protocolVersion: '2025-06-18', serverInfo: { name: 'Example MCP' } },
        { 'mcp-session-id': SESSION },
      ),
    'notifications/initialized': () => jsonResponse(202, null),
    'tools/list': (b) => rpcResult(b.id ?? 0, { tools: TOOLS }),
    'tools/call': (b) =>
      rpcResult(b.id ?? 0, {
        content: [
          { type: 'text', text: `echo:${(b.params?.arguments as { text?: string }).text ?? ''}` },
        ],
        isError: false,
      }),
    ...overrides,
  };
  return vi.fn(async (_url: string, init: RequestInit): Promise<McpHttpResponse> => {
    if (init.method === 'DELETE') return jsonResponse(200, null);
    const body = JSON.parse(String(init.body)) as RpcBody;
    const respond = responders[body.method];
    if (!respond) return jsonResponse(500, { error: `unexpected method ${body.method}` });
    return respond(body);
  });
}

async function connected(overrides: Record<string, Responder> = {}) {
  const fetchFn = fakeMcpServer(overrides);
  const manager = new WebMcpClientManager(fetchFn);
  await manager.connect(CONFIG);
  return { manager, fetchFn };
}

function sentRequests(fetchFn: ReturnType<typeof fakeMcpServer>): SentRequest[] {
  return fetchFn.mock.calls.map(([url, init]) => ({
    url,
    method: init.method,
    headers: (init.headers ?? {}) as Record<string, string>,
    body: init.body ? (JSON.parse(String(init.body)) as RpcBody) : null,
  }));
}

describe('WebMcpClientManager.connect — initialize 握手', () => {
  it('performs the initialize handshake and echoes the session header back', async () => {
    const { manager, fetchFn } = await connected();

    const reqs = sentRequests(fetchFn);
    expect(reqs).toHaveLength(3);

    const [init, initialized, list] = reqs;
    // initialize：POST + Accept 双通道 + 注入的自定义头；此时还没有会话头。
    expect(init?.url).toBe(MCP_URL);
    expect(init?.method).toBe('POST');
    expect(init?.headers['Content-Type']).toBe('application/json');
    expect(init?.headers['Accept']).toBe('application/json, text/event-stream');
    expect(init?.headers.Authorization).toBe('Bearer t');
    expect(init?.headers['Mcp-Session-Id']).toBeUndefined();
    expect(init?.body).toMatchObject({
      jsonrpc: '2.0',
      id: 1,
      method: 'initialize',
      params: {
        protocolVersion: '2025-06-18',
        capabilities: {},
        clientInfo: { name: 'datazen-web' },
      },
    });
    // notifications/initialized 无 id；tools/list 是第一个带 id 的后续请求。
    expect(initialized?.body).toEqual({ jsonrpc: '2.0', method: 'notifications/initialized' });
    expect(list?.body).toMatchObject({ jsonrpc: '2.0', id: 2, method: 'tools/list' });
    // Mcp-Session-Id 响应头已保存，并在所有后续请求回传。
    expect(initialized?.headers['Mcp-Session-Id']).toBe(SESSION);
    expect(list?.headers['Mcp-Session-Id']).toBe(SESSION);

    // serverName 来自 initialize 的 serverInfo.name；tools/list 已在 connect 时缓存。
    expect(manager.connectedServers()).toEqual([
      { serverId: 'example', serverName: 'Example MCP', toolCount: 2 },
    ]);
    const tools = await manager.tools();
    expect(tools).toHaveLength(2);
    expect(tools[0]).toEqual({
      serverId: 'example',
      serverName: 'Example MCP',
      toolName: 'echo',
      qualifiedName: 'mcp/example/echo',
      description: 'Echoes input',
      inputSchema: { type: 'object', properties: { text: { type: 'string' } } },
    });
    expect(tools[1]?.description).toBeNull(); // 桌面 Option::None 同构 → null
  });

  it('a stateless server (no session header) never sends a session header or DELETE', async () => {
    const fetchFn = fakeMcpServer({
      initialize: (b) => rpcResult(b.id ?? 0, { serverInfo: { name: 'S' } }),
    });
    const manager = new WebMcpClientManager(fetchFn);
    await manager.connect({ serverId: 'stateless', url: MCP_URL });

    for (const req of sentRequests(fetchFn).slice(1)) {
      if (req.method === 'POST') {
        expect(req.headers['Mcp-Session-Id']).toBeUndefined();
      }
    }
    await manager.disconnect('stateless');
    expect(sentRequests(fetchFn).some((r) => r.method === 'DELETE')).toBe(false);
  });

  it('paginates tools/list via nextCursor', async () => {
    let listCalls = 0;
    const { manager, fetchFn } = await connected({
      'tools/list': (b) => {
        listCalls += 1;
        return listCalls === 1
          ? rpcResult(b.id ?? 0, { tools: [{ name: 'a', inputSchema: {} }], nextCursor: 'page-2' })
          : rpcResult(b.id ?? 0, { tools: [{ name: 'b', inputSchema: {} }] });
      },
    });

    const tools = await manager.tools();
    expect(tools.map((t) => t.toolName)).toEqual(['a', 'b']);
    const secondPage = sentRequests(fetchFn)[3];
    expect(secondPage?.body).toMatchObject({ method: 'tools/list', params: { cursor: 'page-2' } });
  });

  it('rejects invalid server ids before any request', async () => {
    const fetchFn = fakeMcpServer();
    const manager = new WebMcpClientManager(fetchFn);
    await expect(manager.connect({ ...CONFIG, serverId: 'bad/id' })).rejects.toThrow(
      "Invalid MCP server id 'bad/id': must match ^[a-zA-Z0-9_-]+$",
    );
    await expect(manager.connect({ ...CONFIG, serverId: '' })).rejects.toThrow(
      'Invalid MCP server id',
    );
    expect(sentRequests(fetchFn)).toHaveLength(0);
    expect(manager.connectedServers()).toEqual([]);
  });
});

describe('WebMcpClientManager.tools — 缓存', () => {
  it('serves repeat calls from the cache without new requests', async () => {
    const { manager, fetchFn } = await connected();

    expect(sentRequests(fetchFn)).toHaveLength(3); // initialize + initialized + tools/list
    await manager.tools();
    await manager.tools('example');
    expect(sentRequests(fetchFn)).toHaveLength(3);
  });

  it('flattens all servers, and filters by serverId on request', async () => {
    const fetchFn = fakeMcpServer();
    const manager = new WebMcpClientManager(fetchFn);
    await manager.connect(CONFIG);
    await manager.connect({ serverId: 'second', url: OTHER_URL });

    const all = await manager.tools();
    expect(all).toHaveLength(4);
    expect(all.every((t) => t.serverId === 'example' || t.serverId === 'second')).toBe(true);
    const only = await manager.tools('second');
    expect(only.every((t) => t.serverId === 'second')).toBe(true);
    expect(manager.connectedServers().map((s) => s.serverId)).toEqual(['example', 'second']);
  });
});

describe('WebMcpClientManager.callTool — result/error 两通道', () => {
  it('posts tools/call with the session header and returns the flattened text', async () => {
    const { manager, fetchFn } = await connected();

    const out = await manager.callTool('example', 'echo', { text: 'hi' });
    expect(out).toBe('echo:hi');

    const call = sentRequests(fetchFn)[3];
    expect(call?.body).toMatchObject({
      jsonrpc: '2.0',
      method: 'tools/call',
      params: { name: 'echo', arguments: { text: 'hi' } },
    });
    expect(call?.headers['Mcp-Session-Id']).toBe(SESSION);
  });

  it('joins multiple text content blocks with newline', async () => {
    const { manager } = await connected({
      'tools/call': (b) =>
        rpcResult(b.id ?? 0, {
          content: [{ type: 'text', text: 'a' }, { type: 'text', text: 'b' }],
        }),
    });
    await expect(manager.callTool('example', 'echo', {})).resolves.toBe('a\nb');
  });

  it('an isError result throws the format_call_tool_result failure channel text', async () => {
    const { manager } = await connected({
      'tools/call': (b) =>
        rpcResult(b.id ?? 0, { content: [{ type: 'text', text: 'boom' }], isError: true }),
    });
    await expect(manager.callTool('example', 'echo', {})).rejects.toThrow(
      'MCP tool error (mcp/example/echo): boom',
    );
  });

  it('a JSON-RPC error throws "Tool call failed: …" (desktop map_err parity)', async () => {
    const { manager } = await connected({
      'tools/call': (b) => rpcError(b.id ?? 0, -32602, 'unknown tool'),
    });
    await expect(manager.callTool('example', 'echo', {})).rejects.toThrow(
      'Tool call failed: MCP error -32602: unknown tool',
    );
  });

  it('non-text-only content resolves with the non-text summary (success channel)', async () => {
    const { manager } = await connected({
      'tools/call': (b) => rpcResult(b.id ?? 0, { content: [{ type: 'image', data: '...' }] }),
    });
    await expect(manager.callTool('example', 'shot', {})).resolves.toBe(
      'MCP tool (mcp/example/shot) returned 1 non-text content block(s); no text output available.',
    );
  });

  it('non-object arguments throw the desktop error text', async () => {
    const { manager } = await connected();
    await expect(manager.callTool('example', 'echo', 'nope')).rejects.toThrow(
      'Tool arguments must be a JSON object',
    );
    await expect(manager.callTool('example', 'echo', [1, 2])).rejects.toThrow(
      'Tool arguments must be a JSON object',
    );
  });
});

describe('SSE replies', () => {
  it('parses an SSE tools/list reply, skipping interleaved notifications', async () => {
    const { manager } = await connected({
      'tools/list': (b) =>
        sseResponse([
          { jsonrpc: '2.0', method: 'notifications/progress' }, // interleaved, no id — ignored
          {
            jsonrpc: '2.0',
            id: b.id ?? 0,
            result: { tools: [{ name: 'sse-tool', inputSchema: {} }] },
          },
        ]),
    });

    const tools = await manager.tools();
    expect(tools).toHaveLength(1);
    expect(tools[0]).toMatchObject({ toolName: 'sse-tool', qualifiedName: 'mcp/example/sse-tool' });
  });
});

describe('WebMcpClientManager.disconnect — 清理', () => {
  it('sends DELETE with the session header and clears the registry', async () => {
    const { manager, fetchFn } = await connected();

    await manager.disconnect('example');

    const del = sentRequests(fetchFn).find((r) => r.method === 'DELETE');
    expect(del?.url).toBe(MCP_URL);
    expect(del?.headers['Mcp-Session-Id']).toBe(SESSION);
    expect(del?.headers.Authorization).toBe('Bearer t');
    expect(manager.connectedServers()).toEqual([]);

    await expect(manager.callTool('example', 'echo', {})).rejects.toThrow(
      "MCP server 'example' not connected",
    );
    await expect(manager.tools('example')).rejects.toThrow("MCP server 'example' not connected");
  });

  it('disconnecting an unknown server is a noop with no requests', async () => {
    const { manager, fetchFn } = await connected();

    await expect(manager.disconnect('missing')).resolves.toBeUndefined();
    expect(sentRequests(fetchFn)).toHaveLength(3);
  });

  it('disconnectAll clears every server', async () => {
    const { manager } = await connected();
    await manager.connect({ serverId: 'second', url: OTHER_URL });

    await manager.disconnectAll();
    expect(manager.connectedServers()).toEqual([]);
    await expect(manager.callTool('second', 'ping', {})).rejects.toThrow('not connected');
  });
});

describe('握手失败通道', () => {
  it('initialize HTTP failure → "Failed to initialize MCP client"', async () => {
    const fetchFn = fakeMcpServer({ initialize: () => jsonResponse(500, { message: 'nope' }) });
    const manager = new WebMcpClientManager(fetchFn);

    await expect(manager.connect(CONFIG)).rejects.toThrow('Failed to initialize MCP client');
    expect(manager.connectedServers()).toEqual([]);
  });

  it('initialize JSON-RPC error → same prefix with the MCP error text', async () => {
    const fetchFn = fakeMcpServer({
      initialize: (b) => rpcError(b.id ?? 0, -32600, 'bad request'),
    });
    const manager = new WebMcpClientManager(fetchFn);

    await expect(manager.connect(CONFIG)).rejects.toThrow(
      'Failed to initialize MCP client: MCP error -32600: bad request',
    );
  });

  it('rejected initialized notification → "Failed to initialize MCP client"', async () => {
    const fetchFn = fakeMcpServer({
      'notifications/initialized': () => jsonResponse(500, null),
    });
    const manager = new WebMcpClientManager(fetchFn);

    await expect(manager.connect(CONFIG)).rejects.toThrow('Failed to initialize MCP client');
    expect(manager.connectedServers()).toEqual([]);
  });

  it('tools/list failure → "Failed to list tools" + session cleanup', async () => {
    const fetchFn = fakeMcpServer({ 'tools/list': () => jsonResponse(500, null) });
    const manager = new WebMcpClientManager(fetchFn);

    await expect(manager.connect(CONFIG)).rejects.toThrow('Failed to list tools');
    const del = sentRequests(fetchFn).find((r) => r.method === 'DELETE');
    expect(del?.headers['Mcp-Session-Id']).toBe(SESSION);
    expect(manager.connectedServers()).toEqual([]);
  });
});

describe('registry lifecycle', () => {
  it('reconnecting the same id replaces the old client and terminates its session', async () => {
    const { manager, fetchFn } = await connected();

    await manager.connect(CONFIG); // same id again

    const reqs = sentRequests(fetchFn);
    expect(reqs.filter((r) => r.method === 'DELETE')).toHaveLength(1); // old session torn down
    expect(reqs.filter((r) => r.body?.method === 'initialize')).toHaveLength(2);
    expect(manager.connectedServers()).toEqual([
      { serverId: 'example', serverName: 'Example MCP', toolCount: 2 },
    ]);
  });
});

describe('format helpers — desktop parity', () => {
  it('mcpQualifiedName matches the shared routing rule', () => {
    expect(mcpQualifiedName('my-server_1', 'search')).toBe('mcp/my-server_1/search');
  });

  it('formatCallToolResult mirrors format_call_tool_result channel by channel', () => {
    expect(formatCallToolResult({ content: [{ type: 'text', text: 'x' }] }, 'mcp/s/t')).toBe('x');
    expect(formatCallToolResult({ content: [], isError: true }, 'mcp/s/t')).toBe(
      'MCP tool error (mcp/s/t): ',
    );
    expect(
      formatCallToolResult({ content: [{ type: 'image' }, { type: 'resource' }] }, 'q'),
    ).toBe('MCP tool (q) returned 2 non-text content block(s); no text output available.');
  });
});
