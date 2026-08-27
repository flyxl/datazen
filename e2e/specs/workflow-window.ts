import { expect, browser, $, $$ } from '@wdio/globals';
import { closeExtraWindows, captureJourneyStep, switchWorkspaceNav } from '../helpers.js';
import { t } from '../i18n.js';

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
  const conns = await invokeBackend<{ id: string; name?: string }[]>('get_connections');
  const seeded = conns.find((c) => c.id === 'conn_e2e_pg' || c.name === '本地 PostgreSQL');
  const connId = seeded?.id ?? conns[0]?.id;

  const workflow = {
    id: TEST_WORKFLOW_ID,
    name: 'E2E Tab Test WF',
    description: 'Self-contained workflow for E2E tab system test',
    variables: [],
    steps: [
      { type: 'query', id: 'step_a', sql: "SELECT 1 AS val, 'alpha' AS label", connection: connId },
      { type: 'query', id: 'step_b', sql: "SELECT 2 AS val, 'beta' AS label", connection: connId },
    ],
  };
  await invokeBackend('workflow_save', { workflow });

  const focusWf = {
    id: FOCUS_TEST_WF_ID,
    name: 'E2E Focus Test WF',
    description: 'Workflow with variable to test focus behavior',
    variables: [{ name: 'uid', type: 'string', description: 'User ID', required: true }],
    steps: [{ type: 'query', id: 'step1', sql: "SELECT '{{uid}}' AS uid", connection: connId }],
  };
  await invokeBackend('workflow_save', { workflow: focusWf });
}

async function cleanupTestWorkflow() {
  try {
    await invokeBackend('workflow_delete', { workflowId: TEST_WORKFLOW_ID });
  } catch {
    /* ok */
  }
  try {
    await invokeBackend('workflow_delete', { workflowId: FOCUS_TEST_WF_ID });
  } catch {
    /* ok */
  }
  try {
    await invokeBackend('workflow_history_clear', { workflowId: null });
  } catch {
    /* ok */
  }
}

async function findAndClickButton(textFragments: string[]) {
  return browser.execute((frags: string[]) => {
    const buttons = document.querySelectorAll('button');
    for (const btn of buttons) {
      if (btn.hasAttribute('disabled')) continue;
      const text = btn.textContent || '';
      if (frags.some((f) => text.includes(f))) {
        btn.click();
        return true;
      }
    }
    return false;
  }, textFragments);
}

async function openWorkflowWorkspace(mainHandle: string) {
  await browser.switchToWindow(mainHandle);
  await browser.pause(300);
  await switchWorkspaceNav(
    'workspace-nav-workflow',
    'workflow-workspace',
    'workflow-workspace-open',
  );
}

async function switchToWorkflowsSidebarTab() {
  await openWorkflowWorkspace(await browser.getWindowHandle());
  await browser.execute(() => {
    for (const btn of document.querySelectorAll('button')) {
      const text = (btn.textContent ?? '').trim();
      if (text === 'Workflows' || text.startsWith('Workflows')) {
        btn.click();
        return;
      }
    }
  });
  await browser.pause(400);
}

async function waitForWorkflowList() {
  await switchToWorkflowsSidebarTab();
  await browser.waitUntil(
    async () => {
      const text = await $('body').getText();
      return text.includes('E2E Tab Test WF');
    },
    { timeout: 8000, timeoutMsg: 'Timed out waiting for test workflow in list' },
  );
}

async function openWorkflowEditor(workflowName = 'E2E Tab Test WF') {
  await waitForWorkflowList();
  const opened = await browser.execute((name: string) => {
    for (const edit of document.querySelectorAll('[data-testid="workflow-item-edit"]')) {
      const row = edit.closest('.group');
      if (row?.textContent?.includes(name)) {
        (edit as HTMLElement).click();
        return true;
      }
    }
    return false;
  }, workflowName);
  expect(opened).toBe(true);
  await browser.pause(600);
  await $('[data-testid="workflow-editor-mode-tabs"]').waitForDisplayed({ timeout: 10000 });
}

async function selectWorkflow() {
  await waitForWorkflowList();
  const wfItem = await $('div*=E2E Tab Test WF');
  await wfItem.click();
  await browser.pause(500);
  await captureJourneyStep('workflow-selected');
}

