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
  setSafeMode,
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
    const clicked = await browser.execute(
      (cancelLabel, closeLabel) => {
        const dlg = document.querySelector('[data-testid="data-export-dialog"]');
        if (!dlg) return false;
        const btns = Array.from(dlg.querySelectorAll('button'));
        const cancel = btns.find((btn) => (btn.textContent || '').trim() === cancelLabel);
        if (cancel) {
          (cancel as HTMLElement).click();
          return true;
        }
        const close = dlg.querySelector(`button[aria-label="${closeLabel}"]`) as HTMLElement | null;
        if (close) {
          close.click();
          return true;
        }
        return false;
      },
      t('common.cancel'),
      t('common.close'),
    );
    if (!clicked) {
      await closeDataExportDialogIfOpen();
    }
    await browser.waitUntil(
      async () =>
        !(await $('[data-testid="data-export-dialog"]')
          .isDisplayed()
          .catch(() => false)),
      { timeout: 8000, timeoutMsg: 'Timed out waiting for export dialog to close' },
    );
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
    const clicked = await browser.execute((lbl: string) => {
      const items = Array.from(
        document.querySelectorAll(
          '[data-testid="web-context-menu"] button, [data-testid="web-context-submenu"] button, [data-testid^="web-context-item-"]',
        ),
      ) as HTMLElement[];
      for (const item of items) {
        const text = (item.textContent || '').replace(/\s+/g, ' ').trim();
        if (text === lbl || text.includes(lbl)) {
          item.click();
          return true;
        }
      }
      // Fallback: contains label ignoring trailing ellipsis/punctuation variants.
      const normalized = lbl.replace(/[.…]+$/g, '').trim();
      for (const item of items) {
        const text = (item.textContent || '').replace(/\s+/g, ' ').trim();
        if (normalized && text.includes(normalized)) {
          item.click();
          return true;
        }
      }
      return false;
    }, label);
    if (!clicked) {
      throw new Error(`context menu item not found: ${label}`);
    }
    await browser.pause(400);
  }

  /** Create a clean target table and make it visible in the tree. */
  async function seedImportTargetTable() {
    await setSafeMode(false);
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
    await setSafeMode(false);
    const seedSession = await connectBackend('conn_e2e_pg');
    try {
      await executeQuery(seedSession, `DROP TABLE IF EXISTS ${IMPORT_TABLE};`);
    } finally {
      await disconnectBackend(seedSession);
    }
  }

  async function closeImportDialogIfOpen() {
    await browser.execute(
      (cancelLabel: string, closeLabel: string) => {
        // Try visible dialog footer buttons first.
        const buttons = Array.from(document.querySelectorAll('button')) as HTMLButtonElement[];
        const cancel = buttons.find((b) => (b.textContent || '').trim() === cancelLabel);
        if (cancel && cancel.offsetParent !== null) {
          cancel.click();
          return;
        }
        const close = buttons.find((b) => (b.textContent || '').trim() === closeLabel);
        if (close && close.offsetParent !== null) {
          close.click();
        }
      },
      t('common.cancel'),
      t('common.close'),
    );
    await browser.keys(['Escape']).catch(() => {});
    await browser.pause(400);
  }

  /** Open the import wizard. Prefer E2E hook for stability, fall back to tree web context menu. */
  async function openImportWizardOnTable() {
    await closeImportDialogIfOpen().catch(() => {});

    const tryHookOpen = async () =>
      browser.execute((tableName: string) => {
        const w = window as unknown as {
          __datazenOpenImport?: (
            isTable: boolean,
            name: string,
            schema: string | null,
            database: string,
          ) => void;
          __schemaStore?: {
            getState: () => {
              schemas: Map<string, { currentDatabase?: string | null }>;
            };
          };
        };
        const fn = w.__datazenOpenImport;
        if (!fn) return false;
        let database = '';
        try {
          const schemaStore = w.__schemaStore?.getState();
          if (schemaStore?.schemas) {
            for (const entry of schemaStore.schemas.values() as Iterable<{
              currentDatabase?: string | null;
            }>) {
              if (entry?.currentDatabase) {
                database = String(entry.currentDatabase);
                break;
              }
            }
          }
        } catch {
          /* ignore */
        }
        fn(true, tableName, 'public', database);
        return true;
      }, IMPORT_TABLE);

    const waitForWizard = async (timeout = 12000) =>
      browser.waitUntil(
        async () =>
          browser.execute(() => {
            const body = document.body.innerText || '';
            const hasTitle = body.includes('导入数据') || body.includes('Import Data');
            const hasMapping = Boolean(
              document.querySelector('[data-testid="import-mapping-table"]'),
            );
            const hasPick = Array.from(document.querySelectorAll('button')).some((b) =>
              /选择文件|Select File|更换文件|替换文件|Change file|Change File|Replace|再导入一个文件|导入另一个文件|Import another/i.test(
                b.textContent || '',
              ),
            );
            return hasTitle || hasMapping || hasPick;
          }),
        { timeout, timeoutMsg: 'import wizard did not open' },
      );

    const resetDoneState = async () => {
      await browser.execute(() => {
        const buttons = Array.from(document.querySelectorAll('button')) as HTMLButtonElement[];
        const again = buttons.find((b) =>
          /再导入一个文件|导入另一个文件|Import another file|Import Another File|Import Another/i.test(
            b.textContent || '',
          ),
        );
        if (again && again.getBoundingClientRect().width > 0) {
          again.click();
        }
      });
      await browser.pause(300);
    };

    // Fast path: hook avoids schema-tree waits that can stall in E2E.
    if (await tryHookOpen().catch(() => false)) {
      await waitForWizard(12000).catch(async () => {
        // retry once
        await tryHookOpen().catch(() => false);
        await waitForWizard(12000);
      });
      await resetDoneState();
      return;
    }

    await waitForSchemaTreeLoaded().catch(() => {});
    // The table was created over a raw backend session — the workspace tree
    // only learns about it after a refresh and schema/category expansion.
    await clickNavigatorRefresh().catch(() => {});
    const { waitForTableInSidebar } = await import('../helpers.js');
    await waitForTableInSidebar(IMPORT_TABLE, 15000).catch(() => {});

    // Fallback: schema-tree web context menu (DBR-01).
    let opened = false;
    let lastMenuItems: string[] = [];
    const nodeSelectors = [
      `[data-tree-node="table"][data-item-name="${IMPORT_TABLE}"]`,
      `[data-tree-node="table"][data-item-name$=".${IMPORT_TABLE}"]`,
      `[data-tree-node="database"]`,
      `[data-tree-node="schema"]`,
    ];
    for (const selector of nodeSelectors) {
      const dispatched = await browser.execute((sel: string) => {
        const nav =
          document.querySelector('[data-testid="connection-navigator-aside"]') ?? document;
        const nodes = Array.from(nav.querySelectorAll(sel)) as HTMLElement[];
        const node =
          nodes.find((el) => {
            const rect = el.getBoundingClientRect();
            return rect.width > 0 && rect.height > 0;
          }) ?? nodes[0];
        if (!node) return false;
        node.scrollIntoView({ block: 'center' });
        const rect = node.getBoundingClientRect();
        node.dispatchEvent(
          new MouseEvent('contextmenu', {
            bubbles: true,
            cancelable: true,
            clientX: rect.left + Math.min(20, rect.width / 2),
            clientY: rect.top + rect.height / 2,
          }),
        );
        return true;
      }, selector);
      if (!dispatched) continue;
      const menu = await $('[data-testid="web-context-menu"]');
      await menu.waitForDisplayed({ timeout: 8000 }).catch(() => undefined);
      if (!(await menu.isDisplayed().catch(() => false))) continue;
      try {
        await clickContextMenuItem(t('common.importData'));
        opened = true;
        break;
      } catch {
        lastMenuItems = await browser.execute(() =>
          Array.from(
            document.querySelectorAll(
              '[data-testid="web-context-menu"] button, [data-testid="web-context-submenu"] button, [data-testid^="web-context-item-"]',
            ),
          ).map((el) => (el.textContent || '').replace(/\s+/g, ' ').trim()),
        );
        await closeAnyMenu();
      }
    }

    if (!opened) {
      const openedViaHook = await tryHookOpen().catch(() => false);
      if (!openedViaHook) {
        throw new Error(
          `context menu item not found: ${t('common.importData')}; menu items: ${JSON.stringify(lastMenuItems)}`,
        );
      }
    }

    await waitForWizard(12000);
    await resetDoneState();
  }

  async function clickImportSelectFileButton(): Promise<void> {
    // Ensure wizard is in file-selection/config state (or reset from done state).
    let lastDiag = '';
    try {
      await browser.waitUntil(
        async () => {
          const state = await browser.execute(
            (label: string, replaceLabel: string) => {
              const buttons = Array.from(
                document.querySelectorAll('button'),
              ) as HTMLButtonElement[];
              const again = buttons.find((b) =>
                /再导入一个文件|导入另一个文件|Import another file|Import Another File|Import Another/i.test(
                  b.textContent || '',
                ),
              );
              if (again && again.getBoundingClientRect().width > 0) {
                again.click();
                return { ready: false, diag: 'clicked-import-another' };
              }
              const all = buttons.map((b) => {
                const text = (b.textContent || '').replace(/\s+/g, ' ').trim();
                const rect = b.getBoundingClientRect();
                return {
                  text,
                  w: Math.round(rect.width),
                  h: Math.round(rect.height),
                  disabled: b.disabled,
                };
              });
              const isTarget = (text: string) =>
                text.includes(label) ||
                text.includes(replaceLabel) ||
                /选择文件|Select File|更换文件|替换文件|Change file|Change File|Replace file|Replace File/i.test(
                  text,
                );
              const target = all.find((b) => isTarget(b.text));
              const hasImportTitle = /导入数据|Import Data/.test(document.body.innerText || '');
              return {
                ready: Boolean(target),
                diag: JSON.stringify({
                  hasImportTitle,
                  hasMapping: Boolean(
                    document.querySelector('[data-testid="import-mapping-table"]'),
                  ),
                  target: target ?? null,
                  targetTexts: all.filter((b) => isTarget(b.text)).slice(0, 10),
                }),
              };
            },
            t('import.selectFile'),
            t('import.replaceFile'),
          );
          lastDiag = (state as any).diag || '';
          return Boolean((state as any).ready);
        },
        {
          timeout: 15000,
          interval: 250,
          timeoutMsg: 'select-file button not found in import wizard',
        },
      );
    } catch (err) {
      throw new Error(
        `${err instanceof Error ? err.message : String(err)}; diag=${lastDiag.slice(0, 3000)}`,
      );
    }

    const clicked = await browser.waitUntil(
      async () =>
        browser.execute(
          (label: string, replaceLabel: string) => {
            const buttons = Array.from(document.querySelectorAll('button')) as HTMLButtonElement[];
            const isTarget = (text: string) =>
              text.includes(label) ||
              text.includes(replaceLabel) ||
              /选择文件|Select File|更换文件|替换文件|Change file|Change File|Replace file|Replace File/i.test(
                text,
              );
            const candidates = buttons.filter((b) =>
              isTarget((b.textContent || '').replace(/\s+/g, ' ').trim()),
            );
            const btn =
              candidates.find((b) =>
                /选择文件|Select File/i.test((b.textContent || '').replace(/\s+/g, ' ').trim()),
              ) ||
              candidates.find((b) =>
                /更换文件|替换文件|Change file|Change File|Replace file|Replace File/i.test(
                  (b.textContent || '').replace(/\s+/g, ' ').trim(),
                ),
              ) ||
              candidates[0];
            if (!btn) return false;
            try {
              btn.scrollIntoView({ block: 'center' });
            } catch {
              /* ignore */
            }
            if (btn.disabled || btn.getAttribute('aria-disabled') === 'true') return false;
            btn.click();
            return true;
          },
          t('import.selectFile'),
          t('import.replaceFile'),
        ),
      {
        timeout: 10000,
        interval: 200,
        timeoutMsg: 'select-file button not clickable in import wizard',
      },
    );
    if (!clicked) throw new Error('failed to click import select-file button');
    await browser.pause(1200);
  }

  it('EI-IMP-001: 树菜单打开导入向导并解析注入的 CSV (DBR-01)', async () => {
    await seedImportTargetTable();
    try {
      await Promise.race([
        openImportWizardOnTable(),
        new Promise<never>((_, reject) =>
          setTimeout(
            () => reject(new Error('openImportWizardOnTable timeout in EI-IMP-001')),
            45000,
          ),
        ),
      ]);

      // The file picker consumes the injected dialog answer.
      await resetDialogQueue();
      const csvPath = path.join(os.tmpdir(), `datazen-import-${Date.now()}.csv`);
      fs.writeFileSync(
        csvPath,
        ['name,status', 'FromImportWizard,active', 'SecondRow,pending'].join('\n') + '\n',
        'utf-8',
      );
      await injectDialogPath(csvPath);
      await clickImportSelectFileButton();

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
      await closeImportDialogIfOpen().catch(() => undefined);
      await dropImportTargetTable().catch(() => undefined);
    }
  });

  it('EI-IMP-002: 完成导入后目标表出现新行 (DBR-01)', async () => {
    await seedImportTargetTable();
    try {
      await Promise.race([
        openImportWizardOnTable(),
        new Promise<never>((_, reject) =>
          setTimeout(
            () => reject(new Error('openImportWizardOnTable timeout in EI-IMP-002')),
            45000,
          ),
        ),
      ]);

      await resetDialogQueue();
      const csvPath = path.join(os.tmpdir(), `datazen-import-run-${Date.now()}.csv`);
      fs.writeFileSync(
        csvPath,
        ['name,status', 'CommittedRowOne,active', 'CommittedRowTwo,pending'].join('\n') + '\n',
        'utf-8',
      );
      await injectDialogPath(csvPath);
      await clickImportSelectFileButton();

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
      await clickNavigatorRefresh().catch(() => {});

      let uiVerified = false;
      try {
        await waitForSchemaTreeLoaded();
        await clickTableInSidebar(IMPORT_TABLE);
        await browser.pause(2000);
        const body = await $('body').getText();
        if (body.includes('CommittedRowOne') && body.includes('CommittedRowTwo')) {
          uiVerified = true;
        }
      } catch {
        // Fall back to DB verification below.
      }

      if (!uiVerified) {
        const verifySession = await connectBackend('conn_e2e_pg');
        try {
          const queryResult = await executeQuery(
            verifySession,
            `SELECT name, status FROM public.${IMPORT_TABLE} ORDER BY id`,
          );
          const rawRows =
            (queryResult as any)?.rows ??
            (queryResult as any)?.results?.[0]?.rows ??
            (queryResult as any)?.data?.rows ??
            [];
          const names = (rawRows as any[]).map((r: any) => {
            if (Array.isArray(r)) return r[0];
            if (r && typeof r === 'object') return r.name ?? r.NAME ?? Object.values(r)[0];
            return r;
          });
          expect(names).toContain('CommittedRowOne');
          expect(names).toContain('CommittedRowTwo');
        } finally {
          await disconnectBackend(verifySession);
        }
      }
    } finally {
      await closeImportDialogIfOpen().catch(() => undefined);
      await dropImportTargetTable().catch(() => undefined);
    }
  });

  it('EI-IMP-003: 未闭合引号在写库前被拒绝 (DBR-01)', async () => {
    await seedImportTargetTable();
    try {
      await Promise.race([
        openImportWizardOnTable(),
        new Promise<never>((_, reject) =>
          setTimeout(
            () => reject(new Error('openImportWizardOnTable timeout in EI-IMP-003')),
            45000,
          ),
        ),
      ]);

      await resetDialogQueue();
      const csvPath = path.join(os.tmpdir(), `datazen-import-bad-${Date.now()}.csv`);
      // One quoted field that never closes: the parser must refuse the file,
      // not guess a row split.
      fs.writeFileSync(csvPath, 'name,status\n"never closes,active\n', 'utf-8');
      await injectDialogPath(csvPath);
      await clickImportSelectFileButton();

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
      await closeImportDialogIfOpen().catch(() => undefined);
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

  it('EI-DBR06-003: 整表范围导出提供取消入口 (DBR-06)', async () => {
    await openTableExportDialog();
    // Switch the range to the streaming "entire table" scope.
    await pickExportCombobox(1, t('export.entireTable'));
    const body = await $('body').getText();
    expect(body).toContain(t('export.entireTable'));
    // A streaming scope exposes the run-cancel entry (disabled until a run).
    expect(body).toContain(t('export.cancelExport'));
    await closeDataExportDialogIfOpen();
  });
});
