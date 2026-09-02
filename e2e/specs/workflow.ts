import { expect, browser, $ } from '@wdio/globals';
import {
  openConnectionWindow,
  closeExtraWindows,
  captureJourneyStep,
  switchWorkspaceNav,
} from '../helpers.js';

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

interface WorkflowListItem {
  id: string;
  name: string;
  description: string;
  variables: { name: string; type: string; description: string; required: boolean }[];
}

interface WorkflowExecutionResult {
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
  workflowId: string;
  workflowName: string;
  success: boolean;
  executedAt: string;
  totalTimeMs: number;
}

describe('Workflow 跨库工作流 E2E 测试', () => {
  let mainWindow: string;
  let connWindow: string;
  let dbSessionId: string;

  before(async () => {
    await browser.setTimeout({ script: 120000 });

    const windows = await openConnectionWindow();
    mainWindow = windows.mainWindow;
    connWindow = windows.connWindow;

    dbSessionId = (await browser.execute(() => {
      const params = new URLSearchParams(window.location.search);
      return params.get('connectionId') || '';
    })) as string;
  });

  after(async () => {
    // Clean up test workflows
    try {
      await invokeBackend('workflow_delete', { workflowId: 'e2e-simple-query' });
    } catch {
      /* ignore */
    }
    try {
      await invokeBackend('workflow_delete', { workflowId: 'e2e-condition-test' });
    } catch {
      /* ignore */
    }
    try {
      await invokeBackend('workflow_delete', { workflowId: 'e2e-foreach-test' });
    } catch {
      /* ignore */
    }
    try {
      await invokeBackend('workflow_delete', { workflowId: 'e2e-error-test' });
    } catch {
      /* ignore */
    }
    try {
      await invokeBackend('workflow_history_clear', { workflowId: null });
    } catch {
      /* ignore */
    }

    if (mainWindow) {
      await closeExtraWindows(mainWindow);
    }
  });

  // ── SW-01: Workflow CRUD via IPC ──────────────────────────────────

  it('SW-01: 应能通过 IPC 创建、读取、删除 Workflow', async () => {
    const workflow = {
      id: 'e2e-simple-query',
      name: 'E2E 简单查询',
      description: '测试 Workflow CRUD',
      variables: [{ name: 'table_name', type: 'string', description: '表名', required: true }],
      steps: [{ type: 'query', id: 'run', sql: 'SELECT 1 AS test_col' }],
    };

    await invokeBackend('workflow_save', { workflow });

    const list = await invokeBackend<WorkflowListItem[]>('workflow_list');
    const found = list.find((s) => s.id === 'e2e-simple-query');
    expect(found).toBeDefined();
    expect(found!.name).toBe('E2E 简单查询');
    expect(found!.variables.length).toBe(1);

    const detail = await invokeBackend<any>('workflow_get', { workflowId: 'e2e-simple-query' });
    expect(detail.id).toBe('e2e-simple-query');
    expect(detail.steps.length).toBe(1);

    await invokeBackend('workflow_delete', { workflowId: 'e2e-simple-query' });

    const listAfter = await invokeBackend<WorkflowListItem[]>('workflow_list');
    const foundAfter = listAfter.find((s) => s.id === 'e2e-simple-query');
    expect(foundAfter).toBeUndefined();
  });

  // ── SW-02: Workflow Execution with Structured Result ─────────────

  it('SW-02: 应能执行 Workflow 并返回结构化结果', async function () {
    if (!dbSessionId) return this.skip();
    this.timeout(30000);

    const workflow = {
      id: 'e2e-simple-query',
      name: 'E2E 简单查询',
      description: '测试执行',
      variables: [],
      steps: [{ type: 'query', id: 'step1', sql: "SELECT 1 AS val, 'hello' AS msg" }],
    };
    await invokeBackend('workflow_save', { workflow });

    const result = await invokeBackend<WorkflowExecutionResult>('workflow_execute', {
      workflowId: 'e2e-simple-query',
      variables: {},
      // workflow_execute targets a persistent connection id; the backend's
      // dual-mode resolve_session also accepts the live session id passed here.
      connectionId: dbSessionId,
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
    if (!dbSessionId) return this.skip();

    const history = await invokeBackend<HistoryListItem[]>('workflow_history_list', {
      workflowId: null,
    });
    const e2eEntries = history.filter((h) => h.workflowId === 'e2e-simple-query');
    expect(e2eEntries.length).toBeGreaterThan(0);
    expect(e2eEntries[0].success).toBe(true);
    expect(e2eEntries[0].totalTimeMs).toBeGreaterThanOrEqual(0);

    const detail = await invokeBackend<any>('workflow_history_get', {
      historyId: e2eEntries[0].id,
    });
    expect(detail.workflowId).toBe('e2e-simple-query');
    expect(detail.result.steps.length).toBe(1);

    const cleared = await invokeBackend<number>('workflow_history_clear', { workflowId: null });
    expect(cleared).toBeGreaterThan(0);

    const afterClear = await invokeBackend<HistoryListItem[]>('workflow_history_list', {
      workflowId: null,
    });
    expect(afterClear.length).toBe(0);
  });

  // ── SW-04: Variable Substitution in SQL ────────────────────────

  it('SW-04: Workflow 变量应正确替换到 SQL 模板中', async function () {
    if (!dbSessionId) return this.skip();
    this.timeout(30000);

    const workflow = {
      id: 'e2e-simple-query',
      name: 'E2E 变量替换',
      description: '测试变量替换',
      variables: [{ name: 'val', type: 'string', description: '值', required: true }],
      steps: [{ type: 'query', id: 'step1', sql: "SELECT '{{val}}' AS result" }],
    };
    await invokeBackend('workflow_save', { workflow });

    const result = await invokeBackend<WorkflowExecutionResult>('workflow_execute', {
      workflowId: 'e2e-simple-query',
      variables: { val: 'E2E_TEST_VALUE' },
      connectionId: dbSessionId,
    });

    expect(result.success).toBe(true);
    const sql = result.steps[0].sqlExecuted || '';
    expect(sql).toContain('E2E_TEST_VALUE');
  });

  // ── SW-04b: Multi-step template with data[N].field ─────────────

  it('SW-04b: 多步骤模板引用应支持 data[0].field 语法', async function () {
    if (!dbSessionId) return this.skip();
    this.timeout(30000);

    const workflow = {
      id: 'e2e-simple-query',
      name: 'E2E 模板引用',
      description: '测试 rows[0].field 跨步骤引用',
      variables: [],
      steps: [
        { type: 'query', id: 'step1', sql: "SELECT 42 AS magic_number, 'hello' AS greeting" },
        {
          type: 'query',
          id: 'step2',
          sql: "SELECT '{{steps.step1.rows[0].greeting}}' AS msg, {{steps.step1.rows[0].magic_number}} AS num",
        },
      ],
    };
    await invokeBackend('workflow_save', { workflow });

    const result = await invokeBackend<WorkflowExecutionResult>('workflow_execute', {
      workflowId: 'e2e-simple-query',
      variables: {},
      // workflow_execute targets a persistent connection id; the backend's
      // dual-mode resolve_session also accepts the live session id passed here.
      connectionId: dbSessionId,
    });

    expect(result.success).toBe(true);
    expect(result.steps.length).toBe(2);

    const step2 = result.steps[1];
    expect(step2.status).toBe('success');
    expect(step2.sqlExecuted).toContain('hello');
    expect(step2.sqlExecuted).toContain('42');

    const step1Result = result.steps[0].result;
    expect(step1Result.rows).toBeDefined();
    expect(step1Result.rows.length).toBeGreaterThan(0);
    expect(step1Result.rows[0]).toHaveProperty('magic_number');
    expect(step1Result.rows[0]).toHaveProperty('greeting');
  });

  // ── SW-04c: Template with result[N].field (alias for data) ────

  it('SW-04c: rows[0].field bracket 语法引用', async function () {
    if (!dbSessionId) return this.skip();
    this.timeout(30000);

    const workflow = {
      id: 'e2e-simple-query',
      name: 'E2E result alias',
      description: '测试 rows[0].field bracket 语法',
      variables: [],
      steps: [
        { type: 'query', id: 'src', sql: 'SELECT 99 AS code' },
        { type: 'query', id: 'dst', sql: 'SELECT {{steps.src.rows[0].code}} AS via_rows' },
      ],
    };
    await invokeBackend('workflow_save', { workflow });

    const result = await invokeBackend<WorkflowExecutionResult>('workflow_execute', {
      workflowId: 'e2e-simple-query',
      variables: {},
      // workflow_execute targets a persistent connection id; the backend's
      // dual-mode resolve_session also accepts the live session id passed here.
      connectionId: dbSessionId,
    });

    expect(result.success).toBe(true);
    const dstStep = result.steps.find((s) => s.stepId === 'dst');
    expect(dstStep).toBeDefined();
    expect(dstStep!.status).toBe('success');
    expect(dstStep!.sqlExecuted).toContain('99');
  });

  // ── SW-04d: Workflow query step with database field ────────────

  it('SW-04d: 工作流 query step 应能保存和读取 database 字段', async function () {
    const workflow = {
      id: 'e2e-simple-query',
      name: 'E2E database field',
      description: '测试 database 字段',
      variables: [],
      steps: [
        { type: 'query', id: 'q1', sql: 'SELECT 1', database: 'mydb' },
        { type: 'query', id: 'q2', sql: 'SELECT 2' },
      ],
    };
    await invokeBackend('workflow_save', { workflow });

    const detail = await invokeBackend<any>('workflow_get', { workflowId: 'e2e-simple-query' });
    expect(detail.steps[0].database).toBe('mydb');
    expect(detail.steps[1].database).toBeNull();
  });

  // ── SW-05: Condition Step Execution ─────────────────────────────

  it('SW-05: 条件步骤应根据表达式执行分支', async function () {
    if (!dbSessionId) return this.skip();
    this.timeout(30000);

    const workflow = {
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
    await invokeBackend('workflow_save', { workflow });

    const result = await invokeBackend<WorkflowExecutionResult>('workflow_execute', {
      workflowId: 'e2e-condition-test',
      variables: {},
      connectionId: dbSessionId,
    });

    expect(result.success).toBe(true);
    expect(result.steps.length).toBeGreaterThanOrEqual(2);

    const thenStep = result.steps.find((s) => s.stepId === 'then_query');
    expect(thenStep).toBeDefined();
    expect(thenStep!.status).toBe('success');
  });

  // ── SW-06: Error Handling and Display ──────────────────────────

  it('SW-06: 无效 SQL 应返回错误但不崩溃', async function () {
    if (!dbSessionId) return this.skip();
    this.timeout(30000);

    const workflow = {
      id: 'e2e-error-test',
      name: 'E2E 错误测试',
      description: '测试错误处理',
      variables: [],
      steps: [{ type: 'query', id: 'bad_query', sql: 'SELECT * FROM nonexistent_e2e_table_xyz' }],
    };
    await invokeBackend('workflow_save', { workflow });

    try {
      const result = await invokeBackend<WorkflowExecutionResult>('workflow_execute', {
        workflowId: 'e2e-error-test',
        variables: {},
        connectionId: dbSessionId,
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

  // ── TC-WF-011: GUI context menu (from workflow-lifecycle.ts) ─────

  it('TC-WF-011: 工作流列表右键不应有"立即运行"菜单项', async () => {
    await browser.switchToWindow(mainWindow);
    await switchWorkspaceNav('workspace-nav-workflow', 'workflow-workspace', 'workflow-tab-open');
    await browser.pause(500);

    const rightClicked = await browser.execute(() => {
      const items = document.querySelectorAll('[data-workflow-item], [class*="workflow"] li');
      if (items.length > 0) {
        (items[0] as HTMLElement).dispatchEvent(
          new MouseEvent('contextmenu', { bubbles: true, cancelable: true }),
        );
        return true;
      }
      return false;
    });
    if (rightClicked) {
      await browser.pause(500);
      await captureJourneyStep('workflow-context-menu');
      const body = await $('body').getText();
      const hasRun = body.includes('立即运行') || body.includes('Run Now');
      expect(hasRun).toBe(false);
    }
  });
});
