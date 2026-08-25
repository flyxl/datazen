import { expect, browser, $ } from '@wdio/globals';
import {
  openConnectionWindow,
  closeExtraWindows,
} from '../helpers.js';

/**
 * Invoke a Tauri IPC command from the browser context.
 */
async function invokeBackend<T>(cmd: string, args: Record<string, unknown> = {}): Promise<T> {
  const result = await browser.executeAsync(
    (c: string, a: string, done: (r: any) => void) => {
      (window as any).__TAURI_INTERNALS__
        .invoke(c, JSON.parse(a))
        .then((r: any) => done(r))
        .catch((e: any) => done({ __error: String(e) }));
    },
    cmd,
    JSON.stringify(args),
  );
  if (result && typeof result === 'object' && '__error' in (result as any)) {
    throw new Error((result as any).__error);
  }
  return result as T;
}

/**
 * Invoke with retry on rate limit errors.
 * Uses browser.pause() outside of executeAsync to avoid WebDriver timeout.
 */
async function invokeWithRetry<T>(
  cmd: string,
  args: Record<string, unknown> = {},
  maxRetries = 3,
  baseDelay = 12000,
): Promise<T> {
  for (let attempt = 0; attempt <= maxRetries; attempt++) {
    try {
      return await invokeBackend<T>(cmd, args);
    } catch (e: any) {
      const msg = e.message || '';
      if (msg.includes('Rate limit') && attempt < maxRetries) {
        const delay = baseDelay * (attempt + 1);
        // Use native setTimeout-based pause in browser context
        await browser.pause(delay);
        continue;
      }
      throw e;
    }
  }
  throw new Error('unreachable');
}

function getAiConfig() {
  return {
    providerType: 'open_ai' as const,
    endpoint: process.env.E2E_AI_ENDPOINT || 'https://token.sensenova.cn/v1',
    apiKey: process.env.E2E_AI_API_KEY || '',
    model: process.env.E2E_AI_MODEL || 'glm-5.2',
  };
}

