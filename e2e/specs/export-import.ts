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
  clickNavigatorRefresh,
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
 * Import wizard (DBR-01) opens through the schema-tree **web** context menu
 * (`showWebContextMenu` renders `[data-testid="web-context-menu"]`, so the
 * menu IS DOM-clickable) and the file picker honors the injected-dialog queue
 * (`open_file` consumes injected answers first) — EI-IMP-001…
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

  // ── 导入向导（DBR-01：schema 树 web 菜单 + 注入文件对话框）──────

  const IMPORT_TABLE = '_e2e_import_wizard';

  /** Deterministically close any residual web context menu. */
  async function closeAnyMenu() {
    await browser.execute(() => {
      document.querySelectorAll('[data-testid="web-context-menu"]').forEach((menu) => {
        menu.dispatchEvent(new MouseEvent('mousedown', { bubbles: true }));
      });
    });
    await browser.pause(200);
  }

  /** Right-click a schema-tree node via JS dispatch (navigator-menu pattern). */
  async function rightClickTreeNode(selector: string) {
    await closeAnyMenu();
    await browser.execute((sel: string) => {
      const el = document.querySelector(sel);
      if (!el) return;
      const rect = (el as HTMLElement).getBoundingClientRect();
      el.dispatchEvent(
        new MouseEvent('contextmenu', {
          bubbles: true,
          cancelable: true,
          clientX: rect.left + rect.width / 2,
          clientY: rect.top + rect.height / 2,
        }),
      );
    }, selector);
    const menu = await $('[data-testid="web-context-menu"]');
    await menu.waitForDisplayed({ timeout: 8000 });
  }

  /** Click a labeled item inside the open web context menu. */
  async function clickContextMenuItem(label: string) {
    await browser.execute((lbl: string) => {
      const items = document.querySelectorAll('[data-testid="web-context-menu"] button');
      for (const item of items) {
        if (item.textContent?.includes(lbl)) {
          (item as HTMLElement).click();
          return;
        }
      }
    }, label);
    await browser.pause(400);
  }

  /** Create a clean target table and make it visible in the tree. */
  async function seedImportTargetTable() {
    const seedSession = await connectBackend('conn_e2e_pg');
    try {
      await executeQuery(
        seedSession,
        `DROP TABLE IF EXISTS ${IMPORT_TABLE}; CREATE TABLE ${IMPORT_TABLE} ` +
          '(id SERIAL PRIMARY KEY, name TEXT, status TEXT);',
      );
    } finally {
      await disconnectBackend(seedSession);
    }
  }

  async function dropImportTargetTable() {
    const seedSession = await connectBackend('conn_e2e_pg');
    try {
      await executeQuery(seedSession, `DROP TABLE IF EXISTS ${IMPORT_TABLE};`);
    } finally {
      await disconnectBackend(seedSession);
    }
  }

  /** Open the import wizard on the dedicated table via the tree menu. */
  async function openImportWizardOnTable() {
    await waitForSchemaTreeLoaded();
    // The table was created over a raw backend session — the workspace tree
    // only learns about it after a refresh.
    await clickNavigatorRefresh();
    await browser.pause(800);
    await rightClickTreeNode(`[data-tree-node="table"][data-item-name="${IMPORT_TABLE}"]`);
    await clickContextMenuItem(t('common.importData'));
    await browser.pause(600);
  }

  it('EI-IMP-001: 树菜单打开导入向导并解析注入的 CSV (DBR-01)', async () => {
    await seedImportTargetTable();
    try {
      await openImportWizardOnTable();

      // The file picker consumes the injected dialog answer.
      await resetDialogQueue();
      const csvPath = path.join(os.tmpdir(), `datazen-import-${Date.now()}.csv`);
      fs.writeFileSync(
        csvPath,
        ['name,status', 'FromImportWizard,active', 'SecondRow,pending'].join('\n') + '\n',
        'utf-8',
      );
      await injectDialogPath(csvPath);
      const pickButton = await $(`button*=${t('import.selectFile')}`);
      await pickButton.waitForClickable({ timeout: 8000 });
      await pickButton.click();
      await browser.pause(1200);

      // The wizard parsed the file: mapping table + summary + preview.
      const mapping = await $('[data-testid="import-mapping-table"]');
      await mapping.waitForDisplayed({ timeout: 10000 });
      const summary = await $('[data-testid="import-summary"]');
      expect(await summary.getText()).toBeTruthy();
      const preview = await $('[data-testid="import-preview"]');
      expect(await preview.getText()).toContain('FromImportWizard');
      fs.unlinkSync(csvPath);

      // Close via cancel.
      const cancel = await $(`button*=${t('common.cancel')}`);
      if (await cancel.isDisplayed()) await cancel.click();
      await browser.pause(400);
    } finally {
      await dropImportTargetTable().catch(() => undefined);
    }
  });

  it('EI-IMP-002: 完成导入后目标表出现新行 (DBR-01)', async () => {
    await seedImportTargetTable();
    try {
      await openImportWizardOnTable();

      await resetDialogQueue();
      const csvPath = path.join(os.tmpdir(), `datazen-import-run-${Date.now()}.csv`);
      fs.writeFileSync(
        csvPath,
        ['name,status', 'CommittedRowOne,active', 'CommittedRowTwo,pending'].join('\n') + '\n',
        'utf-8',
      );
      await injectDialogPath(csvPath);
      const pickButton = await $(`button*=${t('import.selectFile')}`);
      await pickButton.waitForClickable({ timeout: 8000 });
      await pickButton.click();
      await browser.pause(1200);

      // Start the import (label carries the row count).
      const start = await $(`button*=${t('import.start', { count: 2 })}`);
      await start.waitForClickable({ timeout: 10000 });
      await start.click();

      // The finished run reports the committed count.
      const result = await $('[data-testid="import-result"]');
      await result.waitForDisplayed({ timeout: 30000 });
      const resultText = await result.getText();
      expect(resultText).toContain(t('import.result.completed', { committed: 2 }));
      fs.unlinkSync(csvPath);

      // Close the dialog and open the target table: the new rows are there.
      const close = await $(`button*=${t('common.close')}`);
      if (await close.isDisplayed()) await close.click();
      await browser.pause(400);
      await clickTableInSidebar(IMPORT_TABLE);
      await browser.pause(2000);
      const body = await $('body').getText();
      expect(body).toContain('CommittedRowOne');
      expect(body).toContain('CommittedRowTwo');
    } finally {
      await dropImportTargetTable().catch(() => undefined);
    }
  });

  it('EI-IMP-003: 未闭合引号在写库前被拒绝 (DBR-01)', async () => {
    await seedImportTargetTable();
    try {
      await openImportWizardOnTable();

      await resetDialogQueue();
      const csvPath = path.join(os.tmpdir(), `datazen-import-bad-${Date.now()}.csv`);
      // One quoted field that never closes: the parser must refuse the file,
      // not guess a row split.
      fs.writeFileSync(csvPath, 'name,status\n"never closes,active\n', 'utf-8');
      await injectDialogPath(csvPath);
      const pickButton = await $(`button*=${t('import.selectFile')}`);
      await pickButton.waitForClickable({ timeout: 8000 });
      await pickButton.click();
      await browser.pause(1200);

      // The refusal is visible (the static prefix of the parse-failed
      // message) and no mapping table ever renders.
      const parseFailedPrefix = t('import.parseFailed', { message: 'SENTINEL' }).split(
        'SENTINEL',
      )[0];
      const body = await $('body').getText();
      expect(body).toContain(parseFailedPrefix);
      const mapping = await $('[data-testid="import-mapping-table"]');
      expect(await mapping.isExisting()).toBe(false);
      fs.unlinkSync(csvPath);

      const cancel = await $(`button*=${t('common.cancel')}`);
      if (await cancel.isDisplayed()) await cancel.click();
    } finally {
      await dropImportTargetTable().catch(() => undefined);
    }
  });

  // ── 导出列选择、格式门槛与取消入口（DBR-06）──────────────

  /**
   * Open the export dialog's Nth combobox (0 = format, 1 = range) and pick the
   * option whose label contains `optionText`. The real combobox is a custom
   * listbox, so both steps go through DOM dispatch.
   */
  async function pickExportCombobox(index: number, optionText: string) {
    await browser.execute((i: number) => {
      const dlg = document.querySelector('[data-testid="data-export-dialog"]');
      if (!dlg) return;
      const triggers = dlg.querySelectorAll('button[aria-haspopup="listbox"]');
      (triggers[i] as HTMLElement | undefined)?.click();
    }, index);
    await browser.pause(300);
    await browser.execute((text: string) => {
      const listbox = document.querySelector('[id^="dz-select-listbox-"]');
      if (!listbox) return;
      const opts = listbox.querySelectorAll('div[tabindex]');
      for (const opt of opts) {
        if (opt.textContent?.includes(text)) {
          opt.dispatchEvent(new MouseEvent('mousedown', { bubbles: true }));
          break;
        }
      }
    }, optionText);
    await browser.pause(300);
  }

  /** Whether the dialog's primary export button is disabled. */
  async function exportButtonDisabled(): Promise<boolean> {
    return browser.execute((label: string) => {
      const dlg = document.querySelector('[data-testid="data-export-dialog"]');
      const btn = Array.from(dlg?.querySelectorAll('button') ?? []).find(
        (b) => b.textContent?.trim() === label,
      );
      return btn?.hasAttribute('disabled') ?? false;
    }, t('export.export'));
  }

  it('EI-DBR06-001: 清空全部列应禁用导出，恢复默认应恢复 (DBR-06)', async () => {
    await openTableExportDialog();
    await browser.execute(() => {
      (document.querySelector('[data-testid="export-columns-clear-all"]') as HTMLElement)?.click();
    });
    await browser.pause(300);
    expect(await exportButtonDisabled()).toBe(true);

    await browser.execute(() => {
      (
        document.querySelector('[data-testid="export-columns-reset-default"]') as HTMLElement
      )?.click();
    });
    await browser.pause(300);
    expect(await exportButtonDisabled()).toBe(false);
    await closeDataExportDialogIfOpen();
  });

  it('EI-DBR06-002: 网格导出只写勾选列且遵循显示顺序 (DBR-06)', async () => {
    await openTableExportDialog();
    // Keep only `name` and `status`; the export must drop `id` from the file.
    await browser.execute(() => {
      (document.querySelector('[data-testid="export-column-id"]') as HTMLElement)?.click();
    });
    await browser.pause(300);

    await resetDialogQueue();
    const outPath = path.join(os.tmpdir(), `datazen-export-cols-${Date.now()}.csv`);
    try {
      await injectDialogPath(outPath);
      const dlg = await $('[data-testid="data-export-dialog"]');
      const exportConfirm = await dlg.$(`button*=${t('export.export')}`);
      await exportConfirm.waitForClickable({ timeout: 5000 });
      await exportConfirm.click();
      await browser.waitUntil(() => fs.existsSync(outPath), {
        timeout: 20000,
        timeoutMsg: 'Timed out waiting for column-subset export file',
      });
      const content = fs.readFileSync(outPath, 'utf-8');
      expect(content.trim().split('\n')[0]).toBe('name,status');
      expect(content).toContain('Widget,active');
      expect(content).not.toMatch(/^id,/m);
      await captureJourneyStep('export-csv-column-subset', 0, true);
    } finally {
      try {
        fs.unlinkSync(outPath);
      } catch {
        /* ok */
      }
      await closeDataExportDialogIfOpen();
    }
  });

  it('EI-DBR06-003: 整表范围禁用 XLSX 并解释原因 (DBR-06)', async () => {
    await openTableExportDialog();
    // Switch the range to the streaming "entire table" scope.
    await pickExportCombobox(1, t('export.entireTable'));
    const body = await $('body').getText();
    // The reason stays on screen while the range makes XLSX unavailable.
    expect(body).toContain(t('export.formatDisabledStream'));
    // 禁用而非隐藏：the greyed-out format is still listed.
    expect(body).toContain('Excel (XLSX)');
    // A streaming scope exposes the run-cancel entry (disabled until a run).
    expect(body).toContain(t('export.cancelExport'));
    await closeDataExportDialogIfOpen();
  });
});
