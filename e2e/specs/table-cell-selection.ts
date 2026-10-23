/**
 * DB-01 cell/region selection — TC-SEL-001 ~ TC-SEL-006.
 *
 * Depends on the DOM wiring that DB-01 Steps 7–9 add to `DataTable`:
 *   - `[data-dt-surface]`               (grid scroll container, keyboard host)
 *   - `[data-dt-col][data-dt-selected="true"]`  (per-cell membership — the row
 *     container carries `data-dt-selected` too since DB-02, so a *cell* locator
 *     has to name a cell; the symmetric row locator is
 *     `[data-dt-row-view][data-dt-selected]`)
 *   - `[data-dt-selection-rows|cols|cells]` + `[data-testid="data-table-selection-summary"]`
 *   - `[role="rowheader"] button`       (row gutter, contract §4.4b)
 * plus the store-owned `gridSelection` on `window.__tableDataStore` (landed in this track).
 *
 * The wiring exists now, so the suite no longer skips itself: a missing surface is a
 * failure, because a silently skipped suite would hide a broken selection contract.
 *
 * WKWebView's WebDriver has limited pointer-input support, so — following the convention in
 * `e2e/specs/table-edit.ts` — interactions dispatch `PointerEvent`s with real client
 * coordinates (the hook re-resolves the target through `document.elementFromPoint`) and
 * state is read back from both the DOM attributes and `window.__tableDataStore`.
 */
import { browser, expect } from '@wdio/globals';
import {
  captureJourneyStep,
  clickTableInSidebar,
  closeExtraWindows,
  connectSeededPgInWorkspace,
  executeSQL,
  executeSQLChecked,
  openQueryTab,
  setSafeMode,
  waitForTableInSidebar,
} from '../helpers.js';
import { t } from '../i18n.js';

const TEST_TABLE = 'e2e_grid_selection_test';
const FIRST_COL = 'id';

interface SelectionSnapshot {
  mode: 'none' | 'cell' | 'row';
  selectedRows: number;
  lastSelectedIndex: number | null;
  anchor: { rowIndex: number; columnName: string } | null;
  focus: { rowIndex: number; columnName: string } | null;
  extraRanges: number;
  selectedCells: number;
}

async function selectionSnapshot(): Promise<SelectionSnapshot | null> {
  return browser.execute(() => {
    type Slice = {
      rows: unknown[];
      selectedRows: Set<number>;
      lastSelectedIndex: number | null;
      gridSelection: {
        mode: 'none' | 'cell' | 'row';
        anchor: { rowIndex: number; columnName: string } | null;
        focus: { rowIndex: number; columnName: string } | null;
        extraRanges: unknown[];
      };
    };
    const store = (
      window as unknown as {
        __tableDataStore?: { getState: () => { byPanel: Map<string, Slice> } };
      }
    ).__tableDataStore;
    if (!store) return null;
    const slice = [...store.getState().byPanel.values()].find(
      (candidate) => candidate.rows.length > 0,
    );
    if (!slice) return null;
    return {
      mode: slice.gridSelection.mode,
      selectedRows: slice.selectedRows.size,
      lastSelectedIndex: slice.lastSelectedIndex,
      anchor: slice.gridSelection.anchor,
      focus: slice.gridSelection.focus,
      extraRanges: slice.gridSelection.extraRanges.length,
      // Cells only: DB-02 puts `data-dt-selected` on the row container as well,
      // so an unscoped count would add one per selected row to this cell count.
      selectedCells: document.querySelectorAll('[data-dt-col][data-dt-selected="true"]').length,
    };
  });
}

/** Dispatch a pointer event with the centre coordinates of `cellSelector`. */
async function pointerOn(
  cellSelector: string,
  type: 'pointerdown' | 'pointerup' | 'pointermove',
  modifiers: { shiftKey?: boolean; metaKey?: boolean } = {},
): Promise<void> {
  const ok = await browser.execute(
    (selector: string, eventType: string, mods: { shiftKey?: boolean; metaKey?: boolean }) => {
      const el = document.querySelector(selector) as HTMLElement | null;
      if (!el) return false;
      const rect = el.getBoundingClientRect();
      const clientX = rect.left + rect.width / 2;
      const clientY = rect.top + rect.height / 2;
      el.dispatchEvent(
        new PointerEvent(eventType, {
          bubbles: true,
          cancelable: true,
          composed: true,
          pointerId: 1,
          pointerType: 'mouse',
          isPrimary: true,
          button: 0,
          buttons: eventType === 'pointerup' ? 0 : 1,
          clientX,
          clientY,
          shiftKey: Boolean(mods.shiftKey),
          metaKey: Boolean(mods.metaKey),
          ctrlKey: Boolean(mods.metaKey),
        }),
      );
      return true;
    },
    cellSelector,
    type,
    modifiers,
  );
  expect(ok).toBe(true);
}

