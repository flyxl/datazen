import { expect, browser, $, $$ } from '@wdio/globals';
import { closeWorkspacePanels } from '../lib/testDataLifecycle.js';
import {
  connectSeededPgInWorkspace,
  executeSQLChecked,
  openQueryTab,
  clickTableInSidebar,
  waitForNewQueryButton,
  waitForTableInSidebar,
  setSafeMode,
} from '../helpers.js';

/**
 * The foreign-key navigation of the table grid (DB-07).
 *
 * 【留待 R 回归】This spec was written against the implementation but **not
 * run**: the machine has no `target/debug/datazen`, so the WebDriver build
 * could not be produced here. Every selector below was checked against the
 * implementation, one by one:
 *
 * - `[data-dt-fk-click="true"]`  — the jump entry of a cell (GridCell; the
 *                                  attribute is "true" or absent, and it only
 *                                  exists on a cell that may be followed);
 * - `[data-dt-fk="<target>"]`   — a cell that belongs to a foreign key; the
 *                                  **value** is the referenced target
 *                                  (`customers.id`, or `sales.parents.(x, y)`
 *                                  for a composite key — 00-contracts §4.4);
 *                                  it marks the **member**, not the arrow;
 * - `[data-dt-row]` / `[data-dt-col]` — the coordinates of a cell
 *                                  (00-contracts §4.4);
 * - `[data-dt-surface]`          — the scroll container of the grid
 *                                  (00-contracts §4.4);
 * - `[data-testid="table-fk-breadcrumb"]` / `-label` — the breadcrumb of a tab
 *                                  that was reached by a jump
 *                                  (TableFkBreadcrumb);
 * - `[data-testid="table-fk-back"]` / `-forward` — the two moves
 *                                  (TableFkBreadcrumb);
 * - `[data-testid="table-fk-error"]` — the inline error of a refused jump
 *                                  (TableView);
 * - `[data-testid="filter-editor"]` / `[data-testid="filter-clear"]` — the
 *                                  filter bar of the grid (FilterEditor);
 * - `[data-testid="panel-tab"]` / `[data-testid="panel-tab-select"]` — the
 *                                  tabs of the workspace (PanelTabBar);
 * - `[data-testid="data-table-cell"]` — a cell of the grid (GridCell).
 *
 * The filter conditions are asserted through the store the app exposes for
 * the E2E (`window.__tableDataStore`), because the contract is about the
 * **structured** conditions ("referenced column = value"), not about the
 * wording of the summary.
 *
 * The fixtures are four tables:
 *
 * - `_e2e_fk_parent`      — the target of the single-column key, and the
 *                           source of the key that points **back** (the cycle);
 * - `_e2e_fk_comp_parent` — the target of the composite key;
 * - `_e2e_fk_child`       — the table the journey starts in;
 * - `_e2e_fk_stale`       — a table whose constraint is dropped behind the
 *                           grid's back (the refused jump).
 */

const PARENT_TABLE = '_e2e_fk_parent';
const COMPOSITE_PARENT_TABLE = '_e2e_fk_comp_parent';
const CHILD_TABLE = '_e2e_fk_child';
const STALE_TABLE = '_e2e_fk_stale';
/** The value of the single-column key of the first child row. */
const CUSTOMER_NO = 4242;
/** The value of the single-column key of the second child row. */
const OTHER_CUSTOMER_NO = 4343;
/** The values of the composite key of the first child row. */
const COMPOSITE_X = 11;
const COMPOSITE_Y = 22;
/** Enough rows for the grid to scroll. */
const ROW_COUNT = 60;

async function openTable(tableName: string) {
  await waitForTableInSidebar(tableName);
  await clickTableInSidebar(tableName);
  await $('[data-testid="data-table-cell"]').waitForDisplayed({ timeout: 20000 });
}

/** The jump entry of a cell, or `null` when the cell has none. */
async function arrowOf(rowIndex: number, columnName: string): Promise<{ found: boolean } | null> {
  return browser.execute(
    (row: number, column: string) => {
      const cell = document.querySelector(`[data-dt-row="${row}"][data-dt-col="${column}"]`);
      if (!cell) return null;
      const arrow = cell.querySelector('[data-dt-fk-click="true"]');
      return { found: arrow !== null };
    },
    rowIndex,
    columnName,
  );
}

/** The number of open tabs. */
async function tabCount(): Promise<number> {
  return browser.execute(() => document.querySelectorAll('[data-testid="panel-tab"]').length);
}

/** The label of the tab that is active, or `''`. */
async function activeTabLabel(): Promise<string> {
  return browser.execute(() => {
    const active = document.querySelector('[data-testid="panel-tab-select"][aria-selected="true"]');
    return active?.getAttribute('aria-label') ?? '';
  });
}

