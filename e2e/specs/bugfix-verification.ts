import { expect, browser, $ } from '@wdio/globals';
import {
  openConnectionWindow,
  closeExtraWindows,
  executeSQL,
  openQueryTab,
  openWorkflowWorkspace,
} from '../helpers.js';
import { t } from '../i18n.js';

describe('Bug Fix Verification', () => {
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

  describe('FIX-001: Workflow 工作区不应包含主题切换按钮', () => {
    it('Workflow 工作区中不应存在 ThemeToggle 按钮', async () => {
      await openWorkflowWorkspace(mainWindow);
      await browser.pause(500);

      const hasThemeToggle = await browser.execute(() => {
        const buttons = document.querySelectorAll('button');
        for (const btn of buttons) {
          const svg = btn.querySelector('svg');
          if (svg) {
            const classes = svg.getAttribute('class') || '';
            if (classes.includes('lucide-sun') || classes.includes('lucide-moon')) {
              return true;
            }
          }
        }
        return false;
      });

      expect(hasThemeToggle).toBe(false);
    });
  });

  describe('FIX-002: Workflow 工作区打开文件夹按钮应有效', () => {
    it('打开文件夹按钮应存在', async () => {
      await openWorkflowWorkspace(mainWindow);
      await browser.pause(500);

      const openDirBtn = await $(`button[title="${t('workflows.openDir')}"]`);
      const exists = await openDirBtn.isExisting();
      expect(exists).toBe(true);
    });
  });

  describe('FIX-003: 图表预览弹窗不应有关闭(X)按钮', () => {
    it('图表放大视图应只有还原按钮，没有 X 关闭按钮', async () => {
      const res = await openConnectionWindow();
      mainWindow = res.mainWindow;
      await openQueryTab();
      await browser.pause(1000);

      await executeSQL(
        'SELECT status, COUNT(*) as count FROM product GROUP BY status ORDER BY count DESC LIMIT 10',
      );
      await browser.pause(1000);

      const chartBtn = await $(`button*=${t('chart.viewChart')}`);
      await chartBtn.waitForDisplayed({ timeout: 5000 });
      await chartBtn.click();
      await browser.pause(1000);

      const expandBtn = await $(`button[title="${t('chart.expand')}"]`);
      await expandBtn.waitForDisplayed({ timeout: 5000 });
      await expandBtn.click();
      await browser.pause(500);

      const collapseBtn = await $(`button[title="${t('chart.collapse')}"]`);
      expect(await collapseBtn.isExisting()).toBe(true);

      const xButtons = await browser.execute(() => {
        const overlay = document.querySelector('[class*="fixed inset-0"]');
        if (!overlay) return 0;
        const buttons = overlay.querySelectorAll('button');
        let count = 0;
        for (const btn of buttons) {
          const svg = btn.querySelector('svg');
          if (svg) {
            const classes = svg.getAttribute('class') || '';
            if (classes.includes('lucide-x') && !classes.includes('lucide-x-circle')) {
              count++;
            }
          }
        }
        return count;
      });

      expect(xButtons).toBe(0);

      await browser.keys('Escape');
      await browser.pause(500);
    });
  });
});
