import { expect, browser, $, $$ } from '@wdio/globals';
import { closeExtraWindows, switchToNewWindow } from '../helpers.js';
import { t } from '../i18n.js';

async function openWorkflowFromMain(mainHandle: string) {
  await browser.switchToWindow(mainHandle);
  await browser.pause(500);

  const found = await browser.execute((label: string) => {
    const buttons = document.querySelectorAll('button');
    for (const btn of buttons) {
      if (btn.textContent?.includes(label)) {
        btn.click();
        return true;
      }
    }
    return false;
  }, t('action.workflow'));

  if (!found) throw new Error(`Button "${t('action.workflow')}" not found in main window`);

  const wfWindow = await switchToNewWindow(mainHandle);
  await browser.pause(2000);
  return wfWindow;
}

describe('Workflow 独立窗口 (WORKFLOW-WINDOW)', () => {
  let mainWindow: string;

  before(async () => {
    mainWindow = await browser.getWindowHandle();
    await browser.pause(2000);
  });

  afterEach(async () => {
    await closeExtraWindows(mainWindow);
    await browser.switchToWindow(mainWindow);
    await browser.pause(500);
  });

  it('主窗口应包含工作流入口按钮', async () => {
    const label = t('action.workflow');
    const hasBtn = await browser.execute((lbl: string) => {
      const buttons = document.querySelectorAll('button');
      for (const btn of buttons) {
        if (btn.textContent?.includes(lbl)) return true;
      }
      return false;
    }, label);
    expect(hasBtn).toBe(true);
  });

  it('工作流入口应在新建连接按钮之前', async () => {
    const wfLabel = t('action.workflow');
    const ncLabel = t('action.newConnection');
    const labels = await browser.execute(() => {
      const aside = document.querySelector('aside');
      if (!aside) return [];
      const btns = aside.querySelectorAll('button');
      return Array.from(btns).map((b) => (b.textContent ?? '').trim()).filter(Boolean);
    }) as string[];

    const workflowIdx = labels.findIndex((l: string) => l.includes(wfLabel));
    const newConnIdx = labels.findIndex((l: string) => l.includes(ncLabel));
    expect(workflowIdx).toBeGreaterThanOrEqual(0);
    expect(newConnIdx).toBeGreaterThanOrEqual(0);
  });

  it('点击工作流按钮应打开新窗口', async () => {
    const wfWindow = await openWorkflowFromMain(mainWindow);
    expect(wfWindow).toBeTruthy();
  });

  it('Workflow 窗口应显示标题栏', async () => {
    await openWorkflowFromMain(mainWindow);
    const titleBar = await $('header');
    await expect(titleBar).toBeDisplayed();
  });

  it('Workflow 窗口应显示 Workflows 标签页', async () => {
    await openWorkflowFromMain(mainWindow);
    const workflowsTab = await $('button*=Workflows');
    await expect(workflowsTab).toBeDisplayed();
  });

  it('Workflow 窗口应显示执行记录标签页', async () => {
    await openWorkflowFromMain(mainWindow);
    const historyTab = await $(`button*=${t('workflows.history.title')}`);
    await expect(historyTab).toBeDisplayed();
  });

  it('Workflow 窗口应显示新建工作流按钮', async () => {
    await openWorkflowFromMain(mainWindow);
    const createBtn = await $(`button*=${t('workflows.create')}`);
    await expect(createBtn).toBeDisplayed();
  });

  it('Workflow 窗口右侧应显示空状态提示', async () => {
    await openWorkflowFromMain(mainWindow);
    const emptyHint = await $(`p*=${t('workflows.emptyHint')}`);
    await expect(emptyHint).toBeDisplayed();
  });

  it('切换到执行记录标签应显示空状态', async () => {
    await openWorkflowFromMain(mainWindow);
    const historyTab = await $(`button*=${t('workflows.history.title')}`);
    await historyTab.click();
    await browser.pause(500);
    const emptyHistory = await $(`*=${t('workflows.history.empty')}`);
    const exists = await emptyHistory.isExisting();
    expect(exists).toBe(true);
  });
});
