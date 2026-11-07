/**
 * WR-F1 web AI 装配测试 —— node 环境、全手写假体（假 transport，风格对齐
 * platformServices / domainContractTables）：
 *
 * 断言样式复制自桌面侧 AI 装配测试（`crates/datazen/src/ai/application/`
 * `tool_loop.rs` 的 `StubHost` / `chat_session_tests.rs` 的
 * `RecordingLoopHost` / `tools_tests.rs` 的假宿主）：**循环只见端口**——
 * 假 provider + 假 `AiToolLoopHost`，循环体零形态分支，行为与桌面完全
 * 同判。web 三端口（requestHost / mcpBroker / secrets）的 wire 线形与
 * JSON-RPC/§13 错误归一在 `mcpSecurityAssembly.test.ts` 逐条钉住；本文件
 * 保留循环同判与装配面里不依赖出站 HTTP 的两半（执行设置、引用解析）。
 */
import { describe, expect, it, vi } from 'vitest';
import type { BackendClient, BackendTransport, MethodMap } from '@datazen/backend-client';
import {
  runAiToolLoop,
  splitQualifiedToolName,
  type AiProviderClient,
  type AiToolLoopHost,
} from '../ai/aiPorts';
import { createWebAiBindings, parseSecretRef } from '../ai/webAiAssembly';

/** 假 transport：call 记录方法名、按表回话（web 装配测试共用）。 */
function fakeTransport(
  respond: (method: string) => unknown = () => ({}),
): BackendTransport & { calls: string[] } {
  const calls: string[] = [];
  return {
    calls,
    unsupportedMethods: {},
    async call(method: keyof MethodMap) {
      calls.push(String(method));
      return respond(String(method));
    },
    // subscribe/cancel 在 AI 装配面不可达；假体显式失败以防被悄悄走到。
    subscribe(): never {
      throw new Error('fake transport: subscribe is unreachable in WR-F1 tests');
    },
    async cancel(): Promise<void> {
      calls.push('cancel');
    },
  } as unknown as BackendTransport & { calls: string[] };
}

/** 假 backend facade：只有 AI 宿主读面用到的设置域。 */
function fakeClient(settings: unknown): BackendClient {
  return {
    getSettings: vi.fn(async () => settings),
  } as unknown as BackendClient;
}

/** 假 provider：脚本化逐轮回复（桌面 tool_loop 测试的 `StubProvider` 同款）。 */
function scriptedProvider(
  rounds: { content?: string; toolCalls?: { name: string; arguments: string }[] }[],
) {
  let next = 0;
  const seenTranscripts: unknown[][] = [];
  const provider: AiProviderClient = {
    async completeRound(messages) {
      seenTranscripts.push([...messages]);
      return rounds[Math.min(next++, rounds.length - 1)];
    },
  };
  return { provider, seenTranscripts };
}

/** 记录型循环宿主（`chat_session_tests.rs` 的 `RecordingLoopHost` 同款）。 */
function recordingHost(overrides: Partial<AiToolLoopHost> = {}): AiToolLoopHost & {
  dbCalls: string[];
  mcpCalls: string[];
} {
  const dbCalls: string[] = [];
  const mcpCalls: string[] = [];
  return {
    dbCalls,
    mcpCalls,
    async egressSummary() {
      return 'egress: none';
    },
    async executeDbTool(call) {
      dbCalls.push(call.name);
      return `db-ok:${call.name}`;
    },
    async executeMcpTool(server, tool) {
      mcpCalls.push(`${server}/${tool}`);
      return `mcp-ok:${server}/${tool}`;
    },
    ...overrides,
  };
}

