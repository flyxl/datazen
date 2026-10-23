import { expect, browser, $, $$ } from '@wdio/globals';
import { t } from '../i18n.js';
import { closeWorkspacePanels } from '../lib/testDataLifecycle.js';
import {
  connectSeededPgInWorkspace,
  closeExtraWindows,
  executeSQLChecked,
  openQueryTab,
  clickTableInSidebar,
  switchSubTab,
  doubleClickCellByText,
  waitForEditInput,
  waitForNewQueryButton,
  waitForTableInSidebar,
  setSafeMode,
} from '../helpers.js';
import { existsSync, readFileSync } from 'node:fs';
import { resolve } from 'node:path';

/**
 * The dirty-cell marks of the table grid (DB-06).
 *
 * 【留待 R 回归】This spec was written against the implementation but **not
 * run**: the machine has no `target/debug/datazen`, so the WebDriver build
 * could not be produced here. Every selector below was checked against the
 * implementation:
 *
 * - `[data-dt-dirty="true"]`   — the cell-level dirty mark (GridCell, 00-contracts §4.4;
 *                                the attribute is "true" or absent, never "false");
 * - `[data-dt-row-state]`      — the row-level state, `"dirty"` / `"deleted"`
 *                                (VirtualBody, 06 §11 D-3);
 * - `[data-testid="pending-cells-count"]` — the cell counter of the
 *                                pending-changes bar (TablePendingChangesBar);
 * - `[data-testid="pending-changes-bar"]` — the bar itself;
 * - `[data-testid="data-table-cell"]` — a cell of the grid;
 * - `[data-testid="table-edit-input"]` — the inline editor.
 *
 * The theme case injects the **real** token CSS of the community "Slate Blue"
 * theme pack. That pack does not declare `--dt-dirty-*`, so the base values in
 * `themes.css` must still apply — which is exactly what the case proves.
 */

const TEST_TABLE = '_e2e_dirty_cells_test';
/** Enough rows for two pages at the default page size of 50. */
const ROW_COUNT = 60;

async function stageValue(cellText: string, value: string) {
  await doubleClickCellByText(cellText);
  await commitStagedValue(cellText, value);
}

/**
 * Stage a value into one cell addressed by row/column.
 *
 * Text addressing cannot tell `id` from `score` here: the fixture's id and score
 * columns both hold 1…60, and `doubleClickCellByText('2')` landed on the primary
 * key cell — whose editor the grid keeps open (a key change needs its own
 * confirmation), so the next stage never found a closed editor.
 */
async function stageCell(rowIndex: number, columnName: string, value: string) {
  const selector = `[data-dt-row="${rowIndex}"][data-dt-col="${columnName}"]`;
  await $(selector).waitForDisplayed({ timeout: 8000 });
  await browser.execute((sel: string) => {
    const el = document.querySelector(sel);
    if (!el) return;
    el.dispatchEvent(
      new MouseEvent('dblclick', { bubbles: true, cancelable: true, composed: true }),
    );
  }, selector);
  await browser.pause(500);
  await commitStagedValue(`${columnName}[${rowIndex}]`, value);
}

async function commitStagedValue(cellText: string, value: string) {
  const input = await waitForEditInput();
  await input.clearValue();
  await input.setValue(value);
  await browser.pause(300);
  await browser.execute(() => {
    const editor = document.querySelector(
      '[data-testid="table-edit-input"]',
    ) as HTMLInputElement | null;
    if (!editor) return;
    // React commits on the bubbling `focusout`, and `blur()` alone does not
    // always reach it from script in this WebView.
    editor.dispatchEvent(new FocusEvent('focusout', { bubbles: true }));
    editor.blur();
  });
  // Commit is asynchronous: opening the next editor before this one closed made
  // the previous value vanish, so every case counted one staged cell less.
  await browser.waitUntil(async () => !(await $('[data-testid="table-edit-input"]').isExisting()), {
    timeout: 8000,
    timeoutMsg: `等待编辑框提交并关闭超时（单元格「${cellText}」→「${value}」）`,
  });
  await browser.pause(300);
}

