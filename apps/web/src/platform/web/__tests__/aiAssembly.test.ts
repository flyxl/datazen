/**
 * WR-F1 web AI 装配测试 —— node 环境、全手写假体（假 transport，风格对齐
 * platformServices / domainContractTables）：
 *
 * 断言样式复制自桌面侧 AI 装配测试（`modules/src/ai/application/`
 * `tool_loop.rs` 的 `StubHost` / `chat_session_tests.rs` 的
 * `RecordingLoopHost` / `tools_tests.rs` 的假宿主）：**循环只见端口**——
 * 假 provider + 假 `AiToolLoopHost`，循环体零形态分支，行为与桌面完全
 * 同判；web 三端口（requestHost / mcpBroker / secrets）在假 transport 上
 * 各自给出「已接的路由」或「命名的 capability 缺口」。
 */
import { describe, expect, it, vi } from 'vitest';
import type { BackendClient, BackendTransport, MethodMap } from '@datazen/backend-client';
import {
  runAiToolLoop,
  splitQualifiedToolName,
  type AiProviderClient,
  type AiToolLoopHost,
} from '../ai/aiPorts';
import {
  createWebAiBindings,
  parseSecretRef,
  WEB_AI_HOST_GAP,
  WEB_MCP_BROKER_GAP,
  WEB_SECRET_PROVIDER_GAP,
} from '../ai/webAiAssembly';

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
function scriptedProvider(rounds: { content?: string; toolCalls?: { name: string; arguments: string }[] }[]) {
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

  it('requestHost names the missing AI tool-host domain for unrouted faces (claim kept in the message)', async () => {
    const { requestHost } = createWebAiBindings(fakeClient({}), fakeTransport());
    await expect(requestHost.getSessionConfig('session-1')).rejects.toMatchObject({
      code: 'CapabilityUnsupported',
    });
    await expect(requestHost.executeTarget('conn-1', 'select', {}, { kind: 'independent' })).rejects.toThrowError(
      WEB_AI_HOST_GAP,
    );
    await expect(
      requestHost.executeTarget('conn-1', 'select', {}, { kind: 'editorGrant', grantId: 'g-1' }),
    ).rejects.toThrowError(/claim: editorGrant/);
  });

  it('mcpBroker surfaces the named capability gap — never a silent empty tool list', async () => {
    const transport = fakeTransport();
    const { mcpBroker } = createWebAiBindings(fakeClient({}), transport);

    await expect(mcpBroker.toolsForAi()).rejects.toThrowError(WEB_MCP_BROKER_GAP);
    await expect(mcpBroker.callTool('srv-1', 'tool', {})).rejects.toMatchObject({
      code: 'CapabilityUnsupported',
      unsupportedKind: 'capability',
    });
    // 缺口登记在案：transport 从未被调用（没有可命中的 mcp 域路由）。
    expect(transport.calls).toEqual([]);
  });

  it('secrets resolve as server-side references: parsing rejects bad refs, resolution names the wiring gap', async () => {
    const { secrets } = createWebAiBindings(fakeClient({}), fakeTransport());

    expect(parseSecretRef({ domain: 'ai-provider', key: 'openai' })).toEqual({
      domain: 'ai-provider',
      key: 'openai',
    });
    expect(() => parseSecretRef({ domain: 'ai-provider', key: '' })).toThrowError(/references, never as values/);
    expect(() => parseSecretRef({ domain: 'browser-local', key: 'k' })).toThrowError();
    expect(() => parseSecretRef(null)).toThrowError();

    // 密钥本体永不进浏览器：安全域方法缺失 = WR-F1#2 的命名缺口。
    await expect(secrets.resolveSecret({ domain: 'ai-provider', key: 'openai' })).rejects.toThrowError(
      WEB_SECRET_PROVIDER_GAP,
    );
    await expect(secrets.resolveSecret({ domain: 'ai-provider', key: 'openai' })).rejects.toMatchObject({
      code: 'CapabilityUnsupported',
      unsupportedKind: 'capability',
    });
  });
});
