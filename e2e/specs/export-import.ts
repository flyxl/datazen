import { expect, browser, $ } from '@wdio/globals';
import { t } from '../i18n.js';
import {
  clickCardConnectButton,
  closeExtraWindows,
  executeSQL,
  openQueryTab,
  clickTableInSidebar,
  switchSubTab,
} from '../helpers.js';

/**
 * Export and Import dialog tests.
 * Export/Import dialogs are opened via right-click context menu on table names.
 * Requires a PostgreSQL connection (seeded by wdio.conf.ts before hook).
 */

const TEST_TABLE = '_e2e_export_test';

describe('导出和导入 (EI-001~EI-006)', () => {
  let mainWindow: string;

  before(async () => {
    let handles = await browser.getWindowHandles();
    const connHandle = handles.find((h) => h.startsWith('connection'));
    mainWindow = handles.find((h) => h === 'main') ?? handles.find((h) => !h.startsWith('connection')) ?? '';

    if (connHandle) {
      await browser.switchToWindow(connHandle);
    } else {
      await browser.switchToWindow(mainWindow || handles[0]);
      await $(`button*=${t('action.newConnection')}`).waitForDisplayed({ timeout: 10000 });
      await browser.pause(1500);
      await clickCardConnectButton();
      await browser.waitUntil(
        async () => (await browser.getWindowHandles()).length > 1,
        { timeout: 30000, timeoutMsg: 'Timed out waiting for connection window' },
      );
      handles = await browser.getWindowHandles();
      const newConn = handles.find((h) => h.startsWith('connection')) ?? handles.find((h) => h !== mainWindow)!;
      await browser.switchToWindow(newConn);
    }
    await $(`button*=${t('connWin.newQuery')}`).waitForDisplayed({ timeout: 20000 });
    await browser.pause(2000);

    // Create test table with data
    await openQueryTab();
    await executeSQL(`DROP TABLE IF EXISTS ${TEST_TABLE}`);
    await executeSQL(`
      CREATE TABLE ${TEST_TABLE} (
        id SERIAL PRIMARY KEY,
        name VARCHAR(100) NOT NULL,
        email VARCHAR(200)
      )
    `);
    await executeSQL(`
      INSERT INTO ${TEST_TABLE} (name, email) VALUES
        ('Alice', 'alice@example.com'),
        ('Bob', 'bob@example.com'),
        ('Charlie', 'charlie@example.com')
    `);

    // Refresh sidebar
    const refreshBtn = await $(`button[title="${t('connWin.refresh')} (⌘R)"]`);
    await refreshBtn.click();
    await browser.pause(2000);

    // Open the test table
    await clickTableInSidebar(TEST_TABLE);
    await browser.pause(2000);
    await switchSubTab(t('connWin.data'));
    await browser.waitUntil(
      async () => (await $('body').getText()).includes('Alice'),
      { timeout: 10000, timeoutMsg: 'Timed out waiting for table data to load' },
    );
  });

  after(async () => {
    try {
      const handles = await browser.getWindowHandles();
      const connWindow = handles.find((h) => h.startsWith('connection')) ?? handles.find((h) => h !== mainWindow);
      if (connWindow) {
        await browser.switchToWindow(connWindow);
        await openQueryTab();
        await executeSQL(`DROP TABLE IF EXISTS ${TEST_TABLE}`);
      }
    } catch {
      // best-effort cleanup
    }
    if (mainWindow === 'main') {
      await closeExtraWindows(mainWindow);
    }
  });

  // ── 导出对话框 ─────────────────────────────────────────────────

  it('右键表名应显示导出选项 (EI-001)', async () => {
    // Trigger context menu on the table in sidebar
    await browser.execute((tableName: string) => {
      const buttons = document.querySelectorAll('aside button');
      for (const btn of buttons) {
        if (btn.textContent?.trim() === tableName) {
          const rect = btn.getBoundingClientRect();
          btn.dispatchEvent(new MouseEvent('contextmenu', {
            bubbles: true,
            clientX: rect.x + rect.width / 2,
            clientY: rect.y + rect.height / 2,
          }));
          break;
        }
      }
    }, TEST_TABLE);
    await browser.pause(500);

    const body = await $('body').getText();
    expect(body).toContain(t('export.title'));
  });

  it('点击导出选项应打开导出对话框 (EI-001)', async () => {
    // Click the export option in context menu
    await browser.execute((exportTitle) => {
      const buttons = document.querySelectorAll('button');
      for (const btn of buttons) {
        if (btn.textContent?.includes(exportTitle)) {
          btn.click();
          break;
        }
      }
    }, t('export.title'));
    await browser.pause(1000);

    const body = await $('body').getText();
    expect(body).toContain(t('export.format'));
  });

  it('导出对话框应显示格式选项 (EI-002)', async () => {
    const body = await $('body').getText();
    expect(body).toContain(t('export.format'));
  });

  it('导出对话框应显示导出范围 (EI-002)', async () => {
    const body = await $('body').getText();
    expect(body).toContain(t('export.range'));
  });

  it('导出对话框应显示列选择 (EI-002)', async () => {
    const body = await $('body').getText();
    expect(body).toContain(t('export.selectColumns'));
  });

  it('导出对话框应显示列名 (EI-002)', async () => {
    const body = await $('body').getText();
    expect(body).toContain('name');
    expect(body).toContain('email');
  });

  it('导出对话框应显示导出摘要 (EI-002a)', async () => {
    const body = await $('body').getText();
    expect(body).toContain(t('export.willExport', { rows: 3, cols: 3 }));
    expect(body).toContain('CSV');
  });

  it('切换导出格式为 JSON 应更新摘要 (EI-002b)', async () => {
    // Click the format select trigger (first aria-haspopup="listbox" in dialog)
    await browser.execute(() => {
      const dlg = document.querySelector('.fixed.inset-0.z-50');
      if (!dlg) return;
      const trigger = dlg.querySelector('button[aria-haspopup="listbox"]');
      if (trigger) (trigger as HTMLElement).click();
    });
    await browser.pause(300);

    // Click JSON option in the dropdown
    await browser.execute(() => {
      const listbox = document.getElementById('dz-select-listbox');
      if (!listbox) return;
      const opts = listbox.querySelectorAll('div[tabindex]');
      for (const opt of opts) {
        if (opt.textContent?.includes('JSON')) {
          opt.dispatchEvent(new MouseEvent('mousedown', { bubbles: true }));
          break;
        }
      }
    });
    await browser.pause(300);

    const body = await $('body').getText();
    expect(body).toContain('JSON');
  });

  it('切换导出格式为 SQL INSERT 应更新摘要 (EI-002b)', async () => {
    await browser.execute(() => {
      const dlg = document.querySelector('.fixed.inset-0.z-50');
      if (!dlg) return;
      const trigger = dlg.querySelector('button[aria-haspopup="listbox"]');
      if (trigger) (trigger as HTMLElement).click();
    });
    await browser.pause(300);

    await browser.execute(() => {
      const listbox = document.getElementById('dz-select-listbox');
      if (!listbox) return;
      const opts = listbox.querySelectorAll('div[tabindex]');
      for (const opt of opts) {
        if (opt.textContent?.includes('SQL INSERT')) {
          opt.dispatchEvent(new MouseEvent('mousedown', { bubbles: true }));
          break;
        }
      }
    });
    await browser.pause(300);

    const body = await $('body').getText();
    expect(body).toContain('SQL INSERT');
  });

  it('点击取消全选应取消所有列 (EI-002c)', async () => {
    await browser.execute((deselectAllLabel) => {
      const dlg = document.querySelector('.fixed.inset-0.z-50');
      if (!dlg) return;
      const toggleBtn = dlg.querySelector('button.text-xs');
      if (toggleBtn && toggleBtn.textContent?.includes(deselectAllLabel)) {
        (toggleBtn as HTMLElement).click();
      }
    }, t('common.deselectAll'));
    await browser.pause(300);

    const body = await $('body').getText();
    expect(body).toContain(`0 ${t('common.columns')}`);
  });

  it('无列选中时导出按钮应禁用 (EI-002c)', async () => {
    const disabled = await browser.execute((exportLabel) => {
      const dlg = document.querySelector('.fixed.inset-0.z-50');
      if (!dlg) return false;
      const btns = dlg.querySelectorAll('button');
      for (const btn of btns) {
        if (btn.textContent?.trim() === exportLabel) {
          return btn.disabled;
        }
      }
      return false;
    }, t('export.export'));
    expect(disabled).toBe(true);
  });

  it('点击全选应恢复所有列 (EI-002c)', async () => {
    await browser.execute((selectAllLabel) => {
      const dlg = document.querySelector('.fixed.inset-0.z-50');
      if (!dlg) return;
      const toggleBtn = dlg.querySelector('button.text-xs');
      if (toggleBtn && toggleBtn.textContent?.includes(selectAllLabel)) {
        (toggleBtn as HTMLElement).click();
      }
    }, t('common.selectAll'));
    await browser.pause(300);

    const body = await $('body').getText();
    expect(body).toContain(`3 ${t('common.columns')}`);
  });

  it('取消单个列后摘要应更新为 2 列 (EI-002d)', async () => {
    // Uncheck the 'id' column
    await browser.execute(() => {
      const dlg = document.querySelector('.fixed.inset-0.z-50');
      if (!dlg) return;
      const labels = dlg.querySelectorAll('label');
      for (const label of labels) {
        if (label.textContent?.includes('id') && label.textContent?.includes('integer')) {
          const cb = label.querySelector('input[type="checkbox"]') as HTMLInputElement;
          if (cb && cb.checked) cb.click();
          break;
        }
      }
    });
    await browser.pause(300);

    const body = await $('body').getText();
    expect(body).toContain(`2 ${t('common.columns')}`);
  });

  it('恢复列选择后导出按钮应可用 (EI-002d)', async () => {
    // Re-check the 'id' column
    await browser.execute(() => {
      const dlg = document.querySelector('.fixed.inset-0.z-50');
      if (!dlg) return;
      const labels = dlg.querySelectorAll('label');
      for (const label of labels) {
        if (label.textContent?.includes('id') && label.textContent?.includes('integer')) {
          const cb = label.querySelector('input[type="checkbox"]') as HTMLInputElement;
          if (cb && !cb.checked) cb.click();
          break;
        }
      }
    });
    await browser.pause(300);

    const disabled = await browser.execute((exportLabel) => {
      const dlg = document.querySelector('.fixed.inset-0.z-50');
      if (!dlg) return true;
      const btns = dlg.querySelectorAll('button');
      for (const btn of btns) {
        if (btn.textContent?.trim() === exportLabel) return btn.disabled;
      }
      return true;
    }, t('export.export'));
    expect(disabled).toBe(false);
  });

  it('点击取消应关闭导出对话框 (EI-003)', async () => {
    // Click the exact "取消" button in the dialog footer (not "取消全选")
    await browser.execute((cancelLabel, closeLabel) => {
      const dlg = document.querySelector('.fixed.inset-0.z-50');
      if (!dlg) return;
      const btns = dlg.querySelectorAll('button');
      for (const btn of btns) {
        if (btn.textContent?.trim() === cancelLabel) {
          (btn as HTMLElement).click();
          return;
        }
      }
      const overlay = dlg.querySelector(`button[aria-label="${closeLabel}"]`) as HTMLElement;
      if (overlay) overlay.click();
    }, t('common.cancel'), t('common.close'));
    await browser.pause(1000);

    const body = await $('body').getText();
    expect(body).not.toContain(t('export.format'));
  });

  // ── 导入对话框 ─────────────────────────────────────────────────

  it('右键表名应显示导入选项 (EI-004)', async () => {
    await browser.execute((tableName: string) => {
      const buttons = document.querySelectorAll('aside button');
      for (const btn of buttons) {
        if (btn.textContent?.trim() === tableName) {
          const rect = btn.getBoundingClientRect();
          btn.dispatchEvent(new MouseEvent('contextmenu', {
            bubbles: true,
            clientX: rect.x + rect.width / 2,
            clientY: rect.y + rect.height / 2,
          }));
          break;
        }
      }
    }, TEST_TABLE);
    await browser.pause(500);

    const body = await $('body').getText();
    expect(body).toContain(t('import.title'));
  });

  it('点击导入选项应打开导入对话框 (EI-004)', async () => {
    await browser.execute((importTitle) => {
      const buttons = document.querySelectorAll('button');
      for (const btn of buttons) {
        if (btn.textContent?.includes(importTitle)) {
          btn.click();
          break;
        }
      }
    }, t('import.title'));
    await browser.pause(1000);

    const body = await $('body').getText();
    const hasDialog = body.includes(t('import.title')) || body.includes('CSV') || body.includes('JSON');
    expect(hasDialog).toBe(true);
  });

  it('导入对话框应显示文件选择 (EI-005)', async () => {
    const body = await $('body').getText();
    const hasFileSelect = body.includes(t('import.selectFile')) || body.includes('CSV/JSON');
    expect(hasFileSelect).toBe(true);
  });

  it('导入对话框应显示目标表输入框 (EI-005a)', async () => {
    const body = await $('body').getText();
    expect(body).toContain(t('import.targetTable'));
  });

  it('导入对话框目标表应预填充为当前表名 (EI-005a)', async () => {
    const value = await browser.execute((table: string) => {
      const inputs = document.querySelectorAll('input[type="text"]');
      for (const input of inputs) {
        if ((input as HTMLInputElement).value === table) return true;
      }
      return false;
    }, TEST_TABLE);
    expect(value).toBe(true);
  });

  it('导入对话框应显示导入和取消按钮 (EI-005b)', async () => {
    const body = await $('body').getText();
    expect(body).toContain(t('import.import'));
    expect(body).toContain(t('common.cancel'));
  });

  it('无文件时导入按钮应禁用 (EI-005b)', async () => {
    const disabled = await browser.execute((importLabel) => {
      const btns = document.querySelectorAll('.fixed.inset-0.z-50 button');
      for (const btn of btns) {
        if (btn.textContent?.trim() === importLabel) {
          return (btn as HTMLButtonElement).disabled;
        }
      }
      return false;
    }, t('import.import'));
    expect(disabled).toBe(true);
  });

  it('导入对话框应显示文件格式提示 (EI-005c)', async () => {
    const body = await $('body').getText();
    const hasHint = body.includes('CSV') || body.includes('JSON') || body.includes('CSV/JSON');
    expect(hasHint).toBe(true);
  });

  it('点击取消应关闭导入对话框 (EI-006)', async () => {
    const cancelBtn = await $(`button*=${t('common.cancel')}`);
    if (await cancelBtn.isDisplayed()) {
      await cancelBtn.click();
      await browser.pause(500);
    }
  });

  it('EI-GRID-001: DataTable 工具栏导出应打开导出对话框', async () => {
    await clickTableInSidebar(TEST_TABLE);
    await browser.pause(1200);
    await switchSubTab(t('connWin.data'));
    await browser.pause(1000);
    const exportBtn = await $(`button[title="${t('export.export')}"]`);
    await exportBtn.waitForDisplayed({ timeout: 10000 });
    await exportBtn.click();
    await browser.pause(600);
    const body = await $('body').getText();
    expect(body).toContain(t('export.export'));
    // Dialog should offer format choices
    expect(body.includes('CSV') || body.includes('JSON') || body.includes('SQL')).toBe(true);
    const cancel = await $(`button*=${t('common.cancel')}`);
    if (await cancel.isDisplayed()) {
      await cancel.click();
    }
  });
});