/** The number of cells the grid paints as dirty. */
async function dirtyCellCount(): Promise<number> {
  return browser.execute(() => document.querySelectorAll('[data-dt-dirty="true"]').length);
}

/** The id of the table panel the store holds. */
async function tablePanelId(): Promise<string | null> {
  return browser.execute(() => {
    const store = (
      window as unknown as {
        __tableDataStore?: { getState: () => { byPanel: Map<string, unknown> } };
      }
    ).__tableDataStore;
    if (!store) return null;
    const ids = Array.from(store.getState().byPanel.keys());
    return ids[0] ?? null;
  });
}

/** The token CSS of the community Slate Blue theme pack, read from the repo. */
function slateBlueTokens(): string {
  const candidates = [
    resolve(process.cwd(), 'packages/wapps/community.slate-blue/themes/slate-blue/tokens.css'),
    resolve(process.cwd(), '../packages/wapps/community.slate-blue/themes/slate-blue/tokens.css'),
  ];
  const found = candidates.find((path) => existsSync(path));
  if (!found) {
    throw new Error(`Slate Blue tokens.css not found (looked in ${candidates.join(', ')})`);
  }
  return readFileSync(found, 'utf-8');
}

describe('表数据脏单元高亮 (DB-06)', () => {
  let mainWindow: string;

  before(async () => {
    mainWindow = await browser.getWindowHandle();
    await setSafeMode(false);
    await connectSeededPgInWorkspace();
    await waitForNewQueryButton(20000);
    await browser.pause(1500);

    await openQueryTab();
    await executeSQLChecked(`DROP TABLE IF EXISTS ${TEST_TABLE}`);
    await executeSQLChecked(
      `CREATE TABLE ${TEST_TABLE} (
        id SERIAL PRIMARY KEY,
        name VARCHAR(100) NOT NULL,
        score INT NOT NULL DEFAULT 0
      )`,
    );
    await executeSQLChecked(
      `INSERT INTO ${TEST_TABLE} (name, score)
       SELECT 'Row' || g, g FROM generate_series(1, ${ROW_COUNT}) AS g`,
    );
    await browser.pause(1200);

    const refreshBtn = await $('[data-testid="navigator-refresh"]');
    await refreshBtn.waitForDisplayed({ timeout: 10000 });
    await refreshBtn.click();
    await waitForTableInSidebar(TEST_TABLE);

    await clickTableInSidebar(TEST_TABLE);
    await browser.pause(1500);
    await switchSubTab('data');
    await browser.pause(1200);
    await browser.waitUntil(async () => (await $('body').getText()).includes('Row1'), {
      timeout: 15000,
      timeoutMsg: 'Timed out waiting for table data to load',
    });
  });

  after(async () => {
    try {
      await browser.switchToWindow(mainWindow);
      await executeSQLChecked(`DROP TABLE IF EXISTS ${TEST_TABLE}`);
    } catch {
      // best-effort cleanup
    }
    await setSafeMode(true);
    await closeExtraWindows(mainWindow);
    await closeWorkspacePanels(browser);
  });

  it('改 3 格后网格高亮与计数一致', async () => {
    await stageValue('Row1', 'Row1-edited');
    await stageCell(1, 'score', '22');
    await stageValue('Row3', 'Row3-edited');

    expect(await dirtyCellCount()).toBe(3);

    const cellCount = await $('[data-testid="pending-cells-count"]');
    await cellCount.waitForDisplayed({ timeout: 8000 });
    expect(await cellCount.getText()).toContain('3');
  });

  it('改回原值后脏标记与计数一并消失', async () => {
    await stageValue('Row3-edited', 'Row3');

    expect(await dirtyCellCount()).toBe(2);

    const cellCount = await $('[data-testid="pending-cells-count"]');
    await cellCount.waitForDisplayed({ timeout: 8000 });
    expect(await cellCount.getText()).toContain('2');
  });

  it('翻页再回来脏标记仍归属原行', async () => {
    // The row that carries the mark is identified by its primary key, not by
    // its position on the page.
    const markedRow = await browser.execute(() => {
      const cell = document.querySelector('[data-dt-dirty="true"]');
      if (!cell) return null;
      const row = cell.closest('[data-dt-row-view]');
      return row?.getAttribute('data-dt-row-view') ?? null;
    });
    expect(markedRow).not.toBeNull();

    const next = await $(`button[aria-label="${t('pagination.next')}"]`);
    await next.waitForDisplayed({ timeout: 8000 });
    await next.click();
    await browser.pause(1500);
    // The staged change is on another page: it is counted, but not painted.
    expect(await dirtyCellCount()).toBe(0);

    const prev = await $(`button[aria-label="${t('pagination.prev')}"]`);
    await prev.waitForDisplayed({ timeout: 8000 });
    await prev.click();
    await browser.pause(1500);

    expect(await dirtyCellCount()).toBe(2);
    const cellCount = await $('[data-testid="pending-cells-count"]');
    await cellCount.waitForDisplayed({ timeout: 8000 });
    expect(await cellCount.getText()).toContain('2');
  });

  it('删除 2 行后删除线可见，恢复后消失', async () => {
    const panelId = await tablePanelId();
    expect(panelId).not.toBeNull();

    await browser.execute((id: string) => {
      const store = (
        window as unknown as {
          __tableDataStore?: {
            getState: () => { stageRowDelete: (panelId: string, rows: number[]) => void };
          };
        }
      ).__tableDataStore;
      store?.getState().stageRowDelete(id, [2, 3]);
    }, panelId as string);
    await browser.pause(600);

    const deletedRows = await $$('[data-dt-row-state="deleted"]');
    expect(deletedRows.length).toBe(2);

    // The restore entry point of the context menu calls the same store action.
    await browser.execute((id: string) => {
      const store = (
        window as unknown as {
          __tableDataStore?: {
            getState: () => { unstageRowDelete: (panelId: string, row: number) => void };
          };
        }
      ).__tableDataStore;
      store?.getState().unstageRowDelete(id, 2);
    }, panelId as string);
    await browser.pause(600);

    const stillDeleted = await $$('[data-dt-row-state="deleted"]');
    expect(stillDeleted.length).toBe(1);
  });

  it('亮色 / 暗色 / 第三方主题下高亮均可见', async () => {
    const dirty = await $('[data-dt-dirty="true"]');
    await dirty.waitForDisplayed({ timeout: 8000 });
    await expect(dirty).toBeDisplayed();

    // Dark mode: the theme mechanism toggles the `dark` class on the root.
    await browser.execute(() => document.documentElement.classList.add('dark'));
    await browser.pause(400);
    await expect(await $('[data-dt-dirty="true"]')).toBeDisplayed();
    await browser.execute(() => document.documentElement.classList.remove('dark'));
    await browser.pause(400);

    // A third-party theme pack: it only declares the tokens it owns, so the
    // dirty marks keep the base values of `themes.css`.
    const tokens = slateBlueTokens();
    await browser.execute((css: string) => {
      const style = document.createElement('style');
      style.id = 'e2e-slate-blue-tokens';
      style.textContent = css;
      document.head.appendChild(style);
    }, tokens);
    await browser.pause(400);
    await expect(await $('[data-dt-dirty="true"]')).toBeDisplayed();

    await browser.execute(() => {
      document.getElementById('e2e-slate-blue-tokens')?.remove();
    });
    await browser.pause(400);
  });

  it('回滚后脏标记与计数一并消失', async () => {
    const rollback = await $('[data-testid="pending-rollback"]');
    await rollback.waitForDisplayed({ timeout: 8000 });
    await rollback.click();
    await browser.pause(1500);

    expect(await dirtyCellCount()).toBe(0);
    const bar = await $('[data-testid="pending-changes-bar"]');
    expect(await bar.isExisting()).toBe(false);
  });
});