async function clickCell(rowIndex: number, columnName: string) {
  const selector = `[data-dt-row="${rowIndex}"][data-dt-col="${columnName}"]`;
  await pointerOn(selector, 'pointerdown');
  await pointerOn(selector, 'pointerup');
  await browser.pause(200);
}

async function clickNextPage(): Promise<boolean> {
  return browser.execute(() => {
    const next = Array.from(document.querySelectorAll('button')).find((button) => {
      const aria = button.getAttribute('aria-label') ?? '';
      const text = button.textContent ?? '';
      return (
        aria.includes('Next') ||
        aria.includes('下一页') ||
        text.includes('Next') ||
        text.includes('下一页')
      );
    });
    if (!next || next.disabled) return false;
    next.click();
    return true;
  });
}

describe('单元格与区域选择 (TC-SEL-001~006)', () => {
  let mainWindow: string;

  before(async function () {
    this.timeout(120000);
    mainWindow = await browser.getWindowHandle();
    await setSafeMode(false);
    await connectSeededPgInWorkspace();
    await openQueryTab();

    await executeSQLChecked(`DROP TABLE IF EXISTS ${TEST_TABLE}`);
    await executeSQLChecked(`
      CREATE TABLE ${TEST_TABLE} (
        id SERIAL PRIMARY KEY,
        code TEXT NOT NULL,
        name TEXT NOT NULL
      )
    `);
    await executeSQLChecked(`
      INSERT INTO ${TEST_TABLE} (code, name)
      -- 60 rows, not 30: the default page size is 50, so a 30-row fixture has no
      -- second page and TC-SEL-006's "turn the page" case could only ever fail.
      SELECT 'code_' || g, 'row_' || g FROM generate_series(1, 60) g
    `);
    await waitForTableInSidebar(TEST_TABLE, 30000);
    await clickTableInSidebar(TEST_TABLE);
    await browser.waitUntil(
      async () =>
        browser.execute(
          () => document.querySelectorAll('[data-testid="data-table-cell"]').length > 0,
        ),
      { timeout: 20000, timeoutMsg: `表 ${TEST_TABLE} 数据行未渲染` },
    );

    // The selection DOM contract is wired: a missing surface fails the suite instead of
    // skipping it, because a skipped suite would hide a broken selection contract.
    const surfaceWired = await browser.execute(() => !!document.querySelector('[data-dt-surface]'));
    expect(surfaceWired).toBe(true);
  });

  after(async () => {
    await openQueryTab();
    await setSafeMode(false);
    await executeSQL(`DROP TABLE IF EXISTS ${TEST_TABLE}`);
    await closeExtraWindows(mainWindow);
  });

  it('TC-SEL-001: 单击单元格应进入单元格选择', async function () {
    await clickCell(0, FIRST_COL);
    const snapshot = await selectionSnapshot();
    expect(snapshot).not.toBeNull();
    expect(snapshot?.mode).toBe('cell');
    expect(snapshot?.selectedRows).toBe(0);
    expect(snapshot?.selectedCells).toBe(1);
    expect(snapshot?.anchor).toEqual({ rowIndex: 0, columnName: FIRST_COL });
    await captureJourneyStep('table-cell-selection-001');
  });

  it('TC-SEL-002: 拖拽应产生矩形区域并更新摘要', async function () {
    const start = `[data-dt-row="2"][data-dt-col="id"]`;
    const end = `[data-dt-row="5"][data-dt-col="name"]`;
    await pointerOn(start, 'pointerdown');
    await pointerOn(end, 'pointermove');
    await pointerOn(end, 'pointerup');
    await browser.pause(200);

    const summary = await browser.execute(() => {
      const el = document.querySelector('[data-testid="data-table-selection-summary"]');
      return el
        ? {
            rows: el.getAttribute('data-dt-selection-rows'),
            cols: el.getAttribute('data-dt-selection-cols'),
            cells: el.getAttribute('data-dt-selection-cells'),
            text: el.textContent ?? '',
          }
        : null;
    });
    expect(summary).not.toBeNull();
    expect(summary?.rows).toBe('4');
    expect(summary?.cols).toBe('3');
    expect(summary?.cells).toBe('12');
    expect(summary?.text).toContain(t('dataTable.selectionSummary', { rows: 4, cols: 3 }));
    const snapshot = await selectionSnapshot();
    expect(snapshot?.anchor).toEqual({ rowIndex: 2, columnName: 'id' });
    expect(snapshot?.focus).toEqual({ rowIndex: 5, columnName: 'name' });
    await captureJourneyStep('table-cell-selection-002');
  });

  it('TC-SEL-003: Shift+单击应从锚点扩区', async function () {
    await clickCell(0, 'name');
    const anchorSelector = `[data-dt-row="3"][data-dt-col="name"]`;
    await pointerOn(anchorSelector, 'pointerdown', { shiftKey: true });
    await pointerOn(anchorSelector, 'pointerup', { shiftKey: true });
    await browser.pause(200);

    const snapshot = await selectionSnapshot();
    expect(snapshot?.mode).toBe('cell');
    expect(snapshot?.anchor).toEqual({ rowIndex: 0, columnName: 'name' });
    expect(snapshot?.focus).toEqual({ rowIndex: 3, columnName: 'name' });
    expect(snapshot?.selectedCells).toBe(4);
    await captureJourneyStep('table-cell-selection-003');
  });

  it('TC-SEL-004: Escape 应清空选区', async function () {
    await clickCell(2, FIRST_COL);
    expect((await selectionSnapshot())?.mode).toBe('cell');

    await browser.execute(() => {
      document
        .querySelector('[data-dt-surface]')
        ?.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }));
    });
    await browser.pause(200);

    const snapshot = await selectionSnapshot();
    expect(snapshot?.mode).toBe('none');
    expect(snapshot?.selectedCells).toBe(0);
    // Deliberately unscoped: after Escape *nothing* may still read as selected —
    // neither a cell nor a row container (DB-02). Both attributes are omitted
    // rather than set to "false", so zero is the only correct count.
    expect(
      await browser.execute(() => document.querySelectorAll('[data-dt-selected]').length),
    ).toBe(0);
    await captureJourneyStep('table-cell-selection-004');
  });

  it('TC-SEL-005: 行号槽仍走整行选择（回归）', async function () {
    const clicked = await browser.execute(() => {
      const gutters = document.querySelectorAll('[role="rowheader"] button');
      if (gutters.length === 0) return false;
      (gutters[0] as HTMLButtonElement).click();
      return true;
    });
    expect(clicked).toBe(true);
    await browser.pause(200);

    const snapshot = await selectionSnapshot();
    expect(snapshot?.mode).toBe('row');
    expect(snapshot?.selectedRows).toBe(1);
    expect(snapshot?.anchor).toBeNull();
    const gutters = await browser.execute(() => ({
      headers: document.querySelectorAll('[role="rowheader"]').length,
      rows: new Set(
        Array.from(document.querySelectorAll('[data-dt-row]')).map((el) =>
          el.getAttribute('data-dt-row'),
        ),
      ).size,
    }));
    expect(gutters.headers).toBe(gutters.rows);
    await captureJourneyStep('table-cell-selection-005');
  });

  it('TC-SEL-006: 翻页后选区不残留', async function () {
    await clickCell(0, FIRST_COL);
    expect((await selectionSnapshot())?.mode).toBe('cell');

    const paged = await clickNextPage();
    expect(paged).toBe(true);
    await browser.pause(1200);

    const snapshot = await selectionSnapshot();
    expect(snapshot?.mode).toBe('none');
    expect(snapshot?.selectedRows).toBe(0);
    expect(snapshot?.lastSelectedIndex).toBeNull();
    expect(snapshot?.selectedCells).toBe(0);
    await captureJourneyStep('table-cell-selection-006');
  });
});
