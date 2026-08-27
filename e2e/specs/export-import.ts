import { expect, browser, $ } from '@wdio/globals';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { t } from '../i18n.js';
import {
  captureJourneyStep,
  connectSeededPgInWorkspace,
  closeExtraWindows,
  clickTableInSidebar,
  closeDataExportDialogIfOpen,
  injectDialogPath,
  resetDialogQueue,
  rightClickTableInSidebar,
  waitForDataExportDialog,
} from '../helpers.js';

/**
 * Export and Import dialog tests.
 * Export dialog is opened via DataTable toolbar (native OS context menus are not DOM-assertable).
 * Batch export dialog is opened via Connection Window toolbar (EI-BE-001); Schema-tree native menu path is skipped.
 * Import dialog is only reachable via schema-tree native context menu → those cases are skipped.
 * Uses the seeded `product` table (see e2e/setup-e2e-env.sh) to avoid flaky post-DDL schema refresh.
 */

const TEST_TABLE = 'product';
const SAMPLE_CELL = 'Widget';

async function openTestTableDataView() {
  await clickTableInSidebar(TEST_TABLE);
  await browser.pause(400);
  await browser.execute(
    (tableName: string, openLabel: string) => {
      const node = document.querySelector(
        `[data-tree-node="table"][data-item-name="${tableName}"]`,
      ) as HTMLElement | null;
      if (!node) return;
      node.dispatchEvent(
        new MouseEvent('contextmenu', {
          bubbles: true,
          cancelable: true,
          clientX: 80,
          clientY: 120,
        }),
      );
      const menuItems = document.querySelectorAll('[data-testid="web-context-menu"] button');
      for (const item of menuItems) {
        if (item.textContent?.includes(openLabel)) {
          (item as HTMLElement).click();
          return;
        }
      }
    },
    TEST_TABLE,
    t('schemaTree.openTable'),
  );
  await browser.pause(2000);
  await browser.execute((label: string) => {
    const bars = document.querySelectorAll('.border-b.border-edge.bg-surface-alt');
    for (const bar of bars) {
      for (const btn of bar.querySelectorAll('button')) {
        if ((btn.textContent ?? '').trim() === label) {
          (btn as HTMLElement).click();
          return;
        }
      }
    }
  }, t('connWin.data'));
  await browser.pause(800);
  await browser.waitUntil(
    async () => {
      const body = await $('body').getText();
      return body.includes(SAMPLE_CELL) || body.includes(t('common.selectAll'));
    },
    { timeout: 15000, timeoutMsg: 'Timed out waiting for table data to load' },
  );
}

async function openTableExportDialog() {
  await closeDataExportDialogIfOpen();
  await openTestTableDataView();
  const exportBtn = await $(`button[title="${t('export.export')}"]`);
  await exportBtn.waitForDisplayed({ timeout: 10000 });
  await exportBtn.click();
  await waitForDataExportDialog();
}

async function rightClickTableNode(tableName: string) {
  await rightClickTableInSidebar(tableName);
}

describe('导出和导入 (EI-001~EI-006)', () => {
  let mainWindow: string;

  before(async () => {
    mainWindow = await browser.getWindowHandle();
    await connectSeededPgInWorkspace();
    await $(`button*=${t('connWin.newQuery')}`).waitForDisplayed({ timeout: 20000 });
    await browser.pause(1500);
    await openTestTableDataView();
  });

  after(async () => {
    if (mainWindow) {
      await closeExtraWindows(mainWindow);
    }
  });

  // ── 导出对话框 ─────────────────────────────────────────────────

  it('右键表名应显示导出数据与导出 (EI-001)', async () => {
    await rightClickTableNode(TEST_TABLE);
    const text = await $('[data-testid="web-context-menu"]').getText();
    expect(text).toContain(t('connWin.exportData'));
    expect(text).toContain(t('batchExport.title'));
    await browser.keys('Escape');
  });

  it('点击工具栏导出应打开导出对话框 (EI-001 → toolbar)', async () => {
    await openTableExportDialog();
  });

  it('导出对话框应显示格式选项 (EI-002)', async () => {
    await openTableExportDialog();
    const body = await $('body').getText();
    expect(body).toContain(t('export.format'));
  });

  it('导出对话框应显示导出范围 (EI-002)', async () => {
    await openTableExportDialog();
    const body = await $('body').getText();
    expect(body).toContain(t('export.range'));
    expect(body).toContain(t('export.entireTable'));
  });

  it.skip('导出对话框应显示列选择 (EI-002) — SKIPPED: DataExportDialog has no column checkboxes', async () => {});

  it.skip('导出对话框应显示列名 (EI-002) — SKIPPED: DataExportDialog has no column checkboxes', async () => {});

  it('导出对话框应显示导出摘要 (EI-002a)', async () => {
    await openTableExportDialog();
    const body = await $('body').getText();
    expect(body).toContain(t('export.willExport', { rows: 4, cols: 3 }));
    expect(body).toContain('CSV');
  });

  it('切换导出格式为 JSON 应更新摘要 (EI-002b)', async () => {
    await openTableExportDialog();
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
    await openTableExportDialog();
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
    await openTableExportDialog();
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

  // ── 导出（顶栏可 DOM 断言；Schema 树原生菜单不可）────────

  it('顶栏导出按钮应存在并可打开对话框 (EI-BE-001)', async () => {
    const batchBtn = await $('[data-testid="conn-toolbar-export"]');
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

  it('Schema 树右键应显示批量导出选项 (EI-BE-002)', async () => {
    await rightClickTableNode(TEST_TABLE);
    const item = await $('[data-testid="web-context-item-batch-export"]');
    await item.waitForDisplayed({ timeout: 5000 });
    await item.click();
    await browser.pause(600);
    const body = await $('body').getText();
    expect(body).toContain(t('batchExport.selectTables'));
    const cancel = await $(`button*=${t('common.cancel')}`);
    if (await cancel.isDisplayed()) await cancel.click();
  });

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

  it('EI-007: CSV 导出应通过注入对话框落盘', async () => {
    await openTableExportDialog();
    await resetDialogQueue();
    const outPath = path.join(os.tmpdir(), `datazen-export-${Date.now()}.csv`);
    try {
      await injectDialogPath(outPath);
      const exportConfirm = await $(`button*=${t('export.export')}`);
      await exportConfirm.waitForClickable({ timeout: 5000 });
      await exportConfirm.click();
      await browser.waitUntil(() => fs.existsSync(outPath), {
        timeout: 20000,
        timeoutMsg: 'Timed out waiting for export file',
      });
      expect(fs.readFileSync(outPath, 'utf-8')).toContain(SAMPLE_CELL);
      await captureJourneyStep('export-csv-saved', 0, true);
    } finally {
      try {
        fs.unlinkSync(outPath);
      } catch {
        /* ok */
      }
      const cancel = await $(`button*=${t('common.cancel')}`);
      if (await cancel.isDisplayed()) await cancel.click();
    }
  });

  it('EI-GRID-001: DataTable 工具栏导出应打开导出对话框', async () => {
    await openTableExportDialog();
    const body = await $('body').getText();
    expect(body.includes('CSV') || body.includes('JSON') || body.includes('SQL')).toBe(true);
    await closeDataExportDialogIfOpen();
  });
});