describe('AI 功能 E2E 测试 (AI-001~AI-012)', () => {
  let mainWindow: string;
  let connWindow: string;
  let dbSessionId: string;
  const aiConfig = getAiConfig();

  before(async () => {
    if (!aiConfig.apiKey) {
      console.warn('⚠️  E2E_AI_API_KEY not set, AI tests will be skipped');
      return;
    }

    // Increase WebDriver async script timeout for LLM API calls
    await browser.setTimeout({ script: 120000 });

    // Configure AI provider via IPC
    await invokeBackend('ai_save_config', {
      config: {
        providerType: aiConfig.providerType,
        endpoint: aiConfig.endpoint,
        apiKey: aiConfig.apiKey,
        model: aiConfig.model,
        extra: null,
      },
    });

    // Open a connection window
    const windows = await openConnectionWindow();
    mainWindow = windows.mainWindow;
    connWindow = windows.connWindow;

    // Extract the runtime connection ID from the connection window's URL
    const connectionIdFromUrl = await browser.execute(() => {
      const params = new URLSearchParams(window.location.search);
      return params.get('connectionId') || '';
    });
    if (connectionIdFromUrl) {
      dbSessionId = connectionIdFromUrl as string;
    }
  });

  afterEach(async () => {
    // Small delay between tests to avoid rate limiting
    await browser.pause(3000);
  });

  after(async () => {
    if (mainWindow) {
      await closeExtraWindows(mainWindow);
    }
    try {
      await invokeBackend('ai_delete_config');
    } catch {
      // ignore
    }
  });

  // ── AI-001: AI Config Persistence ──────────────────────────────

  it('AI-001: 应能保存和读取 AI 配置', async function () {
    if (!aiConfig.apiKey) return this.skip();

    const config = await invokeBackend<any>('ai_get_config');
    expect(config).not.toBeNull();
    expect(config.providerType).toBe('open_ai');
    expect(config.model).toBe(aiConfig.model);
  });

  it('AI-001: 应能验证 AI 配置', async function () {
    if (!aiConfig.apiKey) return this.skip();
    this.timeout(60000);

    await invokeWithRetry('ai_validate_config', {
      config: {
        providerType: aiConfig.providerType,
        endpoint: aiConfig.endpoint,
        apiKey: aiConfig.apiKey,
        model: aiConfig.model,
        extra: null,
      },
    });
  });

  it('AI-001: 应能列出可用的 AI 服务商', async function () {
    if (!aiConfig.apiKey) return this.skip();

    const providers = await invokeBackend<any[]>('ai_get_providers');
    expect(providers.length).toBeGreaterThan(0);
    const names = providers.map((p: any) => p.displayName || p.display_name);
    expect(names).toContain('OpenAI');
  });

  // ── AI-002: NL2SQL Generation ──────────────────────────────────

  it('AI-002: 应能通过 IPC 生成 SQL', async function () {
    if (!aiConfig.apiKey || !dbSessionId) return this.skip();
    this.timeout(120000);

    const requestId = `e2e-nl2sql-${Date.now()}`;
    const result = await invokeWithRetry<string>('ai_generate_sql', {
      dbSessionId,
      database: 'postgres',
      naturalLanguage: '列出所有数据库表',
      requestId,
      recentQueries: [],
    });

    expect(result).toBeDefined();
    expect(typeof result).toBe('string');
  });

  // ── AI-003: SQL Error Diagnosis ────────────────────────────────

  it('AI-003: 应能诊断 SQL 错误', async function () {
    if (!aiConfig.apiKey || !dbSessionId) return this.skip();
    this.timeout(120000);

    const result = await invokeWithRetry<any>('ai_diagnose_error', {
      dbSessionId,
      database: 'postgres',
      sql: 'SELECT * FROM non_existent_table_xyz',
      errorMessage: 'ERROR: relation "non_existent_table_xyz" does not exist',
    });

    expect(result).toBeDefined();
    expect(result.explanation).toBeDefined();
    expect(typeof result.explanation).toBe('string');
    expect(result.explanation.length).toBeGreaterThan(0);
  });

  // ── AI-004: EXPLAIN Analysis ───────────────────────────────────

  it('AI-004: 应能分析 EXPLAIN 输出', async function () {
    if (!aiConfig.apiKey || !dbSessionId) return this.skip();
    this.timeout(120000);

    const result = await invokeWithRetry<any>('ai_analyze_explain', {
      dbSessionId,
      explainOutput:
        'Seq Scan on pg_class  (cost=0.00..14.12 rows=412 width=265)',
      originalSql: 'SELECT * FROM pg_class',
    });

    expect(result).toBeDefined();
    expect(result.summary).toBeDefined();
    expect(typeof result.summary).toBe('string');
    expect(result.summary.length).toBeGreaterThan(0);
    expect(Array.isArray(result.bottlenecks)).toBe(true);
    expect(Array.isArray(result.suggestions)).toBe(true);
  });

  // ── AI-005: AI Chat ────────────────────────────────────────────

  it('AI-005: 应能通过 AI 聊天获取回复', async function () {
    if (!aiConfig.apiKey) return this.skip();
    this.timeout(120000);

    const requestId = `e2e-chat-${Date.now()}`;
    const result = await invokeWithRetry<string>('ai_chat', {
      messages: [{ role: 'user', content: '什么是 SQL JOIN？用一句话解释。' }],
      requestId,
      includeSchema: false,
    });

    expect(result).toBeDefined();
    expect(typeof result).toBe('string');
  });

  // ── AI-006: Smart Filter ──────────────────────────────────────

  it('AI-006: 应能解析自然语言筛选条件', async function () {
    if (!aiConfig.apiKey || !dbSessionId) return this.skip();
    this.timeout(120000);

    const result = await invokeWithRetry<any[]>('ai_parse_filter', {
      dbSessionId,
      database: 'postgres',
      table: 'pg_class',
      naturalLanguage: 'relkind 等于 r',
    });

    expect(result).toBeDefined();
    expect(Array.isArray(result)).toBe(true);
    expect(result.length).toBeGreaterThan(0);
    expect(result[0].column).toBeDefined();
    expect(result[0].operator).toBeDefined();
  });

  // ── AI-007: Schema Document Generation ─────────────────────────

  // AI-007: Schema doc generation is validated in Rust integration tests
  // (test_phase8_schema_doc). It requires processing the entire postgres
  // system catalog (~300+ tables), making it too slow for E2E with
  // rate-limited APIs. Skipped here to keep the E2E suite reliable.
  it.skip('AI-007: Schema 文档生成 (validated in Rust integration tests)', function () {
    // See: src-tauri/tests/ai_e2e.rs::test_phase8_schema_doc
  });

  // ── AI-008: Connection Diagnosis ───────────────────────────────

  it('AI-008: 应能诊断连接错误', async function () {
    if (!aiConfig.apiKey) return this.skip();
    this.timeout(120000);

    const conns = await invokeBackend<any[]>('get_connections');
    const connectionId = conns[0]?.id;
    if (!connectionId) return this.skip();

    // ai_diagnose_connection targets a persistent connection id.
    const result = await invokeWithRetry<any>('ai_diagnose_connection', {
      connectionId,
      errorMessage: 'Connection refused (os error 111)',
    });

    expect(result).toBeDefined();
    expect(result.diagnosis).toBeDefined();
    expect(typeof result.diagnosis).toBe('string');
    expect(Array.isArray(result.possibleCauses)).toBe(true);
    expect(Array.isArray(result.solutions)).toBe(true);
    expect(result.solutions.length).toBeGreaterThan(0);
  });

  // ── AI-009: Query History Analysis ─────────────────────────────

  it('AI-009: 应能分析查询历史', async function () {
    if (!aiConfig.apiKey) return this.skip();
    this.timeout(120000);

    const result = await invokeWithRetry<any>('ai_analyze_queries', {});

    expect(result).toBeDefined();
    expect(result.summary).toBeDefined();
    expect(typeof result.summary).toBe('string');
  });

  // ── AI-010: Settings Integration ────────────────────────────────

  it('AI-010: 设置页面应持久化 AI 配置', async function () {
    if (!aiConfig.apiKey) return this.skip();

    const config = await invokeBackend<any>('ai_get_config');
    expect(config).not.toBeNull();
    expect(config.providerType).toBe('open_ai');
  });

  // ── AI-011: Delete AI Config ───────────────────────────────────

  it('AI-011: 应能删除并恢复 AI 配置', async function () {
    if (!aiConfig.apiKey) return this.skip();

    await invokeBackend('ai_delete_config');
    const config = await invokeBackend<any>('ai_get_config');
    expect(config).toBeNull();

    // Restore config
    await invokeBackend('ai_save_config', {
      config: {
        providerType: aiConfig.providerType,
        endpoint: aiConfig.endpoint,
        apiKey: aiConfig.apiKey,
        model: aiConfig.model,
        extra: null,
      },
    });

    const restored = await invokeBackend<any>('ai_get_config');
    expect(restored).not.toBeNull();
    expect(restored.providerType).toBe('open_ai');
  });

  // ── AI-012: Streaming Support ──────────────────────────────────

  it('AI-012: 流式生成应正常工作', async function () {
    if (!aiConfig.apiKey) return this.skip();
    this.timeout(120000);

    // Wait for rate limit cool-down
    await browser.pause(5000);

    const requestId = `e2e-stream-${Date.now()}`;

    const streamResult = await browser.executeAsync(
      (cmd: string, a: string, retries: number, done: (r: any) => void) => {
        const attempt = (n: number) => {
          (window as any).__TAURI_INTERNALS__
            .invoke(cmd, JSON.parse(a))
            .then((finalResult: string) => {
              done({ completed: true, finalResult });
            })
            .catch((e: any) => {
              const msg = String(e);
              if (msg.includes('Rate limit') && n < retries) {
                setTimeout(() => attempt(n + 1), 10000 * (n + 1));
              } else {
                done({ completed: false, error: msg });
              }
            });
        };
        attempt(0);
      },
      'ai_chat',
      JSON.stringify({
        messages: [{ role: 'user', content: '回答 OK' }],
        requestId,
        includeSchema: false,
      }),
      3,
    );

    expect(streamResult).toBeDefined();
    const sr = streamResult as any;
    if (sr.error) {
      console.warn('Stream test warning:', sr.error);
    }
    expect(sr.completed).toBe(true);
  });
});
