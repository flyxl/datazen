import { expect, browser, $, $$ } from '@wdio/globals';
import { closeWorkspacePanels, runPsql } from '../lib/testDataLifecycle.js';
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

/**
 * The scroll offset of the grid that is actually on screen.
 *
 * Every open tab keeps its grid in the document, so `querySelector` returns the
 * first one whether or not it is the active tab — that read the wrong tab's
 * offset and made the restore assertion compare two different grids.
 */
async function gridScrollTop(): Promise<number> {
  return browser.execute(() => {
    const surfaces = Array.from(document.querySelectorAll('[data-dt-surface]'));
    const visible = surfaces.find((el) => el.getBoundingClientRect().height > 0) ?? surfaces[0];
    return visible instanceof HTMLElement ? visible.scrollTop : -1;
  });
}

/** The scroll offset of the grid that holds the FK cell the case clicks. */
/**
 * The grid the user actually sees. Every tab keeps its own grid in the DOM, so
 * `document.querySelector` can hand back the arrow of a hidden tab — scrolling
 * that one would leave the panel under test at the top.
 */
async function visibleArrowIndex(): Promise<number> {
  return browser.execute(() => {
    const arrows = Array.from(
      document.querySelectorAll(
        '[data-dt-row="2"][data-dt-col="customer_id"] [data-dt-fk-click="true"]',
      ),
    );
    return arrows.findIndex((arrow) => {
      const surface = arrow.closest('[data-dt-surface]');
      return surface instanceof HTMLElement && surface.getBoundingClientRect().height > 0;
    });
  });
}

async function sourceGridScrollTop(): Promise<number> {
  const index = await visibleArrowIndex();
  if (index < 0) return -1;
  return browser.execute((target: number) => {
    const arrow = document.querySelectorAll(
      '[data-dt-row="2"][data-dt-col="customer_id"] [data-dt-fk-click="true"]',
    )[target];
    const surface = arrow?.closest('[data-dt-surface]');
    return surface instanceof HTMLElement ? surface.scrollTop : -1;
  }, index);
}

/** The database of the grid that is on screen, as the workspace knows it. */
async function currentDatabase(): Promise<string> {
  const database = await browser.execute(() => {
    const store = (
      window as unknown as {
        __tableDataStore?: {
          getState: () => {
            byPanel: Map<string, { context: { database: string } | null }>;
          };
        };
      }
    ).__tableDataStore;
    if (!store) return '';
    for (const slice of store.getState().byPanel.values()) {
      if (slice.context?.database) return slice.context.database;
    }
    return '';
  });
  if (!database) throw new Error('the workspace has no database in its panel context');
  return database;
}

