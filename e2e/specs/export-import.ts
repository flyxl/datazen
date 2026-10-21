import { expect, browser, $ } from '@wdio/globals';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { t } from '../i18n.js';
import {
  captureJourneyStep,
  connectBackend,
  connectSeededPgInWorkspace,
  closeExtraWindows,
  closeDataExportDialogIfOpen,
  clickTableInSidebar,
  disconnectBackend,
  executeQuery,
  injectDialogPath,
  openQueryTab,
  openTableFromSidebar,
  resetDialogQueue,
  waitForDataExportDialog,
  waitForSchemaTreeLoaded,
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
  await waitForSchemaTreeLoaded();
  const openPanel = async () => {
    try {
      await clickTableInSidebar(TEST_TABLE);
    } catch {
      await openTableFromSidebar(TEST_TABLE);
    }
    await browser.pause(1500);
  };
  await openPanel();
  let dataTab = await $('[data-testid="sub-tab-data"]');
  if (!(await dataTab.isExisting())) {
    dataTab = await $(`button*=${t('connWin.data')}`);
  }
  if (!(await dataTab.isExisting())) {
    await openTableFromSidebar(TEST_TABLE);
    dataTab = await $('[data-testid="sub-tab-data"]');
    if (!(await dataTab.isExisting())) {
      dataTab = await $(`button*=${t('connWin.data')}`);
    }
  }
  if (await dataTab.isExisting()) {
    await dataTab.waitForDisplayed({ timeout: 10000 });
    await dataTab.click();
    await browser.pause(800);
  }
  await browser.waitUntil(
    async () => {
      const body = await $('body').getText();
      return body.includes(SAMPLE_CELL) || body.includes(t('common.selectAll'));
    },
    { timeout: 15000, timeoutMsg: 'Timed out waiting for table data to load' },
  );
}

async function openTableExportDialog() {
  const dlg = await $('[data-testid="data-export-dialog"]');
  if (await dlg.isDisplayed().catch(() => false)) {
    const hasForm = await browser.execute(() => {
      const d = document.querySelector('[data-testid="data-export-dialog"]');
      return !!d?.querySelector('button[role="combobox"]');
    });
    if (hasForm) return;
    await closeDataExportDialogIfOpen();
  }
  await openTestTableDataView();
  const clicked = await browser.execute(
    (exportLabel: string, sampleCell: string) => {
      const byTestId = document.querySelector(
        '[data-testid="data-table-export"]',
      ) as HTMLElement | null;
      if (byTestId) {
        byTestId.click();
        return true;
      }
      const anchor = document.querySelector(`span[title="${sampleCell}"]`);
      const panel = anchor?.closest('.flex.min-h-0.min-w-0.flex-1.flex-col');
      const scoped = panel?.querySelector(`button[title="${exportLabel}"]`) as HTMLElement | null;
      if (scoped) {
        scoped.click();
        return true;
      }
      return false;
    },
    t('export.export'),
    SAMPLE_CELL,
  );
  if (!clicked) {
    const exportBtn = await $(`button[title="${t('export.export')}"]`);
    await exportBtn.waitForDisplayed({ timeout: 10000 });
    await exportBtn.click();
  }
  await waitForDataExportDialog();
}

describe('导出和导入 (EI-001~EI-006)', () => {
  let mainWindow: string;

  before(async () => {
    mainWindow = await browser.getWindowHandle();
    // Re-seed the fixture `product` table deterministically. The DataTable export
    // button is disabled when the table is empty (DataTable.exportEnabled requires
    // rows.length > 0), and the shared E2E DB may have left `product` empty after
    // earlier specs ran, so construct the 4 canonical rows instead of relying on
    // shared env state. `product` schema: id serial PK, name, status → 4 rows / 3 cols.
    const seedSession = await connectBackend('conn_e2e_pg');
    try {
      await executeQuery(
        seedSession,
        'DELETE FROM product WHERE id > 0; INSERT INTO product (name, status) VALUES ' +
          "('Widget','active'),('Gadget','active'),('Thing','pending'),('Gizmo','inactive');",
      );
    } finally {
      await disconnectBackend(seedSession);
    }
    await connectSeededPgInWorkspace();
    await browser.pause(1000);
    await openTestTableDataView();
  });

  after(async () => {
    if (mainWindow) {
      await closeExtraWindows(mainWindow);
    }
  });

  // ── 导出对话框 ─────────────────────────────────────────────────

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
    expect(body).toMatch(new RegExp(`${t('export.currentPage')}|${t('export.entireTable')}`));
  });

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
      const listbox = document.querySelector('[id^="dz-select-listbox-"]');
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
      const listbox = document.querySelector('[id^="dz-select-listbox-"]');
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

  // ── 导入对话框（仅 Schema 树原生菜单可开，不可 DOM 断言）────────

  it('EI-007: CSV 导出应通过注入对话框落盘', async () => {
    await openTableExportDialog();
    await resetDialogQueue();
    const outPath = path.join(os.tmpdir(), `datazen-export-${Date.now()}.csv`);
    try {
      await injectDialogPath(outPath);
      const dlg = await $('[data-testid="data-export-dialog"]');
      const exportConfirm = await dlg.$(`button*=${t('export.export')}`);
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
