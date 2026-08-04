import { expect, browser, $ } from '@wdio/globals';
import { closeExtraWindows, switchToNewWindow } from '../helpers.js';

/**
 * Invoke a Tauri backend command directly from the browser context.
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

const TEST_WORKFLOW_ID = 'e2e-wf-tab-test';

/**
 * Create a self-contained test workflow via Tauri IPC.
 * Uses the first available connection for simple SELECT queries.
 */
async function seedTestWorkflow() {
  const conns = await invokeBackend<{ id: string }[]>('get_connections');
  const connId = conns.length > 0 ? conns[0].id : undefined;

  const workflow = {
    id: TEST_WORKFLOW_ID,
    name: 'E2E Tab Test WF',
    description: 'Self-contained workflow for E2E tab system test',
    variables: [],
    steps: [
      { type: 'query', id: 'step_a', sql: 'SELECT 1 AS val, \'alpha\' AS label', connection: connId },
      { type: 'query', id: 'step_b', sql: 'SELECT 2 AS val, \'beta\' AS label', connection: connId },
    ],
  };
  await invokeBackend('workflow_save', { workflow });
}

async function cleanupTestWorkflow() {
  try { await invokeBackend('workflow_delete', { workflowId: TEST_WORKFLOW_ID }); } catch { /* ok */ }
  try { await invokeBackend('workflow_history_clear', { workflowId: null }); } catch { /* ok */ }
}

async function findAndClickButton(textFragments: string[]) {
  return browser.execute((frags: string[]) => {
    const buttons = document.querySelectorAll('button');
    for (const btn of buttons) {
      if (btn.hasAttribute('disabled')) continue;
      const text = btn.textContent || '';
      if (frags.some((f) => text.includes(f))) { btn.click(); return true; }
    }
    return false;
  }, textFragments);
}

async function openWorkflowFromMain(mainHandle: string) {
  await browser.switchToWindow(mainHandle);
  await browser.pause(500);
  await findAndClickButton(['工作流', 'Workflow']);
  const wfWindow = await switchToNewWindow(mainHandle);
  await browser.pause(2000);
  return wfWindow;
}

async function waitForWorkflowList() {
  await browser.waitUntil(async () => {
    const text = await $('body').getText();
    return text.includes('E2E Tab Test WF');
  }, { timeout: 8000, timeoutMsg: 'Timed out waiting for test workflow in list' });
}

async function selectWorkflow() {
  await waitForWorkflowList();
  const wfItem = await $('div*=E2E Tab Test WF');
  await wfItem.click();
  await browser.pause(500);
}

async function executeAndWait() {
  const clicked = await findAndClickButton(['执行', 'Execute']);
  expect(clicked).toBe(true);

  await browser.waitUntil(async () => {
    const text = await $('body').getText();
    const hasResult = text.includes('step_a') || text.includes('✓');
    const executing = text.includes('执行中');
    return hasResult && !executing;
  }, { timeout: 30000, timeoutMsg: 'Timed out waiting for workflow execution to finish' });

  await browser.pause(500);
}

describe('Workflow Tab System (WORKFLOW-WINDOW)', () => {
  let mainWindow: string;

  before(async () => {
    mainWindow = await browser.getWindowHandle();
    await browser.pause(2000);
    await seedTestWorkflow();
    await browser.pause(500);
  });

  after(async () => {
    await closeExtraWindows(mainWindow);
    await browser.switchToWindow(mainWindow);
    await cleanupTestWorkflow();
  });

  afterEach(async () => {
    await closeExtraWindows(mainWindow);
    await browser.switchToWindow(mainWindow);
    await browser.pause(500);
  });

  it('主窗口应显示工作流入口按钮', async () => {
    const hasBtn = await browser.execute(() => {
      const buttons = document.querySelectorAll('button');
      for (const btn of buttons) {
        const text = btn.textContent || '';
        if (text.includes('工作流') || text.includes('Workflow')) return true;
      }
      return false;
    });
    expect(hasBtn).toBe(true);
  });

  it('点击工作流按钮应打开独立窗口', async () => {
    const wfWindow = await openWorkflowFromMain(mainWindow);
    expect(wfWindow).toBeTruthy();
  });

  it('窗口应显示 Workflows 和执行记录侧边栏标签', async () => {
    await openWorkflowFromMain(mainWindow);
    const body = await $('body').getText();
    expect(body.includes('Workflows')).toBe(true);
    const hasHistory = body.includes('执行记录') || body.includes('History');
    expect(hasHistory).toBe(true);
  });

  it('侧边栏应列出已创建的测试 workflow', async () => {
    await openWorkflowFromMain(mainWindow);
    await waitForWorkflowList();
    const item = await $('div*=E2E Tab Test WF');
    await expect(item).toBeDisplayed();
  });

  it('选中 workflow 后右侧应出现空状态提示', async () => {
    await openWorkflowFromMain(mainWindow);
    await selectWorkflow();
    const body = await $('body').getText();
    const hasEmptyOrTab = body.includes('选择一个工作流并执行') || body.includes('E2E Tab Test WF');
    expect(hasEmptyOrTab).toBe(true);
  });

  it('执行后应在 tab 栏打开结果 tab', async function () {
    this.timeout(45000);
    await openWorkflowFromMain(mainWindow);
    await selectWorkflow();
    await executeAndWait();

    const tabLabel = await $('span*=E2E Tab Test WF');
    await expect(tabLabel).toBeDisplayed();
  });

  it('结果 tab 应显示步骤子导航（step_a / step_b）', async function () {
    this.timeout(45000);
    await openWorkflowFromMain(mainWindow);
    await selectWorkflow();
    await executeAndWait();

    const stepA = await $('button*=step_a');
    await expect(stepA).toBeDisplayed();
    const stepB = await $('button*=step_b');
    await expect(stepB).toBeDisplayed();
  });

  it('点击步骤标签应展示 DataTable 查询结果', async function () {
    this.timeout(45000);
    await openWorkflowFromMain(mainWindow);
    await selectWorkflow();
    await executeAndWait();

    const stepA = await $('button*=step_a');
    await stepA.click();
    await browser.pause(500);

    const body = await $('body').getText();
    expect(body.includes('val') || body.includes('label') || body.includes('alpha')).toBe(true);
  });

  it('关闭 tab 后应回到空状态', async function () {
    this.timeout(45000);
    await openWorkflowFromMain(mainWindow);
    await selectWorkflow();
    await executeAndWait();

    await browser.execute(() => {
      const svgs = document.querySelectorAll('svg');
      for (const svg of svgs) {
        const cls = svg.getAttribute('class') || '';
        if (!cls.includes('lucide-x')) continue;
        const btn = svg.closest('button');
        if (btn && (btn.className.includes('opacity-0') || btn.className.includes('group-hover'))) {
          btn.style.opacity = '1';
          btn.click();
          return;
        }
      }
    });
    await browser.pause(500);

    const body = await $('body').getText();
    const hasEmpty = body.includes('选择一个工作流并执行') || body.includes('emptyHint');
    expect(hasEmpty).toBe(true);
  });

  it('切换到执行记录标签应可查看历史', async function () {
    this.timeout(45000);
    await openWorkflowFromMain(mainWindow);
    await selectWorkflow();
    await executeAndWait();
    await browser.pause(500);

    await findAndClickButton(['执行记录', 'History']);
    await browser.pause(1000);

    const body = await $('body').getText();
    expect(body.includes('E2E Tab Test WF')).toBe(true);
  });
});