async function executeAndWait() {
  const clicked = await browser.execute(() => {
    const buttons = document.querySelectorAll('button');
    for (const btn of buttons) {
      if (btn.hasAttribute('disabled')) continue;
      const text = (btn.textContent || '').trim();
      // "执行记录" also contains 执行 — require an exact action label.
      if (text === '执行' || text === 'Execute') {
        btn.click();
        return true;
      }
    }
    return false;
  });
  expect(clicked).toBe(true);

  await browser.waitUntil(
    async () => {
      const text = await $('body').getText();
      const hasResult = text.includes('step_a') || text.includes('✓');
      const executing = text.includes('执行中');
      return hasResult && !executing;
    },
    { timeout: 30000, timeoutMsg: 'Timed out waiting for workflow execution to finish' },
  );

  await browser.pause(500);
  await captureJourneyStep('workflow-executed');
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

  it('主窗口应显示工作流导航入口', async () => {
    const workflowNav = await $('[data-testid="workspace-nav-workflow"]');
    await expect(workflowNav).toBeDisplayed();
  });

  it('点击工作流导航应切换到嵌入工作区', async () => {
    await openWorkflowWorkspace(mainWindow);
    const workflowWorkspace = await $('[data-testid="workflow-workspace"]');
    await expect(workflowWorkspace).toBeDisplayed();
    await captureJourneyStep('workflow-workspace-open', 0, true);
  });

  it('工作区应显示 Workflows 和执行记录侧边栏标签', async () => {
    await openWorkflowWorkspace(mainWindow);
    const body = await $('body').getText();
    expect(body.includes('Workflows')).toBe(true);
    const hasHistory = body.includes('执行记录') || body.includes('History');
    expect(hasHistory).toBe(true);
  });

  it('侧边栏应列出已创建的测试 workflow', async () => {
    await openWorkflowWorkspace(mainWindow);
    await waitForWorkflowList();
    const item = await $('div*=E2E Tab Test WF');
    await expect(item).toBeDisplayed();
  });

  it('WF-CTX-001: workflow 列表右键使用原生菜单（非 Web 菜单）', async () => {
    await openWorkflowWorkspace(mainWindow);
    await waitForWorkflowList();
    const item = await $('div*=E2E Tab Test WF');
    await item.click({ button: 'right' });
    await browser.pause(400);
    expect(await $('[data-testid="web-context-menu"]').isExisting()).toBe(false);
    await captureJourneyStep('workflow-context-menu');
  });

  it('WF-CTX-002: 执行记录右键不弹出菜单', async () => {
    await openWorkflowWorkspace(mainWindow);
    await waitForWorkflowList();
    const historyTab = await $(`button*=${t('workflows.history.title')}`);
    await historyTab.click();
    await browser.pause(400);
    await browser.execute(() => {
      const rows = Array.from(document.querySelectorAll('button'));
      const row = rows.find((b) => b.querySelector('.text-xs.font-medium'));
      row?.dispatchEvent(
        new MouseEvent('contextmenu', { bubbles: true, clientX: 40, clientY: 80 }),
      );
    });
    await browser.pause(300);
    const menu = await $('[data-testid="web-context-menu"]');
    expect(await menu.isExisting()).toBe(false);
  });

  it('选中 workflow 后右侧应出现空状态提示', async () => {
    await openWorkflowWorkspace(mainWindow);
    await selectWorkflow();
    const body = await $('body').getText();
    const hasEmptyOrTab = body.includes('选择一个工作流并执行') || body.includes('E2E Tab Test WF');
    expect(hasEmptyOrTab).toBe(true);
  });

  it('执行后应在 tab 栏打开结果 tab', async function () {
    this.timeout(45000);
    await openWorkflowWorkspace(mainWindow);
    await selectWorkflow();
    await executeAndWait();

    const tabLabel = await $('span*=E2E Tab Test WF');
    await expect(tabLabel).toBeDisplayed();
  });

  it('结果 tab 应显示步骤子导航（step_a / step_b）', async function () {
    this.timeout(45000);
    await openWorkflowWorkspace(mainWindow);
    await selectWorkflow();
    await executeAndWait();

    const stepA = await $('button*=step_a');
    await expect(stepA).toBeDisplayed();
    const stepB = await $('button*=step_b');
    await expect(stepB).toBeDisplayed();
  });

  it('点击步骤标签应展示 DataTable 查询结果', async function () {
    this.timeout(45000);
    await openWorkflowWorkspace(mainWindow);
    await selectWorkflow();
    await executeAndWait();

    const stepA = await $('button*=step_a');
    await stepA.click();
    await browser.pause(500);
    await captureJourneyStep('workflow-step-result');

    const body = await $('body').getText();
    expect(body.includes('val') || body.includes('label') || body.includes('alpha')).toBe(true);
  });

  it('关闭 tab 后应回到空状态', async function () {
    this.timeout(45000);
    await openWorkflowWorkspace(mainWindow);
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
    await openWorkflowWorkspace(mainWindow);
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
    await openWorkflowWorkspace(mainWindow);
    await selectWorkflow();
    await executeAndWait();
    await browser.pause(500);

    // Switch to history tab
    await findAndClickButton(['执行记录', 'History']);
    await browser.pause(1000);

    // Count tabs before clicking history
    const tabsBefore = await browser.execute(() => {
      const tabBar = document.querySelectorAll(
        '[class*="border-r"][class*="border-edge"][class*="text-xs"]',
      );
      return tabBar.length;
    });

    // Click the first history item
    const historyItems = await $$('button*=E2E Tab Test WF');
    if (historyItems.length > 0) {
      await historyItems[0].click();
      await browser.pause(1000);

      // Count tabs after first click
      const tabsAfterFirst = await browser.execute(() => {
        const tabBar = document.querySelectorAll(
          '[class*="border-r"][class*="border-edge"][class*="text-xs"]',
        );
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
        const tabBar = document.querySelectorAll(
          '[class*="border-r"][class*="border-edge"][class*="text-xs"]',
        );
        return tabBar.length;
      });

      expect(tabsAfterSecond).toBe(tabsAfterFirst);
    }
  });

  it('执行后默认显示第一个 step 结果', async function () {
    this.timeout(45000);
    await openWorkflowWorkspace(mainWindow);
    await selectWorkflow();
    await executeAndWait();

    // After execution, the first step should be automatically selected
    const body = await $('body').getText();
    // step_a should be visible in the step detail view (its content or its tab highlighted)
    const hasStepContent =
      body.includes('val') || body.includes('alpha') || body.includes('step_a');
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
    await openWorkflowWorkspace(mainWindow);
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

  it('打开工作流目录按钮应通过 open_workflows_dir 命令打开文件夹', async function () {
    this.timeout(20000);
    await openWorkflowWorkspace(mainWindow);
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

    // Tauri 2 freezes invoke — assert source wiring + button click + IPC availability.
    const fs = await import('node:fs');
    const path = await import('node:path');
    const root = path.resolve(import.meta.dirname, '../..');
    const wfWin = fs.readFileSync(path.join(root, 'src/windows/workflow/WorkflowPage.tsx'), 'utf8');
    // Prefer dedicated command; tolerate either window or shared settings wrapper.
    const settingsCmd = fs.readFileSync(path.join(root, 'src/commands/settings.ts'), 'utf8');
    expect(settingsCmd).toContain("invoke<void>('open_workflows_dir')");
    expect(wfWin.includes('openWorkflowsDir') || wfWin.includes('open_workflows_dir')).toBe(true);

    await browser.execute(() => {
      const buttons = document.querySelectorAll('button');
      for (const btn of buttons) {
        const title = btn.getAttribute('title') || '';
        if (title.includes('打开工作流目录') || title.includes('Open workflow directory')) {
          btn.click();
          break;
        }
      }
    });
    await browser.pause(800);

    // Dedicated command must exist in webdriver builds.
    await invokeBackend('open_workflows_dir');
  });

  it('输入框有焦点时单击侧边栏标签应立即切换', async function () {
    this.timeout(30000);
    await openWorkflowWorkspace(mainWindow);
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
        if (
          (text.includes('执行记录') || text.includes('History')) &&
          btn.className.includes('font-medium')
        ) {
          return true;
        }
      }
      return false;
    });
    expect(isHistoryActive).toBe(true);
  });

  it('WF-SQL-001: 可视化编辑查询步骤应使用 SQL 高亮编辑器', async () => {
    await openWorkflowEditor();
    const editor = await $('[data-testid="workflow-sql-editor"]');
    await editor.waitForDisplayed({ timeout: 10000 });
    await expect(await $('[data-testid="workflow-sql-editor"] .cm-editor')).toBeDisplayed();
  });

  it('WF-SQL-002: 编辑 workflow 时 SQL 编辑器右键为原生菜单', async () => {
    await openWorkflowEditor();
    const editor = await $('[data-testid="workflow-sql-editor"] .cm-editor');
    await editor.waitForDisplayed({ timeout: 5000 });
    await editor.click({ button: 'right' });
    await browser.pause(400);
    expect(await $('[data-testid="web-context-menu"]').isExisting()).toBe(false);
  });

  it('WF-YAML-001: 应能切换到 YAML 编辑模式', async () => {
    await openWorkflowEditor();
    const yamlTab = await $('[data-testid="workflow-mode-yaml"]');
    await yamlTab.waitForDisplayed({ timeout: 10000 });
    await yamlTab.click();
    await browser.pause(600);
    await expect(await $('[data-testid="workflow-yaml-editor"]')).toBeDisplayed();
  });

  it('WF-YAML-002: YAML 模式应显示保存入口', async () => {
    await openWorkflowEditor();
    const yamlTab = await $('[data-testid="workflow-mode-yaml"]');
    await yamlTab.click();
    await browser.pause(400);
    const saveBtn = await $('[data-testid="workflow-yaml-save"]');
    await expect(saveBtn).toBeDisplayed();
    const visualTab = await $('[data-testid="workflow-mode-visual"]');
    await visualTab.click();
    await browser.pause(400);
  });
});
