/**
 * DB-04 new rows (INSERT) — TC-INS-001 ~ TC-INS-007.
 *
 * The grid gains a "+ row" entry, a draft row that is rendered **after** the
 * real rows, and the commit path that sends the draft as an `insertMarked`
 * row change.
 *
 * DOM contract this suite relies on (all of it produced by this track):
 *   - `[data-testid="table-insert-row"]`        toolbar entry (hidden on a read-only connection)
 *   - `[data-testid="draft-row-N"]`              the N-th draft row (N = draft ordinal, 0-based)
 *   - `[data-row-kind="draft"]` / `[data-draft-id]`
 *   - `[data-testid="draft-cell"]` + `[data-draft-col]` + `[data-draft-state]` (default | value | null)
 *   - `[data-testid="draft-row-badge"]`          the "NEW" marker
 *   - `[data-testid="draft-row-remove"]`         discard a single draft
 *   - `[data-testid="draft-add-row"]`            the empty area below the last row
 *   - `[data-testid="pending-changes-bar"]` / `pending-preview` / `pending-commit`
 *   - `[data-testid="pending-plan-dialog"]`      the preview of the plan
 *   - `[data-testid="table-insert-degraded"]`    the neutral notice when the new row cannot be located
 *
 * Approach — the hybrid strategy of `e2e/specs/table-edit.ts`:
 *   - **entries** are driven through the DOM (a real click on the button, a real
 *     double click on a draft cell);
 *   - **cell values** are written through the store API exposed at
 *     `window.__tableDataStore`, because WKWebView's WebDriver does not deliver
 *     keyboard input into a React controlled input reliably. The store call is
 *     the same call the cell editor makes when it commits, and every case still
 *     asserts the resulting **DOM**, not only the state;
 *   - **the database** is checked with SQL through the query tab.
 *
 * Two fixtures are used: a table with a primary key (the normal case) and one
 * without (the case where the new row provably cannot be located, because the
 * "locate the new row" rule needs a single comparable primary key).
 *
 * 【留待 R 回归】This suite was written without a compiled binary on the
 * authoring machine (`target/debug/datazen` was absent), so it has **not** been
 * executed here — no case is allowed to be reported as passed on the strength
 * of a skipped or self-satisfied assertion. The R regression has to run it
 * against a binary built with `pnpm tauri:build:webdriver` (a plain
 * `cargo build` produces a bundle without the `VITE_E2E` flag, and then the
 * `tid()`-gated test ids of the surrounding application — and the
 * `window.__tableDataStore` handle — are not rendered).
 */
import { browser, expect, $, $$ } from '@wdio/globals';
import { closeWorkspacePanels } from '../lib/testDataLifecycle.js';
import {
  captureJourneyStep,
  clickTableInSidebar,
  closeExtraWindows,
  connectSeededPgInWorkspace,
  confirmWebDialog,
  executeSQL,
  executeSQLChecked,
  openQueryTab,
  setSafeMode,
  switchSubTab,
  waitForTableInSidebar,
} from '../helpers.js';

const TEST_TABLE = '_e2e_insert_test';
const NO_PK_TABLE = '_e2e_insert_nopk_test';

interface DraftSnapshot {
  drafts: number;
  order: string[];
  values: Record<string, Record<string, unknown>>;
  rows: number;
  highlightedNewRowKey: string | null;
}

/** The staged drafts of the panel this suite works on. */
async function draftSnapshot(): Promise<DraftSnapshot | null> {
  return browser.execute(() => {
    type Slice = {
      rows: unknown[];
      draftInserts: Map<string, { draftId: string; values: Record<string, unknown> }>;
      draftInsertOrder: string[];
      highlightedNewRowKey: string | null;
    };
    const store = (
      window as unknown as {
        __tableDataStore?: { getState: () => { byPanel: Map<string, Slice> } };
      }
    ).__tableDataStore;
    if (!store) return null;
    const slice = [...store.getState().byPanel.values()].find(
      (candidate) => candidate.draftInserts.size > 0,
    );
    if (!slice) return null;
    const values: Record<string, Record<string, unknown>> = {};
    for (const [draftId, draft] of slice.draftInserts.entries()) {
      values[draftId] = draft.values;
    }
    return {
      drafts: slice.draftInserts.size,
      order: [...slice.draftInsertOrder],
      values,
      rows: slice.rows.length,
      highlightedNewRowKey: slice.highlightedNewRowKey,
    };
  });
}