/** The conditions the grid of a table runs with. */
async function filtersOf(
  tableName: string,
): Promise<{ column: string; operator: string; value: unknown }[]> {
  return browser.execute((table: string) => {
    const store = (
      window as unknown as {
        __tableDataStore?: {
          getState: () => {
            byPanel: Map<
              string,
              {
                context: { table: string } | null;
                filters: { column: string; operator: string; value: unknown }[];
              }
            >;
          };
        };
      }
    ).__tableDataStore;
    if (!store) return [];
    for (const slice of store.getState().byPanel.values()) {
      if (slice.context?.table === table) return slice.filters;
    }
    return [];
  }, tableName);
}

/** The filter logic of the grid of a table. */
async function filterLogicOf(tableName: string): Promise<string> {
  return browser.execute((table: string) => {
    const store = (
      window as unknown as {
        __tableDataStore?: {
          getState: () => {
            byPanel: Map<string, { context: { table: string } | null; filterLogic: string }>;
          };
        };
      }
    ).__tableDataStore;
    if (!store) return '';
    for (const slice of store.getState().byPanel.values()) {
      if (slice.context?.table === table) return slice.filterLogic;
    }
    return '';
  }, tableName);
}

/** The scroll offset of the grid. */
async function gridScrollTop(): Promise<number> {
  return browser.execute(() => {
    const surface = document.querySelector('[data-dt-surface]');
    return surface instanceof HTMLElement ? surface.scrollTop : -1;
  });
}

/** The number of rows the grid currently shows. */
async function visibleRowCount(): Promise<number> {
  return browser.execute(() => {
    const rows = new Set<number>();
    document.querySelectorAll('[data-dt-row]').forEach((cell) => {
      const row = cell.getAttribute('data-dt-row');
      if (row !== null) rows.add(Number(row));
    });
    return rows.size;
  });
}

/** Select the tab of a table by its name. */
async function selectTableTab(tableName: string) {
  const tabs = await $$('[data-testid="panel-tab-select"]');
  for (const tab of tabs) {
    const label = (await tab.getAttribute('aria-label')) ?? '';
    if (!label.includes(tableName)) continue;
    await tab.click();
    await browser.pause(600);
    return;
  }
  throw new Error(`The tab of ${tableName} is not open`);
}

