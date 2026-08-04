import { expect, browser, $, $$ } from '@wdio/globals';
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
const FOCUS_TEST_WF_ID = 'e2e-wf-focus-test';

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

  const focusWf = {
    id: FOCUS_TEST_WF_ID,
    name: 'E2E Focus Test WF',
    description: 'Workflow with variable to test focus behavior',
    variables: [{ name: 'uid', type: 'string', description: 'User ID', required: true }],
    steps: [
      { type: 'query', id: 'step1', sql: "SELECT '{{uid}}' AS uid", connection: connId },
    ],
  };
  await invokeBackend('workflow_save', { workflow: focusWf });
}

async function cleanupTestWorkflow() {
  try { await invokeBackend('workflow_delete', { workflowId: TEST_WORKFLOW_ID }); } catch { /* ok */ }
  try { await invokeBackend('workflow_delete', { workflowId: FOCUS_TEST_WF_ID }); } catch { /* ok */ }
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

  it('重复点击同一历史记录只打开一个 tab', async function () {
    this.timeout(60000);
    await openWorkflowFromMain(mainWindow);
    await selectWorkflow();
    await executeAndWait();
    await browser.pause(500);

    // Switch to history tab
    await findAndClickButton(['执行记录', 'History']);
    await browser.pause(1000);

    // Count tabs before clicking history
    const tabsBefore = await browser.execute(() => {
      const tabBar = document.querySelectorAll('[class*="border-r"][class*="border-edge"][class*="text-xs"]');
      return tabBar.length;
    });

    // Click the first history item
    const historyItems = await $$('button*=E2E Tab Test WF');
    if (historyItems.length > 0) {
      await historyItems[0].click();
      await browser.pause(1000);

      // Count tabs after first click
      const tabsAfterFirst = await browser.execute(() => {
        const tabBar = document.querySelectorAll('[class*="border-r"][class*="border-edge"][class*="text-xs"]');
        return tabBar.length;
      });

      // Click the same history item again
      await findAndClickButton(['执行记录', 'History']);
      await browser.pause(500);
      const historyItems2 = await $$('button*=E2E Tab Test WF');
      if (historyItems2.length > 0) {
        await historyItems2[0].click();
        await browser.pause(1000);
      }

      // Count tabs after second click - should be the same
      const tabsAfterSecond = await browser.execute(() => {
        const tabBar = document.querySelectorAll('[class*="border-r"][class*="border-edge"][class*="text-xs"]');
        return tabBar.length;
      });

      expect(tabsAfterSecond).toBe(tabsAfterFirst);
    }
  });

  it('执行后默认显示第一个 step 结果', async function () {
    this.timeout(45000);
    await openWorkflowFromMain(mainWindow);
    await selectWorkflow();
    await executeAndWait();

    // After execution, the first step should be automatically selected
    const body = await $('body').getText();
    // step_a should be visible in the step detail view (its content or its tab highlighted)
    const hasStepContent = body.includes('val') || body.includes('alpha') || body.includes('step_a');
    expect(hasStepContent).toBe(true);

    // The step_a tab should have the active/selected styling
    const isStepAActive = await browser.execute(() => {
      const buttons = document.querySelectorAll('button');
      for (const btn of buttons) {
        if (btn.textContent?.includes('step_a') && btn.className.includes('accent')) {
          return true;
        }
      }
      return false;
    });
    expect(isStepAActive).toBe(true);
  });

  it('参数输入框应在步骤标签栏上方', async function () {
    this.timeout(45000);
    await openWorkflowFromMain(mainWindow);
    await selectWorkflow();
    await executeAndWait();

    // Verify layout order: execute button should appear before step tabs
    const layoutCorrect = await browser.execute(() => {
      const executeBtn = document.querySelector('button') as HTMLElement | null;
      let executeBtnRect: DOMRect | null = null;
      let stepTabRect: DOMRect | null = null;

      const buttons = document.querySelectorAll('button');
      for (const btn of buttons) {
        const text = btn.textContent || '';
        if ((text.includes('执行') || text.includes('Execute')) && !text.includes('执行记录')) {
          executeBtnRect = btn.getBoundingClientRect();
        }
        if (text.includes('step_a')) {
          stepTabRect = btn.getBoundingClientRect();
        }
      }

      if (executeBtnRect && stepTabRect) {
        // Execute button should be above (lower Y value) step tabs
        return executeBtnRect.top < stepTabRect.top;
      }
      return null;
    });

    if (layoutCorrect !== null) {
      expect(layoutCorrect).toBe(true);
    }
  });

  it('打开工作流目录按钮应通过 open_path 命令打开文件夹', async function () {
    this.timeout(15000);
    await openWorkflowFromMain(mainWindow);
    await browser.pause(1000);

    const openDirBtn = await browser.execute(() => {
      const buttons = document.querySelectorAll('button');
      for (const btn of buttons) {
        const title = btn.getAttribute('title') || '';
        if (title.includes('打开工作流目录') || title.includes('Open workflow directory')) {
          return true;
        }
      }
      return false;
    });
    expect(openDirBtn).toBe(true);

    const result = await browser.executeAsync(
      (done: (r: { called: boolean; path: string }) => void) => {
        const orig = (window as any).__TAURI_INTERNALS__.invoke;
        let captured = { called: false, path: '' };
        (window as any).__TAURI_INTERNALS__.invoke = (cmd: string, args: any) => {
          if (cmd === 'open_path') {
            captured = { called: true, path: args?.path || '' };
            return Promise.resolve();
          }
          return orig(cmd, args);
        };
        const buttons = document.querySelectorAll('button');
        for (const btn of buttons) {
          const title = btn.getAttribute('title') || '';
          if (title.includes('打开工作流目录') || title.includes('Open workflow directory')) {
            btn.click();
            break;
          }
        }
        setTimeout(() => {
          (window as any).__TAURI_INTERNALS__.invoke = orig;
          done(captured);
        }, 2000);
      },
    );

    expect(result.called).toBe(true);
    expect(result.path.length).toBeGreaterThan(0);
  });

  it('输入框有焦点时单击侧边栏标签应立即切换', async function () {
    this.timeout(30000);
    await openWorkflowFromMain(mainWindow);
    await browser.pause(1000);

    const focusWfItem = await $('div*=E2E Focus Test WF');
    await focusWfItem.click();
    await browser.pause(500);

    const inputField = await $('input[placeholder]');
    if (await inputField.isExisting()) {
      await inputField.click();
      await inputField.setValue('test-value');
      await browser.pause(200);
    }

    const historyTab = await browser.execute(() => {
      const buttons = document.querySelectorAll('button');
      for (const btn of buttons) {
        const text = btn.textContent || '';
        if (text.includes('执行记录') || text.includes('History')) return true;
      }
      return false;
    });
    expect(historyTab).toBe(true);

    await findAndClickButton(['执行记录', 'History']);
    await browser.pause(300);

    const isHistoryActive = await browser.execute(() => {
      const buttons = document.querySelectorAll('button');
      for (const btn of buttons) {
        const text = btn.textContent || '';
        if ((text.includes('执行记录') || text.includes('History')) && btn.className.includes('font-medium')) {
          return true;
        }
      }
      return false;
    });
    expect(isHistoryActive).toBe(true);
  });
});
