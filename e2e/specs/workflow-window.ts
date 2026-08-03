import { expect, browser, $, $$ } from '@wdio/globals';
import { closeExtraWindows, switchToNewWindow } from '../helpers.js';

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

const TEST_WORKFLOW_ID = 'e2e-wf-window-test';

async function seedTestWorkflow() {
  const conns = await invokeBackend<{ id: string }[]>('get_connections');
  const connId = conns.length > 0 ? conns[0].id : undefined;

  const workflow = {
    id: TEST_WORKFLOW_ID,
    name: 'E2E Window 测试',
    description: '用于 Workflow 窗口 E2E 测试',
    variables: [],
    steps: [
      { type: 'query', id: 'step1', sql: 'SELECT 1 AS id, \'hello\' AS msg', connection: connId },
      { type: 'query', id: 'step2', sql: 'SELECT 2 AS id, \'world\' AS msg', connection: connId },
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

async function clickExecuteButton() {
  return browser.execute(() => {
    const buttons = document.querySelectorAll('button');
    for (const btn of buttons) {
      if (btn.hasAttribute('disabled')) continue;
      const text = (btn.textContent || '').trim();
      // Match "执行" exactly but NOT "执行记录" or "执行中"
      if (text === '执行' || text === 'Execute' || text.match(/^执行$/)) {
        btn.click();
        return true;
      }
      // Also match button with Play icon + "执行" text (the execute button has an icon)
      if (text.endsWith('执行') && !text.includes('记录') && !text.includes('中')) {
        btn.click();
        return true;
      }
    }
    return false;
  });
}

async function selectAndExecuteWorkflow() {
  await browser.waitUntil(async () => {
    const text = await $('body').getText();
    return text.includes('E2E Window 测试');
  }, { timeout: 5000, timeoutMsg: '等待 workflow 列表加载' });

  const wfItem = await $('div*=E2E Window 测试');
  await wfItem.click();
  await browser.pause(500);

  const clicked = await clickExecuteButton();
  expect(clicked).toBe(true);

  // Wait for execution to complete: result tab appears (with step or status markers)
  await browser.waitUntil(async () => {
    const text = await $('body').getText();
    const hasResult = text.includes('step1') || text.includes('✗ms') || (text.includes('✓') && text.includes('ms'));
    const isStillExecuting = text.includes('执行中');
    return hasResult && !isStillExecuting;
  }, { timeout: 30000, timeoutMsg: '等待 workflow 执行完成超时' });

  await browser.pause(500);
}

describe('Workflow 独立窗口 (WORKFLOW-WINDOW)', () => {
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

  it('主窗口应包含工作流入口按钮', async () => {
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

  it('点击工作流按钮应打开新窗口', async () => {
    const wfWindow = await openWorkflowFromMain(mainWindow);
    expect(wfWindow).toBeTruthy();
  });

  it('Workflow 窗口应显示 Workflows 和执行记录标签', async () => {
    await openWorkflowFromMain(mainWindow);
    const bodyText = await $('body').getText();
    expect(bodyText.includes('Workflows')).toBe(true);
    const hasHistory = bodyText.includes('执行记录') || bodyText.includes('History');
    expect(hasHistory).toBe(true);
  });

  it('应显示测试 workflow 在列表中', async () => {
    await openWorkflowFromMain(mainWindow);
    await browser.pause(1000);
    const wfItem = await $('div*=E2E Window 测试');
    await wfItem.waitForDisplayed({ timeout: 5000 });
  });

  it('选择 workflow 后应显示执行按钮', async () => {
    await openWorkflowFromMain(mainWindow);
    await browser.pause(500);
    const wfItem = await $('div*=E2E Window 测试');
    await wfItem.click();
    await browser.pause(300);
    const hasExecBtn = await browser.execute(() => {
      const buttons = document.querySelectorAll('button');
      for (const btn of buttons) {
        const text = btn.textContent || '';
        if (text.includes('执行') || text.includes('Execute')) return true;
      }
      return false;
    });
    expect(hasExecBtn).toBe(true);
  });

  it('执行后应在右侧打开结果 tab', async function () {
    this.timeout(45000);
    await openWorkflowFromMain(mainWindow);
    await selectAndExecuteWorkflow();

    // Should have a result tab with the workflow name
    const tabLabel = await $('span*=E2E Window 测试');
    await expect(tabLabel).toBeDisplayed();
  });

  it('结果 tab 应显示步骤子导航', async function () {
    this.timeout(45000);
    await openWorkflowFromMain(mainWindow);
    await selectAndExecuteWorkflow();

    const step1 = await $('button*=step1');
    await expect(step1).toBeDisplayed();
    const step2 = await $('button*=step2');
    await expect(step2).toBeDisplayed();
  });

  it('关闭结果 tab 应回到空状态', async function () {
    this.timeout(45000);
    await openWorkflowFromMain(mainWindow);
    await selectAndExecuteWorkflow();

    // Close the result tab via the X icon
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

    const bodyText = await $('body').getText();
    const hasEmptyState = bodyText.includes('选择一个工作流并执行') || bodyText.includes('emptyHint') || bodyText.includes('Wand');
    expect(hasEmptyState).toBe(true);
  });

  it('切换到执行记录标签应可查看历史', async function () {
    this.timeout(45000);
    await openWorkflowFromMain(mainWindow);
    await selectAndExecuteWorkflow();
    await browser.pause(500);

    // Switch to history tab in the sidebar
    await findAndClickButton(['执行记录', 'History']);
    await browser.pause(1000);

    const bodyText = await $('body').getText();
    expect(bodyText.includes('E2E Window 测试')).toBe(true);
  });
});