describe('外键跳转与返回栈 (DB-07)', () => {
  let mainWindow: string;

  before(async () => {
    mainWindow = await browser.getWindowHandle();
    await setSafeMode(false);
    await connectSeededPgInWorkspace();
    await waitForNewQueryButton(20000);
    await browser.pause(1500);

    await openQueryTab();
    for (const table of [CHILD_TABLE, PARENT_TABLE, COMPOSITE_PARENT_TABLE, STALE_TABLE]) {
      await executeSQLChecked(`DROP TABLE IF EXISTS ${table} CASCADE`);
    }
    await executeSQLChecked(
      `CREATE TABLE ${PARENT_TABLE} (
        cust_no INT PRIMARY KEY,
        name VARCHAR(100) NOT NULL,
        origin_child_id INT
      )`,
    );
    await executeSQLChecked(
      `CREATE TABLE ${COMPOSITE_PARENT_TABLE} (
        px INT NOT NULL,
        py INT NOT NULL,
        label VARCHAR(50),
        PRIMARY KEY (px, py)
      )`,
    );
    await executeSQLChecked(
      `CREATE TABLE ${CHILD_TABLE} (
        id SERIAL PRIMARY KEY,
        customer_id INT REFERENCES ${PARENT_TABLE}(cust_no),
        parent_px INT,
        parent_py INT,
        CONSTRAINT ${CHILD_TABLE}_comp_fk FOREIGN KEY (parent_px, parent_py)
          REFERENCES ${COMPOSITE_PARENT_TABLE}(px, py)
      )`,
    );
    // The key that points **back**: it is what makes the round trip possible.
    await executeSQLChecked(
      `ALTER TABLE ${PARENT_TABLE} ADD CONSTRAINT ${PARENT_TABLE}_origin_fk
         FOREIGN KEY (origin_child_id) REFERENCES ${CHILD_TABLE}(id)`,
    );
    await executeSQLChecked(
      `CREATE TABLE ${STALE_TABLE} (
        id SERIAL PRIMARY KEY,
        parent_id INT REFERENCES ${PARENT_TABLE}(cust_no)
      )`,
    );
    await executeSQLChecked(
      `INSERT INTO ${PARENT_TABLE} (cust_no, name) VALUES (${CUSTOMER_NO}, 'Alice'), (${OTHER_CUSTOMER_NO}, 'Bob')`,
    );
    await executeSQLChecked(
      `INSERT INTO ${COMPOSITE_PARENT_TABLE} (px, py, label) VALUES (${COMPOSITE_X}, ${COMPOSITE_Y}, 'composite')`,
    );
    // Row 0: a complete single-column key and a complete composite key.
    await executeSQLChecked(
      `INSERT INTO ${CHILD_TABLE} (customer_id, parent_px, parent_py)
       VALUES (${CUSTOMER_NO}, ${COMPOSITE_X}, ${COMPOSITE_Y})`,
    );
    // Row 1: the key columns are NULL — there is nothing to follow.
    await executeSQLChecked(
      `INSERT INTO ${CHILD_TABLE} (customer_id, parent_px, parent_py) VALUES (NULL, NULL, NULL)`,
    );
    // Row 2: a second, different value of the single-column key.
    await executeSQLChecked(
      `INSERT INTO ${CHILD_TABLE} (customer_id, parent_px, parent_py) VALUES (${OTHER_CUSTOMER_NO}, NULL, NULL)`,
    );
    // The rest: rows without any key, so the grid can scroll.
    await executeSQLChecked(
      `INSERT INTO ${CHILD_TABLE} (customer_id, parent_px, parent_py)
       SELECT NULL, NULL, NULL FROM generate_series(1, ${ROW_COUNT - 3}) AS g`,
    );
    await executeSQLChecked(
      `UPDATE ${PARENT_TABLE} SET origin_child_id = 1 WHERE cust_no = ${CUSTOMER_NO}`,
    );
    await executeSQLChecked(
      `INSERT INTO ${STALE_TABLE} (parent_id) VALUES (${CUSTOMER_NO})`,
    );
    await browser.pause(1200);
    await closeWorkspacePanels(browser);
    await browser.pause(500);
  });

  after(async () => {
    await browser.switchToWindow(mainWindow);
    await openQueryTab();
    for (const table of [CHILD_TABLE, PARENT_TABLE, COMPOSITE_PARENT_TABLE, STALE_TABLE]) {
      await executeSQLChecked(`DROP TABLE IF EXISTS ${table} CASCADE`);
    }
    await closeWorkspacePanels(browser);
  });

  it('TC-FK-001 single-column jump seeds a visible filter', async () => {
    await openTable(CHILD_TABLE);

    const arrow = await arrowOf(0, 'customer_id');
    expect(arrow?.found).toBe(true);

    const before = await tabCount();
    await $('[data-dt-row="0"][data-dt-col="customer_id"] [data-dt-fk-click="true"]').click();
    await browser.pause(1500);

    // A new tab, and it is the tab of the parent table.
    expect(await tabCount()).toBe(before + 1);
    await $('[data-testid="table-fk-breadcrumb"]').waitForDisplayed({ timeout: 10000 });

    // The condition is the *referenced* column of the target, and it is the
    // value of the cell the arrow was clicked in.
    const filters = await filtersOf(PARENT_TABLE);
    expect(filters).toHaveLength(1);
    expect(filters[0].column).toBe('cust_no');
    expect(filters[0].operator).toBe('eq');
    expect(Number(filters[0].value)).toBe(CUSTOMER_NO);
    expect(await filterLogicOf(PARENT_TABLE)).toBe('and');
    // The condition is visible in the filter bar of the grid.
    await $('[data-testid="filter-editor"]').waitForDisplayed({ timeout: 10000 });

    // The breadcrumb names both sides of the jump.
    const label = await $('[data-testid="table-fk-breadcrumb-label"]').getText();
    expect(label).toContain(CHILD_TABLE);
    expect(label).toContain(PARENT_TABLE);

    // Clearing the filters restores the whole table, and the breadcrumb stays.
    await $('[data-testid="filter-clear"]').click();
    await browser.pause(1500);
    expect(await filtersOf(PARENT_TABLE)).toHaveLength(0);
    await $('[data-testid="table-fk-breadcrumb"]').waitForDisplayed({ timeout: 5000 });
  });

  it('TC-FK-002 composite fk jump uses an AND group', async () => {
    await openTable(CHILD_TABLE);

    // The arrow of a composite key sits on the first member of the group.
    const first = await arrowOf(0, 'parent_px');
    const second = await arrowOf(0, 'parent_py');
    expect(first?.found).toBe(true);
    expect(second?.found).toBe(false);

    const before = await tabCount();
    await $('[data-dt-row="0"][data-dt-col="parent_px"] [data-dt-fk-click="true"]').click();
    await browser.pause(1500);

    expect(await tabCount()).toBe(before + 1);
    const filters = await filtersOf(COMPOSITE_PARENT_TABLE);
    // Both referenced columns, combined with AND — never the first one alone.
    expect(filters).toHaveLength(2);
    expect(filters.map((filter) => filter.column)).toEqual(['px', 'py']);
    expect(Number(filters[0].value)).toBe(COMPOSITE_X);
    expect(Number(filters[1].value)).toBe(COMPOSITE_Y);
    expect(await filterLogicOf(COMPOSITE_PARENT_TABLE)).toBe('and');
    // The result is the one row of the seed.
    expect(await visibleRowCount()).toBe(1);
  });

  it('TC-FK-003 back restores the source panel position', async () => {
    await openTable(CHILD_TABLE);

    // Scroll the source grid down before the jump.
    await browser.execute(() => {
      const surface = document.querySelector('[data-dt-surface]');
      if (surface instanceof HTMLElement) surface.scrollTop = 120;
    });
    await browser.pause(500);
    const scrollBefore = await gridScrollTop();
    expect(scrollBefore).toBeGreaterThan(0);

    await $('[data-dt-row="2"][data-dt-col="customer_id"] [data-dt-fk-click="true"]').click();
    await browser.pause(1500);
    expect(await gridScrollTop()).not.toBe(scrollBefore);

    // "Back" returns to the source tab and puts the grid back where it was.
    await $('[data-testid="table-fk-back"]').click();
    await browser.pause(1500);
    await selectTableTab(CHILD_TABLE);

    const scrollAfter = await gridScrollTop();
    expect(Math.abs(scrollAfter - scrollBefore)).toBeLessThanOrEqual(28);
  });

  it('TC-FK-004 A to B to A keeps exactly two tabs', async () => {
    await openTable(CHILD_TABLE);

    const before = await tabCount();
    // A → B: the parent table opens in a new tab.
    await $('[data-dt-row="2"][data-dt-col="customer_id"] [data-dt-fk-click="true"]').click();
    await browser.pause(1500);
    expect(await tabCount()).toBe(before + 1);

    // B → A: the parent row points back at the child row. The tab of the child
    // is already open, so the jump reuses it instead of opening a third one.
    const back = await arrowOf(0, 'origin_child_id');
    expect(back?.found).toBe(true);
    await $('[data-dt-row="0"][data-dt-col="origin_child_id"] [data-dt-fk-click="true"]').click();
    await browser.pause(1500);

    expect(await tabCount()).toBe(before + 1);
    // The active tab is the child table again.
    expect(await activeTabLabel()).toContain(CHILD_TABLE);
  });

  it('TC-FK-005 null fk cell shows no arrow', async () => {
    await openTable(CHILD_TABLE);

    // Row 1 has no value in the key column: there is nothing to follow.
    const nullCell = await arrowOf(1, 'customer_id');
    expect(nullCell?.found).toBe(false);
    // The cell is still marked as a member of the key, and the marker names
    // the target it points at.
    const marked = await browser.execute(() => {
      const cell = document.querySelector('[data-dt-row="1"][data-dt-col="customer_id"]');
      return cell?.getAttribute('data-dt-fk') ?? null;
    });
    expect(marked).toBe('cust_no');

    const before = await tabCount();
    await $('[data-dt-row="1"][data-dt-col="customer_id"]').click();
    await browser.pause(1000);
    expect(await tabCount()).toBe(before);
  });

  it('TC-FK-006 unreachable target shows a classified error, not a blank screen', async () => {
    await openTable(STALE_TABLE);

    // The grid believes in the constraint (its metadata is cached), and the
    // host does not any more: the constraint is dropped behind the grid's
    // back. The jump is then refused with a classified reason.
    await openQueryTab();
    await executeSQLChecked(`ALTER TABLE ${STALE_TABLE} DROP CONSTRAINT ${STALE_TABLE}_parent_id_fkey`);
    await browser.pause(1000);
    await selectTableTab(STALE_TABLE);

    const before = await tabCount();
    const arrow = await arrowOf(0, 'parent_id');
    expect(arrow?.found).toBe(true);

    await $('[data-dt-row="0"][data-dt-col="parent_id"] [data-dt-fk-click="true"]').click();

    await $('[data-testid="table-fk-error"]').waitForDisplayed({ timeout: 15000 });
    const message = await $('[data-testid="table-fk-error"]').getText();
    expect(message.length).toBeGreaterThan(0);
    // No tab was opened, and the grid is still there.
    expect(await tabCount()).toBe(before);
    await $('[data-testid="data-table-cell"]').waitForDisplayed({ timeout: 5000 });
  });
});