/** Write one cell of a draft — the call the cell editor makes when it commits. */
async function writeDraftCell(
  draftId: string,
  column: string,
  value: unknown,
  clear = false,
): Promise<boolean> {
  return browser.execute(
    (id: string, col: string, val: unknown, isClear: boolean) => {
      type Slice = { draftInserts: Map<string, unknown> };
      const store = (
        window as unknown as {
          __tableDataStore?: {
            getState: () => {
              byPanel: Map<string, Slice>;
              setDraftValue: (
                panelId: string,
                draftId: string,
                column: string,
                value: unknown,
              ) => void;
            };
          };
        }
      ).__tableDataStore;
      if (!store) return false;
      for (const [panelId, slice] of store.getState().byPanel.entries()) {
        if (!slice.draftInserts.has(id)) continue;
        // `undefined` removes the key: the column is not written at all.
        store.getState().setDraftValue(panelId, id, col, isClear ? undefined : val);
        return true;
      }
      return false;
    },
    draftId,
    column,
    value,
    clear,
  );
}

/** The draft id of the N-th draft row, read from the DOM contract. */
async function draftIdOf(index: number): Promise<string | null> {
  return browser.execute(
    (rowIndex: number) => {
      const row = document.querySelector(`[data-testid="draft-row-${rowIndex}"]`);
      return row?.getAttribute('data-draft-id') ?? null;
    },
    index,
  );
}

async function clickInsertRow(): Promise<boolean> {
  return browser.execute(() => {
    const button = document.querySelector(
      '[data-testid="table-insert-row"]',
    ) as HTMLButtonElement | null;
    if (!button) return false;
    button.click();
    return true;
  });
}

async function doubleClickDraftCell(draftIndex: number, column: string): Promise<boolean> {
  return browser.execute(
    (rowIndex: number, col: string) => {
      const cell = document.querySelector(
        `[data-testid="draft-row-${rowIndex}"] [data-draft-col="${col}"]`,
      ) as HTMLElement | null;
      if (!cell) return false;
      cell.dispatchEvent(new MouseEvent('dblclick', { bubbles: true, cancelable: true }));
      return true;
    },
    draftIndex,
    column,
  );
}

/** The `data-draft-state` of one cell of a draft row. */
async function draftCellState(draftIndex: number, column: string): Promise<string | null> {
  return browser.execute(
    (rowIndex: number, col: string) => {
      const cell = document.querySelector(
        `[data-testid="draft-row-${rowIndex}"] [data-draft-col="${col}"]`,
      );
      return cell?.getAttribute('data-draft-state') ?? null;
    },
    draftIndex,
    column,
  );
}

async function openTable(tableName: string): Promise<void> {
  await clickTableInSidebar(tableName);
  await browser.pause(1500);
  // The grid lives on the "data" sub tab of the table panel.
  await switchSubTab('data');
  await browser.waitUntil(
    async () =>
      browser.execute(
        () => document.querySelectorAll('[data-testid="data-table-cell"]').length > 0,
      ),
    { timeout: 20000, timeoutMsg: `表 ${tableName} 数据行未渲染` },
  );
  await browser.pause(500);
}

