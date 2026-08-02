import { expect, browser } from '@wdio/globals';
import { openConnectionWindow, closeExtraWindows } from '../helpers.js';

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

interface SkillListItem {
  id: string;
  name: string;
  description: string;
  variables: { name: string; type: string; description: string; required: boolean }[];
}

interface SkillExecutionResult {
  success: boolean;
  finalOutput: string;
  steps: {
    stepId: string;
    stepType: string;
    status: string;
    result: any;
    executionTimeMs: number;
    error: string | null;
    connectionName: string | null;
    sqlExecuted: string | null;
  }[];
  totalTimeMs: number;
  error: string | null;
}

interface HistoryListItem {
  id: string;
  skillId: string;
  skillName: string;
  success: boolean;
  executedAt: string;
  totalTimeMs: number;
}

describe('Skill 跨库工作流 E2E 测试', () => {
  let mainWindow: string;
  let connWindow: string;
  let runtimeConnId: string;

  before(async () => {
    await browser.setTimeout({ script: 120000 });

    const windows = await openConnectionWindow();
    mainWindow = windows.mainWindow;
    connWindow = windows.connWindow;

    runtimeConnId = (await browser.execute(() => {
      const params = new URLSearchParams(window.location.search);
      return params.get('connectionId') || '';
    })) as string;
  });

  after(async () => {
    // Clean up test skills
    try {
      await invokeBackend('skill_delete', { skillId: 'e2e-simple-query' });
    } catch { /* ignore */ }
    try {
      await invokeBackend('skill_delete', { skillId: 'e2e-condition-test' });
    } catch { /* ignore */ }
    try {
      await invokeBackend('skill_delete', { skillId: 'e2e-foreach-test' });
    } catch { /* ignore */ }
    try {
      await invokeBackend('skill_delete', { skillId: 'e2e-error-test' });
    } catch { /* ignore */ }
    try {
      await invokeBackend('skill_history_clear', { skillId: null });
    } catch { /* ignore */ }

    if (mainWindow) {
      await closeExtraWindows(mainWindow);
    }
  });

  // ── SW-01: Skill CRUD via IPC ──────────────────────────────────

  it('SW-01: 应能通过 IPC 创建、读取、删除 Skill', async () => {
    const skill = {
      id: 'e2e-simple-query',
      name: 'E2E 简单查询',
      description: '测试 Skill CRUD',
      variables: [
        { name: 'table_name', type: 'string', description: '表名', required: true },
      ],
      steps: [
        { type: 'query', id: 'run', sql: 'SELECT 1 AS test_col' },
      ],
    };

    await invokeBackend('skill_save', { skill });

    const list = await invokeBackend<SkillListItem[]>('skill_list');
    const found = list.find((s) => s.id === 'e2e-simple-query');
    expect(found).toBeDefined();
    expect(found!.name).toBe('E2E 简单查询');
    expect(found!.variables.length).toBe(1);

    const detail = await invokeBackend<any>('skill_get', { skillId: 'e2e-simple-query' });
    expect(detail.id).toBe('e2e-simple-query');
    expect(detail.steps.length).toBe(1);

    await invokeBackend('skill_delete', { skillId: 'e2e-simple-query' });

    const listAfter = await invokeBackend<SkillListItem[]>('skill_list');
    const foundAfter = listAfter.find((s) => s.id === 'e2e-simple-query');
    expect(foundAfter).toBeUndefined();
  });

  // ── SW-02: Skill Execution with Structured Result ─────────────

  it('SW-02: 应能执行 Skill 并返回结构化结果', async function () {
    if (!runtimeConnId) return this.skip();
    this.timeout(30000);

    const skill = {
      id: 'e2e-simple-query',
      name: 'E2E 简单查询',
      description: '测试执行',
      variables: [],
      steps: [
        { type: 'query', id: 'step1', sql: 'SELECT 1 AS val, \'hello\' AS msg' },
      ],
    };
    await invokeBackend('skill_save', { skill });

    const result = await invokeBackend<SkillExecutionResult>('skill_execute', {
      skillId: 'e2e-simple-query',
      variables: {},
      connectionId: runtimeConnId,
    });

    expect(result.success).toBe(true);
    expect(result.steps.length).toBe(1);
    expect(result.steps[0].stepId).toBe('step1');
    expect(result.steps[0].stepType).toBe('query');
    expect(result.steps[0].status).toBe('success');
    expect(result.steps[0].executionTimeMs).toBeGreaterThanOrEqual(0);
    expect(result.totalTimeMs).toBeGreaterThanOrEqual(0);
    expect(result.error).toBeNull();

    const stepResult = result.steps[0].result;
    expect(stepResult).toBeDefined();
    if (stepResult && stepResult.rows) {
      expect(stepResult.rows.length).toBeGreaterThan(0);
    }
  });

  // ── SW-03: Execution History Persistence ───────────────────────

  it('SW-03: 执行后应自动记录历史', async function () {
    if (!runtimeConnId) return this.skip();

    const history = await invokeBackend<HistoryListItem[]>('skill_history_list', {
      skillId: null,
    });
    const e2eEntries = history.filter((h) => h.skillId === 'e2e-simple-query');
    expect(e2eEntries.length).toBeGreaterThan(0);
    expect(e2eEntries[0].success).toBe(true);
    expect(e2eEntries[0].totalTimeMs).toBeGreaterThanOrEqual(0);

    const detail = await invokeBackend<any>('skill_history_get', {
      historyId: e2eEntries[0].id,
    });
    expect(detail.skillId).toBe('e2e-simple-query');
    expect(detail.result.steps.length).toBe(1);

    const cleared = await invokeBackend<number>('skill_history_clear', { skillId: null });
    expect(cleared).toBeGreaterThan(0);

    const afterClear = await invokeBackend<HistoryListItem[]>('skill_history_list', {
      skillId: null,
    });
    expect(afterClear.length).toBe(0);
  });

  // ── SW-04: Variable Substitution in SQL ────────────────────────

  it('SW-04: Skill 变量应正确替换到 SQL 模板中', async function () {
    if (!runtimeConnId) return this.skip();
    this.timeout(30000);

    const skill = {
      id: 'e2e-simple-query',
      name: 'E2E 变量替换',
      description: '测试变量替换',
      variables: [
        { name: 'val', type: 'string', description: '值', required: true },
      ],
      steps: [
        { type: 'query', id: 'step1', sql: "SELECT '{{val}}' AS result" },
      ],
    };
    await invokeBackend('skill_save', { skill });

    const result = await invokeBackend<SkillExecutionResult>('skill_execute', {
      skillId: 'e2e-simple-query',
      variables: { val: 'E2E_TEST_VALUE' },
      connectionId: runtimeConnId,
    });

    expect(result.success).toBe(true);
    const sql = result.steps[0].sqlExecuted || '';
    expect(sql).toContain('E2E_TEST_VALUE');
  });

  // ── SW-05: Condition Step Execution ─────────────────────────────

  it('SW-05: 条件步骤应根据表达式执行分支', async function () {
    if (!runtimeConnId) return this.skip();
    this.timeout(30000);

    const skill = {
      id: 'e2e-condition-test',
      name: 'E2E 条件测试',
      description: '测试条件分支',
      variables: [],
      steps: [
        { type: 'query', id: 'count_check', sql: 'SELECT 5 AS cnt' },
        {
          type: 'condition',
          id: 'branch',
          if: 'steps.count_check.rows_count > 0',
          then_steps: [
            { type: 'query', id: 'then_query', sql: "SELECT 'condition_true' AS branch" },
          ],
          else_steps: [
            { type: 'query', id: 'else_query', sql: "SELECT 'condition_false' AS branch" },
          ],
        },
      ],
    };
    await invokeBackend('skill_save', { skill });

    const result = await invokeBackend<SkillExecutionResult>('skill_execute', {
      skillId: 'e2e-condition-test',
      variables: {},
      connectionId: runtimeConnId,
    });

    expect(result.success).toBe(true);
    expect(result.steps.length).toBeGreaterThanOrEqual(2);

    const thenStep = result.steps.find((s) => s.stepId === 'then_query');
    expect(thenStep).toBeDefined();
    expect(thenStep!.status).toBe('success');
  });

  // ── SW-06: Error Handling and Display ──────────────────────────

  it('SW-06: 无效 SQL 应返回错误但不崩溃', async function () {
    if (!runtimeConnId) return this.skip();
    this.timeout(30000);

    const skill = {
      id: 'e2e-error-test',
      name: 'E2E 错误测试',
      description: '测试错误处理',
      variables: [],
      steps: [
        { type: 'query', id: 'bad_query', sql: 'SELECT * FROM nonexistent_e2e_table_xyz' },
      ],
    };
    await invokeBackend('skill_save', { skill });

    try {
      const result = await invokeBackend<SkillExecutionResult>('skill_execute', {
        skillId: 'e2e-error-test',
        variables: {},
        connectionId: runtimeConnId,
      });

      // Depending on error strategy, might succeed with error in steps
      if (!result.success) {
        expect(result.error).toBeDefined();
      }
      const badStep = result.steps.find((s) => s.stepId === 'bad_query');
      if (badStep) {
        expect(badStep.status).not.toBe('success');
        expect(badStep.error).toBeDefined();
      }
    } catch (e: any) {
      // Also acceptable: the command returns an error
      expect(e.message).toBeDefined();
      expect(e.message.length).toBeGreaterThan(0);
    }
  });
});