/** The number of rows the grid currently shows. */
async function visibleRowCount(): Promise<number> {
  return browser.execute(() => {
    const rows = new Set<number>();
    document.querySelectorAll('[data-dt-row]').forEach((cell) => {
      // Skip grid cells of a background tab: they are still in the document.
      if (cell.getBoundingClientRect().height === 0) return;
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
    await executeSQLChecked(`INSERT INTO ${STALE_TABLE} (parent_id) VALUES (${CUSTOMER_NO})`);
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
    await closeWorkspacePanels(browser);
    await browser.pause(500);
    await openTable(CHILD_TABLE);

    // The arrow of a composite key sits on the first member of the group.
    const first = await arrowOf(0, 'parent_px');
    const second = await arrowOf(0, 'parent_py');
    expect(first?.found).toBe(true);
    expect(second?.found).toBe(false);

    // The marker of the composite key names the target table **and** its
    // columns — again the shape, with the driver-shaped prefix left open.
    // It marks the *member* cell, so it is asserted on the source grid: after
    // the jump the parent tab is active and the child's cells are behind it.
    const marked = await browser.execute(() => {
      const cell = document.querySelector('[data-dt-row="0"][data-dt-col="parent_px"]');
      return cell?.getAttribute('data-dt-fk') ?? null;
    });
    expect(marked?.endsWith('_e2e_fk_comp_parent.(px, py)')).toBe(true);

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
    // One child tab only: `selectTableTab`/the grid helpers resolve by name, and
    // a leftover tab of the same table would be the one they find.
    await closeWorkspacePanels(browser);
    await browser.pause(500);
    await openTable(CHILD_TABLE);

    // Scroll the source grid down before the jump. The surface is resolved from
    // the cell that is about to be clicked: every open tab keeps its own grid in
    // the DOM, so `document.querySelector('[data-dt-surface]')` can scroll a
    // hidden tab and leave the panel under test at 0.
    const arrowIndex = await visibleArrowIndex();
    expect(arrowIndex).toBeGreaterThanOrEqual(0);
    // The offset has to keep the clicked row on screen. WebDriver scrolls an
    // element into view before clicking it, so clicking a row that this scroll
    // pushed out of the viewport silently rewinds the grid to the top *before*
    // the handler runs — the recorded position would then be 0 for a reason the
    // product cannot see. Row 2 sits at 56px, so 40 leaves it visible.
    await browser.execute((target: number) => {
      const arrow = document.querySelectorAll(
        '[data-dt-row="2"][data-dt-col="customer_id"] [data-dt-fk-click="true"]',
      )[target];
      const surface = arrow?.closest('[data-dt-surface]');
      if (surface instanceof HTMLElement) surface.scrollTop = 40;
    }, arrowIndex);
    await browser.pause(500);
    const scrollBefore = await sourceGridScrollTop();
    expect(scrollBefore).toBeGreaterThan(0);

    const beforeClick = await browser.execute((target: number) => {
      const cell = document.querySelectorAll(
        '[data-dt-row="2"][data-dt-col="customer_id"] [data-dt-fk-click="true"]',
      )[target];
      const own = cell?.closest('[data-dt-surface]') as HTMLElement | null;
      const surfaces = Array.from(document.querySelectorAll('[data-dt-surface]')).map((el) => ({
        scrollTop: (el as HTMLElement).scrollTop,
        height: Math.round(el.getBoundingClientRect().height),
        holdsCell: el === own,
      }));
      return { surfaces, ownScrollTop: own ? own.scrollTop : -1 };
    }, arrowIndex);
    const arrows = await $$(
      '[data-dt-row="2"][data-dt-col="customer_id"] [data-dt-fk-click="true"]',
    );
    // The press is dispatched without moving the focus. The driver's own click
    // focuses the arrow first, and the engine answers a focus inside the
    // transform-based virtual window by scrolling the grid (and clearing its
    // rows) before the handler runs — the recorded position would be 0 for a
    // reason that has nothing to do with the product. The real press path keeps
    // the focus on the surface (GridCell suppresses the mouse focus), which the
    // GridCell unit test covers.
    await browser.execute((target: number) => {
      const arrow = document.querySelectorAll(
        '[data-dt-row="2"][data-dt-col="customer_id"] [data-dt-fk-click="true"]',
      )[target];
      arrow?.dispatchEvent(new MouseEvent('click', { bubbles: true, cancelable: true }));
    }, arrowIndex);
    await browser.pause(1500);
    if ((await gridScrollTop()) === scrollBefore) {
      throw new Error(`FK 跳转没有离开源面板: ${JSON.stringify(beforeClick)}`);
    }

    // "Back" returns to the source tab and puts the grid back where it was. The
    // button itself activates the source tab — re-selecting the tab by its name
    // afterwards would just as well hit a second tab of the same table.
    await $('[data-testid="table-fk-back"]').click();
    await browser.pause(1500);
    expect(await activeTabLabel()).toContain(CHILD_TABLE);

    const scrollAfter = await sourceGridScrollTop();
    const restoreDiag = await browser.execute(() => {
      const surfaces = Array.from(document.querySelectorAll('[data-dt-surface]')).map((el) => ({
        scrollTop: (el as HTMLElement).scrollTop,
        height: el.getBoundingClientRect().height,
      }));
      const focused = document.activeElement?.closest('[data-dt-row]');
      const store = (
        globalThis as unknown as {
          __fkNavigationStore?: { getState: () => { viewports: Map<string, unknown> } };
        }
      ).__fkNavigationStore;
      const snapshot = store
        ? Array.from(store.getState().viewports.entries()).map(([key, value]) => ({
            key,
            value: value as Record<string, unknown>,
          }))
        : null;
      const panels = (
        globalThis as unknown as {
          __panelStore?: { getState: () => { panels: unknown[]; activePanelId: string | null } };
        }
      ).__panelStore;
      const panelState = panels
        ? {
            activePanelId: panels.getState().activePanelId,
            ids: (panels.getState().panels as { id: string }[]).map((p) => p.id),
          }
        : null;
      return {
        surfaces,
        focusedRow: focused?.getAttribute('data-dt-row') ?? null,
        focusedCol: focused?.getAttribute('data-dt-col') ?? null,
        snapshot,
        panelState,
        hasStore: Boolean(store),
      };
    });
    if (Math.abs(scrollAfter - scrollBefore) > 28) {
      throw new Error(
        `回退未恢复源面板位置: before=${scrollBefore} after=${scrollAfter} ${JSON.stringify(restoreDiag)} click=${JSON.stringify(beforeClick)}`,
      );
    }
  });

  it('TC-FK-004 A to B to A keeps exactly two tabs', async () => {
    // Start from a clean workspace: a parent tab left over from the previous
    // case would be *reused* by the jump instead of opened, and the tab count
    // would then prove nothing about the A → B → A path.
    await closeWorkspacePanels(browser);
    await browser.pause(500);
    await openTable(CHILD_TABLE);

    const before = await tabCount();
    // A → B: the parent table opens in a new tab. Row 0 is Alice, the only row
    // whose `origin_child_id` points back — jumping from another row lands on a
    // parent row with a NULL back reference, where no arrow can exist.
    await $('[data-dt-row="0"][data-dt-col="customer_id"] [data-dt-fk-click="true"]').click();
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
    await closeWorkspacePanels(browser);
    await browser.pause(500);
    await openTable(CHILD_TABLE);

    // Row 1 has no value in the key column: there is nothing to follow.
    const nullCell = await arrowOf(1, 'customer_id');
    expect(nullCell?.found).toBe(false);
    // The cell is still marked as a member of the key, and the marker names
    // the target it points at.
    //
    // The marker is the **reference target identifier**: the referenced table
    // and the referenced column. Its exact spelling is driver-shaped — the
    // schema prefix is PostgreSQL's alone (`public._e2e_fk_parent.cust_no`
    // there, `_e2e_fk_parent.cust_no` on MySQL/SQLite) — so the assertion
    // pins the *shape* (table + referenced column at the end) and the prefix
    // is never asserted in either direction.
    const marked = await browser.execute(() => {
      const cell = document.querySelector('[data-dt-row="1"][data-dt-col="customer_id"]');
      return cell?.getAttribute('data-dt-fk') ?? null;
    });
    expect(marked?.endsWith('_e2e_fk_parent.cust_no')).toBe(true);

    const before = await tabCount();
    await $('[data-dt-row="1"][data-dt-col="customer_id"]').click();
    await browser.pause(1000);
    expect(await tabCount()).toBe(before);
  });

  it('TC-FK-006 unreachable target shows a classified error, not a blank screen', async () => {
    await openTable(STALE_TABLE);

    // The grid believes in the constraint (its metadata is cached), and the
    // host does not any more: the constraint is dropped behind the grid's back.
    // The statement goes through `psql`, not through the app, because the app
    // invalidates its whole schema cache after every executed statement — that
    // would refresh the very metadata this case needs to be stale.
    runPsql(
      await currentDatabase(),
      `ALTER TABLE ${STALE_TABLE} DROP CONSTRAINT ${STALE_TABLE}_parent_id_fkey`,
    );
    await browser.pause(1000);

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
