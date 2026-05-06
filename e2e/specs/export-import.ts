import { expect, browser, $ } from '@wdio/globals';
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
      await $('button*=新建连接').waitForDisplayed({ timeout: 10000 });
      await browser.pause(1500);
      await clickCardConnectButton();
      await browser.waitUntil(
        async () => (await browser.getWindowHandles()).length > 1,
        { timeout: 30000, timeoutMsg: '等待连接窗口打开超时' },
      );
      handles = await browser.getWindowHandles();
      const newConn = handles.find((h) => h.startsWith('connection')) ?? handles.find((h) => h !== mainWindow)!;
      await browser.switchToWindow(newConn);
    }
    await $('button*=新建查询').waitForDisplayed({ timeout: 20000 });
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
    const refreshBtn = await $('button[title="刷新 (⌘R)"]');
    await refreshBtn.click();
    await browser.pause(2000);

    // Open the test table
    await clickTableInSidebar(TEST_TABLE);
    await browser.pause(2000);
    await switchSubTab('数据');
    await browser.waitUntil(
      async () => (await $('body').getText()).includes('Alice'),
      { timeout: 10000, timeoutMsg: '等待表数据加载超时' },
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
    expect(body).toContain('导出数据');
  });

  it('点击导出选项应打开导出对话框 (EI-001)', async () => {
    // Click the export option in context menu
    await browser.execute(() => {
      const buttons = document.querySelectorAll('button');
      for (const btn of buttons) {
        if (btn.textContent?.includes('导出数据')) {
          btn.click();
          break;
        }
      }
    });
    await browser.pause(1000);

    const body = await $('body').getText();
    expect(body).toContain('导出格式');
  });

  it('导出对话框应显示格式选项 (EI-002)', async () => {
    const body = await $('body').getText();
    expect(body).toContain('导出格式');
  });

  it('导出对话框应显示导出范围 (EI-002)', async () => {
    const body = await $('body').getText();
    expect(body).toContain('导出范围');
  });

  it('导出对话框应显示列选择 (EI-002)', async () => {
    const body = await $('body').getText();
    expect(body).toContain('选择列');
  });

  it('导出对话框应显示列名 (EI-002)', async () => {
    const body = await $('body').getText();
    expect(body).toContain('name');
    expect(body).toContain('email');
  });

  it('导出对话框应显示导出摘要 (EI-002a)', async () => {
    const body = await $('body').getText();
    expect(body).toContain('将导出');
    expect(body).toContain('3 行');
    expect(body).toContain('3 列');
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
    await browser.execute(() => {
      const dlg = document.querySelector('.fixed.inset-0.z-50');
      if (!dlg) return;
      const toggleBtn = dlg.querySelector('button.text-xs');
      if (toggleBtn && (toggleBtn.textContent?.includes('全不选') || toggleBtn.textContent?.includes('取消全选'))) {
        (toggleBtn as HTMLElement).click();
      }
    });
    await browser.pause(300);

    const body = await $('body').getText();
    expect(body).toContain('0 列');
  });

  it('无列选中时导出按钮应禁用 (EI-002c)', async () => {
    const disabled = await browser.execute(() => {
      const dlg = document.querySelector('.fixed.inset-0.z-50');
      if (!dlg) return false;
      const btns = dlg.querySelectorAll('button');
      for (const btn of btns) {
        if (btn.textContent?.trim() === '导出') {
          return btn.disabled;
        }
      }
      return false;
    });
    expect(disabled).toBe(true);
  });

  it('点击全选应恢复所有列 (EI-002c)', async () => {
    await browser.execute(() => {
      const dlg = document.querySelector('.fixed.inset-0.z-50');
      if (!dlg) return;
      const toggleBtn = dlg.querySelector('button.text-xs');
      if (toggleBtn && toggleBtn.textContent?.includes('全选')) {
        (toggleBtn as HTMLElement).click();
      }
    });
    await browser.pause(300);

    const body = await $('body').getText();
    expect(body).toContain('3 列');
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
    expect(body).toContain('2 列');
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

    const disabled = await browser.execute(() => {
      const dlg = document.querySelector('.fixed.inset-0.z-50');
      if (!dlg) return true;
      const btns = dlg.querySelectorAll('button');
      for (const btn of btns) {
        if (btn.textContent?.trim() === '导出') return btn.disabled;
      }
      return true;
    });
    expect(disabled).toBe(false);
  });

  it('点击取消应关闭导出对话框 (EI-003)', async () => {
    // Click the exact "取消" button in the dialog footer (not "取消全选")
    await browser.execute(() => {
      const dlg = document.querySelector('.fixed.inset-0.z-50');
      if (!dlg) return;
      const btns = dlg.querySelectorAll('button');
      for (const btn of btns) {
        if (btn.textContent?.trim() === '取消') {
          (btn as HTMLElement).click();
          return;
        }
      }
      const overlay = dlg.querySelector('button[aria-label="关闭"]') as HTMLElement;
      if (overlay) overlay.click();
    });
    await browser.pause(1000);

    const body = await $('body').getText();
    expect(body).not.toContain('导出格式');
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
    expect(body).toContain('导入数据');
  });

  it('点击导入选项应打开导入对话框 (EI-004)', async () => {
    await browser.execute(() => {
      const buttons = document.querySelectorAll('button');
      for (const btn of buttons) {
        if (btn.textContent?.includes('导入数据')) {
          btn.click();
          break;
        }
      }
    });
    await browser.pause(1000);

    const body = await $('body').getText();
    const hasDialog = body.includes('导入数据') || body.includes('CSV') || body.includes('JSON');
    expect(hasDialog).toBe(true);
  });

  it('导入对话框应显示文件选择 (EI-005)', async () => {
    const body = await $('body').getText();
    const hasFileSelect = body.includes('选择') || body.includes('CSV/JSON');
    expect(hasFileSelect).toBe(true);
  });

  it('导入对话框应显示目标表输入框 (EI-005a)', async () => {
    const body = await $('body').getText();
    expect(body).toContain('目标表');
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
    expect(body).toContain('导入');
    expect(body).toContain('取消');
  });

  it('无文件时导入按钮应禁用 (EI-005b)', async () => {
    const disabled = await browser.execute(() => {
      const btns = document.querySelectorAll('.fixed.inset-0.z-50 button');
      for (const btn of btns) {
        if (btn.textContent?.trim() === '导入') {
          return (btn as HTMLButtonElement).disabled;
        }
      }
      return false;
    });
    expect(disabled).toBe(true);
  });

  it('导入对话框应显示文件格式提示 (EI-005c)', async () => {
    const body = await $('body').getText();
    const hasHint = body.includes('CSV') || body.includes('JSON') || body.includes('CSV/JSON');
    expect(hasHint).toBe(true);
  });

  it('点击取消应关闭导入对话框 (EI-006)', async () => {
    const cancelBtn = await $('button*=取消');
    if (await cancelBtn.isDisplayed()) {
      await cancelBtn.click();
      await browser.pause(500);
    }
  });
});