describe('WR-F1 AI tool loop (form-agnostic)', () => {
  it('routes db and mcp calls purely by qualified name and feeds results back', async () => {
    const { provider, seenTranscripts } = scriptedProvider([
      {
        toolCalls: [
          { name: 'list_tables', arguments: '{}' },
          { name: 'mcp/srv-1/search_docs', arguments: '{"q":"x"}' },
        ],
      },
      { content: 'done' },
    ]);
    const host = recordingHost();

    const answer = await runAiToolLoop(provider, host, 4, [{ role: 'user', content: 'hi' }]);

    expect(answer).toBe('done');
    expect(host.dbCalls).toEqual(['list_tables']);
    expect(host.mcpCalls).toEqual(['srv-1/search_docs']);
    // 第二轮 transcript 回灌了两个工具结果——循环的内存只经端口。
    expect(seenTranscripts[1]).toEqual([
      { role: 'user', content: 'hi' },
      { tool: 'list_tables', result: 'db-ok:list_tables' },
      { tool: 'mcp/srv-1/search_docs', result: 'mcp-ok:srv-1/search_docs' },
    ]);
  });

  it('folds host failures into text and keeps the loop running (never throws form errors)', async () => {
    const { provider } = scriptedProvider([
      { toolCalls: [{ name: 'mcp/srv-2/broken', arguments: '{}' }] },
      { content: 'recovered' },
    ]);
    // 宿主方法抛错（模拟 MCP server 不可达），但调用仍要留痕——
    // overrides 是整体替换，替身需自行先记录再抛（与 RecordingLoopHost
    // 的记录语义一致：记录发生在分发点，不因结果成败而异）。
    const host = recordingHost();
    host.executeMcpTool = async (server, tool) => {
      host.mcpCalls.push(`${server}/${tool}`);
      throw new Error('mcp server unavailable');
    };

    const answer = await runAiToolLoop(provider, host, 3, []);

    expect(answer).toBe('recovered');
    expect(host.mcpCalls).toEqual(['srv-2/broken']);
  });

  it('exhausts bounded rounds into an explicit error text', async () => {
    const loopingCall = { name: 'list_tables', arguments: '{}' };
    const { provider } = scriptedProvider([{ toolCalls: [loopingCall] }]);
    const host = recordingHost();

    const answer = await runAiToolLoop(provider, host, 2, []);

    expect(answer).toMatch(/Error: tool loop exhausted 2 rounds/);
    expect(host.dbCalls).toHaveLength(2);
  });

  it('splitQualifiedToolName follows the mcp/{serverId}/{toolName} routing rule', () => {
    expect(splitQualifiedToolName('mcp/a/b')).toEqual({ serverId: 'a', toolName: 'b' });
    expect(splitQualifiedToolName('mcp/a')).toBeNull();
    expect(splitQualifiedToolName('mcp//b')).toBeNull();
    expect(splitQualifiedToolName('list_tables')).toBeNull();
  });
});

describe('WR-F1 web AI assembly over the WebClient transport', () => {
  it('requestHost reads execution settings through the settings domain (same underlying facts as desktop)', async () => {
    const transport = fakeTransport();
    const client = fakeClient({
      execution: { allowWrites: false, timeoutMs: 5000 },
    });
    const { requestHost } = createWebAiBindings(client, transport);

    const settings = await requestHost.executionSettings();

    expect(settings).toEqual({ allowWrites: false, timeoutMs: 5000 });
    expect(client.getSettings).toHaveBeenCalledTimes(1);
    expect(transport.calls).toEqual([]);
  });

  it('secrets parse as server-side references at the assembly boundary: bad refs never become an outbound request', () => {
    expect(parseSecretRef({ domain: 'ai-provider', key: 'openai' })).toEqual({
      domain: 'ai-provider',
      key: 'openai',
    });
    expect(() => parseSecretRef({ domain: 'ai-provider', key: '' })).toThrowError(
      /references, never as values/,
    );
    expect(() => parseSecretRef({ domain: 'browser-local', key: 'k' })).toThrowError();
    expect(() => parseSecretRef(null)).toThrowError();
  });

  it('two-argument assembly stays backward compatible and exposes the wire-direct clients it holds', () => {
    // 生产形态（assembleWebPlatform）的两参调用；wire-direct 客户端缺省
    // 自建（node 下 fetch 缺省绑定全局，不出站 —— 三端口的具体线形由
    // mcpSecurityAssembly.test.ts 注入假体逐条验证）。
    const bindings = createWebAiBindings(fakeClient({}), fakeTransport());
    expect(bindings.requestHost).toBeDefined();
    expect(bindings.mcpBroker).toBeDefined();
    expect(bindings.secrets).toBeDefined();
    expect(bindings.security).toBeDefined();
    expect(bindings.mcp).toBeDefined();
  });
});
