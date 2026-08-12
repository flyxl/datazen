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
 * Export dialog is opened via DataTable toolbar (native OS context menus are not DOM-assertable).
 * Batch export dialog is opened via Connection Window toolbar (EI-BE-001); Schema-tree native menu path is skipped.
 * Import dialog is only reachable via schema-tree native context menu → those cases are skipped.
 * Requires a PostgreSQL connection (seeded by wdio.conf.ts before hook).
 */

const TEST_TABLE = '_e2e_export_test';

describe('导出和导入 (EI-001~EI-006)', () => {
  let mainWindow: string;

  before(async () => {
    let handles = await browser.getWindowHandles();
    const connHandle = handles.find((h) => h.startsWith('connection'));
    mainWindow =
      handles.find((h) => h === 'main') ?? handles.find((h) => !h.startsWith('connection')) ?? '';

    if (connHandle) {
      await browser.switchToWindow(connHandle);
    } else {
      await browser.switchToWindow(mainWindow || handles[0]);
      await $(`button*=${t('action.newConnection')}`).waitForDisplayed({ timeout: 10000 });
      await browser.pause(1500);
      await clickCardConnectButton();
      await browser.waitUntil(async () => (await browser.getWindowHandles()).length > 1, {
        timeout: 30000,
        timeoutMsg: 'Timed out waiting for connection window',
      });
      handles = await browser.getWindowHandles();
      const newConn =
        handles.find((h) => h.startsWith('connection')) ?? handles.find((h) => h !== mainWindow)!;
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
    await browser.waitUntil(async () => (await $('body').getText()).includes('Alice'), {
      timeout: 10000,
      timeoutMsg: 'Timed out waiting for table data to load',
    });
  });

  after(async () => {
    try {
      const handles = await browser.getWindowHandles();
      const connWindow =
        handles.find((h) => h.startsWith('connection')) ?? handles.find((h) => h !== mainWindow);
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

  it.skip('右键表名应显示导出选项 (EI-001) — SKIPPED: native OS menu not DOM-assertable', async () => {});

  it('点击工具栏导出应打开导出对话框 (EI-001 → toolbar)', async () => {
    const exportBtn = await $(`button[title="${t('export.export')}"]`);
    await exportBtn.waitForDisplayed({ timeout: 10000 });
    await exportBtn.click();
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

  it.skip('导出对话框应显示列选择 (EI-002) — SKIPPED: DataExportDialog has no column checkboxes', async () => {});

  it.skip('导出对话框应显示列名 (EI-002) — SKIPPED: DataExportDialog has no column checkboxes', async () => {});

  it('导出对话框应显示导出摘要 (EI-002a)', async () => {
    const body = await $('body').getText();
    expect(body).toContain(t('export.willExport', { rows: 3, cols: 3 }));
    expect(body).toContain('CSV');
  });

  it('切换导出格式为 JSON 应更新摘要 (EI-002b)', async () => {
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

  it.skip('点击取消全选应取消所有列 (EI-002c) — SKIPPED: DataExportDialog has no column checkboxes', async () => {});

  it.skip('无列选中时导出按钮应禁用 (EI-002c) — SKIPPED: DataExportDialog has no column checkboxes', async () => {});

  it.skip('点击全选应恢复所有列 (EI-002c) — SKIPPED: DataExportDialog has no column checkboxes', async () => {});

  it.skip('取消单个列后摘要应更新为 2 列 (EI-002d) — SKIPPED: DataExportDialog has no column checkboxes', async () => {});

  it.skip('恢复列选择后导出按钮应可用 (EI-002d) — SKIPPED: DataExportDialog has no column checkboxes', async () => {});

  it('点击取消应关闭导出对话框 (EI-003)', async () => {
    await browser.execute(
      (cancelLabel, closeLabel) => {
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
      },
      t('common.cancel'),
      t('common.close'),
    );
    await browser.pause(1000);

    const body = await $('body').getText();
    expect(body).not.toContain(t('export.format'));
  });

  // ── 批量导出（顶栏可 DOM 断言；Schema 树原生菜单不可）────────

  it('顶栏批量导出按钮应存在并可打开对话框 (EI-BE-001)', async () => {
    const batchBtn = await $(`button[title="${t('batchExport.title')}"]`);
    await batchBtn.waitForDisplayed({ timeout: 10000 });
    await batchBtn.click();
    await browser.pause(1000);

    const body = await $('body').getText();
    expect(body).toContain(t('batchExport.title'));
    expect(body).toContain(t('batchExport.selectTables'));

    await browser.execute(
      (cancelLabel, closeLabel) => {
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
      },
      t('common.cancel'),
      t('common.close'),
    );
    await browser.pause(500);
  });

  it.skip('Schema 树右键应显示批量导出选项 (EI-BE-002) — SKIPPED: native OS menu not DOM-assertable', async () => {});

  // ── 导入对话框（仅 Schema 树原生菜单可开，不可 DOM 断言）────────

  it.skip('右键表名应显示导入选项 (EI-004) — SKIPPED: native OS menu not DOM-assertable', async () => {});

  it.skip('点击导入选项应打开导入对话框 (EI-004) — SKIPPED: Import only via native OS menu', async () => {});

  it.skip('导入对话框应显示文件选择 (EI-005) — SKIPPED: Import dialog not openable without native menu', async () => {});

  it.skip('导入对话框应显示目标表输入框 (EI-005a) — SKIPPED: Import dialog not openable without native menu', async () => {});

  it.skip('导入对话框目标表应预填充为当前表名 (EI-005a) — SKIPPED: Import dialog not openable without native menu', async () => {});

  it.skip('导入对话框应显示导入和取消按钮 (EI-005b) — SKIPPED: Import dialog not openable without native menu', async () => {});

  it.skip('无文件时导入按钮应禁用 (EI-005b) — SKIPPED: Import dialog not openable without native menu', async () => {});

  it.skip('导入对话框应显示文件格式提示 (EI-005c) — SKIPPED: Import dialog not openable without native menu', async () => {});

  it.skip('点击取消应关闭导入对话框 (EI-006) — SKIPPED: Import dialog not openable without native menu', async () => {});

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