describe('新增行 INSERT (TC-INS-001~007)', () => {
  let mainWindow: string;
  let queryTabLabel: string;

  /**
   * Each query tab owns a retained physical session, so the suite uses **one**
   * fixture editor and switches back to it instead of opening a new tab per
   * statement (see `e2e/specs/table-edit.ts`).
   */
  async function activateFixtureQuery() {
    const tabs = await $$('[data-testid="panel-tab-select"]');
    for (const tab of tabs) {
      if ((await tab.getAttribute('aria-label')) !== queryTabLabel) continue;
      await tab.scrollIntoView({ block: 'center' });
      await tab.click();
      await $('[data-testid="editor-execute-button"]').waitForDisplayed({ timeout: 10000 });
      return;
    }
    throw new Error('The table-insert fixture query tab is no longer available');
  }

  before(async function () {
    this.timeout(120000);
    mainWindow = await browser.getWindowHandle();
    await setSafeMode(false);
    await connectSeededPgInWorkspace();
    await openQueryTab();

    const activeQueryTab = await $('[data-testid="panel-tab-select"][aria-selected="true"]');
    const label = await activeQueryTab.getAttribute('aria-label');
    if (!label) throw new Error('The table-insert fixture query tab has no label');
    queryTabLabel = label;

    await executeSQLChecked(`DROP TABLE IF EXISTS ${TEST_TABLE}`);
    await executeSQLChecked(`
      CREATE TABLE ${TEST_TABLE} (
        id SERIAL PRIMARY KEY,
        code TEXT,
        name TEXT
      )
    `);
    await executeSQLChecked(`
      INSERT INTO ${TEST_TABLE} (code, name)
      SELECT 'code_' || g, 'row_' || g FROM generate_series(1, 3) g
    `);

    await executeSQLChecked(`DROP TABLE IF EXISTS ${NO_PK_TABLE}`);
    await executeSQLChecked(`
      CREATE TABLE ${NO_PK_TABLE} (
        code TEXT,
        name TEXT
      )
    `);

    await waitForTableInSidebar(TEST_TABLE, 30000);
    await openTable(TEST_TABLE);

    // The entry point is wired: a missing button fails the suite instead of
    // skipping it, because a skipped suite would hide a broken insert path.
    const entryWired = await browser.execute(
      () => !!document.querySelector('[data-testid="table-insert-row"]'),
    );
    expect(entryWired).toBe(true);
  });

  after(async () => {
    try {
      await browser.switchToWindow(mainWindow);
      await activateFixtureQuery();
      await executeSQL(`DROP TABLE IF EXISTS ${TEST_TABLE}`);
      await executeSQL(`DROP TABLE IF EXISTS ${NO_PK_TABLE}`);
    } catch {
      // best-effort cleanup
    }
    await setSafeMode(true);
    await closeExtraWindows(mainWindow);
    await closeWorkspacePanels(browser);
  });

  it('TC-INS-001: 「+ 行」应追加一条草稿行，且草稿不进入真实行集', async () => {
    const clicked = await clickInsertRow();
    expect(clicked).toBe(true);
    await browser.pause(400);

    const draftRow = await $('[data-testid="draft-row-0"]');
    await expect(draftRow).toBeDisplayed();
    expect(await draftRow.getAttribute('data-row-kind')).toBe('draft');
    expect(await draftRow.getAttribute('data-draft-id')).toBeTruthy();
    // The "NEW" marker tells the user the row is not saved yet.
    await expect(await $('[data-testid="draft-row-badge"]')).toBeDisplayed();

    const snapshot = await draftSnapshot();
    expect(snapshot?.drafts).toBe(1);
    // The real row set is untouched: a draft is a derived row (04 §3.7 R-5).
    expect(snapshot?.rows).toBe(3);
    await captureJourneyStep('table-insert-001');
  });

  it('TC-INS-002: 未写的列显示 DEFAULT，写入后显示该值', async () => {
    const draftId = await draftIdOf(0);
    expect(draftId).toBeTruthy();

    // Nothing written yet: every column shows the DEFAULT placeholder.
    expect(await draftCellState(0, 'code')).toBe('default');
    expect(await draftCellState(0, 'name')).toBe('default');
    const placeholder = await $('[data-testid="draft-row-0"] [data-testid="draft-cell-default"]');
    await expect(placeholder).toBeDisplayed();

    // The cell is editable: a double click opens the editor.
    expect(await doubleClickDraftCell(0, 'code')).toBe(true);
    await browser.pause(300);
    expect(
      await browser.execute(() => !!document.querySelector('[data-testid="table-edit-input"]')),
    ).toBe(true);
    // Escape leaves the editor without writing anything.
    await browser.execute(() => {
      const editor = document.querySelector(
        '[data-testid="table-edit-input"]',
      ) as HTMLInputElement | null;
      editor?.dispatchEvent(
        new KeyboardEvent('keydown', { key: 'Escape', code: 'Escape', bubbles: true }),
      );
    });
    await browser.pause(300);
    expect(await draftCellState(0, 'code')).toBe('default');

    // The value itself is written through the store call the editor makes
    // when it commits (WKWebView does not deliver keyboard input into a React
    // controlled input reliably — see the header).
    const written = await writeDraftCell(draftId!, 'code', 'e2e_code');
    expect(written).toBe(true);
    await browser.pause(300);

    expect(await draftCellState(0, 'code')).toBe('value');
    // The column that was not written keeps its placeholder.
    expect(await draftCellState(0, 'name')).toBe('default');
    const snapshot = await draftSnapshot();
    expect(snapshot?.values[draftId!]).toEqual({ code: 'e2e_code' });
    await captureJourneyStep('table-insert-002');
  });

  it('TC-INS-003: 清空草稿格应让该列回到「不写」', async () => {
    const draftId = await draftIdOf(0);
    expect(draftId).toBeTruthy();

    // Clearing a cell removes the key: the column is not written any more
    // (it must not become "written as NULL").
    const cleared = await writeDraftCell(draftId!, 'code', undefined, true);
    expect(cleared).toBe(true);
    await browser.pause(300);

    expect(await draftCellState(0, 'code')).toBe('default');
    const snapshot = await draftSnapshot();
    expect(snapshot?.values[draftId!]).toEqual({});
    await captureJourneyStep('table-insert-003');
  });

  it('TC-INS-004: 快捷键应追加草稿行，行号槽可丢弃草稿', async () => {
    const firstDraftId = await draftIdOf(0);
    expect(firstDraftId).toBeTruthy();

    // Cmd/Ctrl+I appends a draft row.
    await browser.execute(() => {
      document
        .querySelector('[data-testid="table-insert-row"]')
        ?.dispatchEvent(
          new KeyboardEvent('keydown', { key: 'i', metaKey: true, bubbles: true, cancelable: true }),
        );
    });
    await browser.pause(400);
    await expect(await $('[data-testid="draft-row-1"]')).toBeDisplayed();
    const secondDraftId = await draftIdOf(1);
    expect(secondDraftId).toBeTruthy();
    expect(secondDraftId).not.toBe(firstDraftId);

    const snapshot = await draftSnapshot();
    expect(snapshot?.drafts).toBe(2);
    // The creation order decides the plan order: the assertion is on the
    // order **itself**, not on its length.
    expect(snapshot?.order).toEqual([firstDraftId, secondDraftId]);

    // The row-number slot of the second draft discards it.
    const removed = await browser.execute(() => {
      const button = document.querySelector(
        '[data-testid="draft-row-1"] [data-testid="draft-row-remove"]',
      ) as HTMLButtonElement | null;
      if (!button) return false;
      button.click();
      return true;
    });
    expect(removed).toBe(true);
    await browser.pause(300);

    expect(await browser.execute(() => !!document.querySelector('[data-testid="draft-row-1"]'))).toBe(
      false,
    );
    expect((await draftSnapshot())?.drafts).toBe(1);
    await captureJourneyStep('table-insert-004');
  });

  it('TC-INS-005: 预览应列出 INSERT 语句与插入条数', async () => {
    const draftId = await draftIdOf(0);
    const written = await writeDraftCell(draftId!, 'code', 'e2e_preview');
    expect(written).toBe(true);
    await browser.pause(300);

    const pendingBar = await $('[data-testid="pending-changes-bar"]');
    await pendingBar.waitForDisplayed({ timeout: 8000 });
    await expect(await $('[data-testid="pending-preview"]')).toBeDisplayed();

    await $('[data-testid="pending-preview"]').click();
    const preview = await $('[data-testid="pending-plan-dialog"]');
    await preview.waitForDisplayed({ timeout: 8000 });
    const previewText = await preview.getText();
    // The table is called `_e2e_insert_test`, so an assertion on "INSERT"
    // would be satisfied by the table name alone. Only a rendered statement
    // says "INSERT INTO".
    expect(previewText).toContain('INSERT INTO');
    // The plan lists exactly one insert — the count is asserted, not implied.
    // The description renders the raw count next to the interpolated string,
    // so the segment reads "1 1 insert(s)"; the boundary in the pattern is
    // what makes the assertion about **one** insert: "11 insert(s)" and
    // "21 insert(s)" must not satisfy it.
    expect(previewText).toMatch(/(?:^|\D)1 insert\(s\)(?:\D|$)/);
    // And the preview really lists one INSERT statement (the fingerprint is a
    // <code> element too, so the statements are counted, not guessed).
    const insertStatements = await browser.execute(() => {
      const dialog = document.querySelector('[data-testid="pending-plan-dialog"]');
      const codes = Array.from(dialog?.querySelectorAll('code') ?? []);
      return codes.filter((code) => (code.textContent ?? '').includes('INSERT INTO')).length;
    });
    expect(insertStatements).toBe(1);
    // The bound value is shown in the parameter summary.
    expect(previewText).toContain('e2e_preview');

    const previewButtons = await preview.$$('button');
    await previewButtons[(await previewButtons.length) - 1].click();
    await preview.waitForDisplayed({ reverse: true, timeout: 5000 });
    await captureJourneyStep('table-insert-005');
  });

  it('TC-INS-006: 提交应把草稿写成一行新记录并清空草稿', async () => {
    await $('[data-testid="pending-commit"]').click();
    await confirmWebDialog();

    // The drafts are staged work: the bar disappears once they are committed.
    await $('[data-testid="pending-changes-bar"]').waitForDisplayed({
      reverse: true,
      timeout: 15000,
    });
    expect(await browser.execute(() => !!document.querySelector('[data-row-kind="draft"]'))).toBe(
      false,
    );

    // The row really is in the database.
    await activateFixtureQuery();
    await executeSQLChecked(`SELECT code FROM ${TEST_TABLE} WHERE code = 'e2e_preview'`);
    await browser.pause(600);
    expect(await $('body').getText()).toContain('e2e_preview');
    await captureJourneyStep('table-insert-006');
  });

  it('TC-INS-007: 无主键表插入后应给出中性的「无法定位」提示', async () => {
    // A table without a primary key: inserting is allowed, but the new row
    // cannot be located (the rule needs a single comparable primary key), so
    // the interface has to say so instead of claiming a highlight.
    await waitForTableInSidebar(NO_PK_TABLE, 30000);
    await openTable(NO_PK_TABLE);

    const clicked = await clickInsertRow();
    expect(clicked).toBe(true);
    await browser.pause(400);

    const draftId = await draftIdOf(0);
    expect(draftId).toBeTruthy();
    const written = await writeDraftCell(draftId!, 'code', 'e2e_nopk');
    expect(written).toBe(true);
    await browser.pause(300);

    const pendingBar = await $('[data-testid="pending-changes-bar"]');
    await pendingBar.waitForDisplayed({ timeout: 8000 });
    await $('[data-testid="pending-commit"]').click();
    await confirmWebDialog();

    const degraded = await $('[data-testid="table-insert-degraded"]');
    await degraded.waitForDisplayed({ timeout: 15000 });
    // The notice is a status, not an error, and it does not claim a highlight.
    expect(await degraded.getAttribute('role')).toBe('status');

    // The row is in the database even though it could not be located.
    await activateFixtureQuery();
    await executeSQLChecked(`SELECT code FROM ${NO_PK_TABLE} WHERE code = 'e2e_nopk'`);
    await browser.pause(600);
    expect(await $('body').getText()).toContain('e2e_nopk');
    await captureJourneyStep('table-insert-007');
  });
});
