/**
 * Data-browsing release screenshots (v0.3.0: DB-05/06/07, DBR-04, issue-32).
 *
 * Companion to zz-screenshots.ts: captures the v0.3.0 data-browsing feature
 * highlights into e2e/screenshots/data-browsing/ via the WebDriver screenshot
 * API. Runs against the seeded demo PostgreSQL database (`datazen_demo`, role
 * `datazen_demo`) so the frames show real demo data instead of `_e2e_*`
 * fixtures.
 *
 * The file name matches the `*screenshot*` exclude pattern, so a plain
 * `pnpm e2e` never runs it. Opt in with --capture:
 *
 *   pnpm e2e:shots:data-browsing
 *   # = node e2e/run.mjs --skip-build --capture -- --suite data-browsing-shots
 *
 * Demo data is never mutated durably: every staged cell edit is discarded
 * through the pending-changes rollback, and no filter or sort is applied —
 * the frames only ever show editor/menu/panel state on top of pristine rows.
 */
import { browser, $, $$ } from '@wdio/globals';
import fs from 'node:fs';
import path from 'node:path';
import url from 'node:url';
import { ensureMaximized } from '../lib/capture-window';
import {
  backFromSettingsInMainWindow,
  dbSessionIdOf,
  openSettingsInMainWindow,
} from '../helpers.js';
import { readCatalog } from '../schema-metadata.js';
import { t } from '../i18n';

const ROOT = path.resolve(path.dirname(url.fileURLToPath(import.meta.url)), '..', '..');
const OUT = path.join(ROOT, 'e2e', 'screenshots', 'data-browsing');

const DEMO_PG_DB = process.env.E2E_DEMO_PG_DB || 'datazen_demo';
const DEMO_PG_CONN_NAME = '演示 PostgreSQL';
const DEMO_PG_CONN_ID = 'conn_demo_pg';

interface ConnectionConfig {
  id: string;
  name: string;
  databaseType: string;
  host: string;
  port: number;
  username: string;
  password: string;
  database: string;
  group?: string;
  colorTag?: string;
  sslMode?: string;
  [key: string]: unknown;
}

fs.mkdirSync(OUT, { recursive: true });

async function shot(name: string, settleMs = 800) {
  await browser.pause(settleMs);
  await browser.saveScreenshot(path.join(OUT, name));
  const size = fs.statSync(path.join(OUT, name)).size;
  console.log(`[shot] ${name} (${size} bytes)`);
}

async function invoke<T = unknown>(cmd: string, args: Record<string, unknown> = {}): Promise<T> {
  return browser.executeAsync(
    (c: string, a: string, done: (r: unknown) => void) => {
      (
        window as unknown as {
          __TAURI_INTERNALS__: {
            invoke: (cmd: string, args: unknown) => Promise<unknown>;
          };
        }
      ).__TAURI_INTERNALS__
        .invoke(c, JSON.parse(a))
        .then((r: unknown) => done(r ?? 'ok'))
        .catch((e: unknown) => done(`ERR: ${String(e)}`));
    },
    cmd,
    JSON.stringify(args),
  ) as Promise<T>;
}

async function countDtRows() {
  // `[data-dt-row]` rides on every **cell**, so counting nodes over-reports by
  // the column count. Count DISTINCT row indices instead (see zz-screenshots).
  return browser.execute(() => {
    const indices = new Set<string>();
    document.querySelectorAll('[data-dt-row]').forEach((el) => {
      indices.add(el.getAttribute('data-dt-row') ?? '');
    });
    return indices.size || document.querySelectorAll('table tbody tr').length;
  });
}

async function waitResults(minRows = 1, timeout = 20000) {
  await browser.waitUntil(async () => (await countDtRows()) >= minRows, {
    timeout,
    timeoutMsg: `结果行数未达到 ${minRows}`,
  });
  await browser.pause(500);
}

async function goToConnections() {
  await browser.execute(() => {
    document
      .querySelector('[data-testid="workspace-nav-databases"]')
      ?.dispatchEvent(new MouseEvent('click', { bubbles: true }));
  });
  await browser.pause(800);
}

async function pinDemoPgDatabase() {
  const connId = dbSessionIdOf(await invoke('connect', { connectionId: DEMO_PG_CONN_ID }));
  if (typeof connId !== 'string' || connId.startsWith('__error')) {
    throw new Error(`connect(${DEMO_PG_CONN_NAME}) failed: ${JSON.stringify(connId)}`);
  }
  const tables = await readCatalog({ dbSessionId: connId, database: DEMO_PG_DB });
  if (tables && typeof tables === 'object' && '__error' in (tables as object)) {
    throw new Error(`list_catalog(${DEMO_PG_DB}) failed: ${JSON.stringify(tables)}`);
  }
  await browser.pause(300);
}

/** True when `public` schema node is visible under the given database subtree. */
async function demoSchemaVisible(dbName: string) {
  return browser.execute((name: string) => {
    const all = Array.from(document.querySelectorAll<HTMLElement>('button[data-tree-node]'));
    const i = all.findIndex(
      (n) => n.getAttribute('data-tree-node') === 'db' && n.getAttribute('data-db-name') === name,
    );
    if (i < 0) return false;
    const rest = all.slice(i + 1);
    const nextDb = rest.findIndex((n) => n.getAttribute('data-tree-node') === 'db');
    const subtree = nextDb >= 0 ? rest.slice(0, nextDb) : rest;
    return subtree.some(
      (n) =>
        n.getAttribute('data-tree-node') === 'schema' &&
        n.getAttribute('data-schema-name') === 'public',
    );
  }, dbName);
}

/** True when a tree node shows expanded state (chevron-down). */
async function demoNodeExpanded(dbName: string, kind: 'db' | 'schema') {
  return browser.execute(
    (name: string, k: string) => {
      const all = Array.from(document.querySelectorAll<HTMLElement>('button[data-tree-node]'));
      const dbIdx = all.findIndex(
        (n) => n.getAttribute('data-tree-node') === 'db' && n.getAttribute('data-db-name') === name,
      );
      if (dbIdx < 0) return false;
      const target = all.slice(dbIdx).find((n) => {
        if (k === 'db') {
          return (
            n.getAttribute('data-tree-node') === 'db' && n.getAttribute('data-db-name') === name
          );
        }
        return n.getAttribute('data-tree-node') === 'schema';
      });
      return !!target?.querySelector('svg.lucide-chevron-down');
    },
    dbName,
    kind,
  );
}

async function clickDemoDbNode(dbName: string, kind: 'db' | 'schema' | 'category') {
  await browser.execute(
    (name: string, k: string) => {
      const all = Array.from(document.querySelectorAll<HTMLElement>('button[data-tree-node]'));
      const dbIdx = all.findIndex(
        (n) => n.getAttribute('data-tree-node') === 'db' && n.getAttribute('data-db-name') === name,
      );
      if (dbIdx < 0) return false;
      const rest = all.slice(dbIdx + 1);
      const nextDb = rest.findIndex((n) => n.getAttribute('data-tree-node') === 'db');
      const subtree = nextDb >= 0 ? rest.slice(0, nextDb) : rest;

      let target: HTMLElement | undefined;
      if (k === 'db') {
        target = all[dbIdx];
      } else if (k === 'schema') {
        target = subtree.find((n) => n.getAttribute('data-tree-node') === 'schema');
      } else {
        target = subtree.find(
          (n) =>
            n.getAttribute('data-tree-node') === 'category' &&
            n.getAttribute('data-cat-id') === 'tables',
        );
      }
      if (!target) return false;
      target.scrollIntoView({ block: 'center' });
      target.click();
      return true;
    },
    dbName,
    kind,
  );
  await browser.pause(350);
}

/** True when demo_sales is visible under the given demo database subtree. */
async function demoTablesVisible(dbName: string) {
  return browser.execute((name: string) => {
    const all = Array.from(document.querySelectorAll<HTMLElement>('button[data-tree-node]'));
    const dbIdx = all.findIndex(
      (n) => n.getAttribute('data-tree-node') === 'db' && n.getAttribute('data-db-name') === name,
    );
    if (dbIdx < 0) return false;
    const rest = all.slice(dbIdx + 1);
    const nextDb = rest.findIndex((n) => n.getAttribute('data-tree-node') === 'db');
    const subtree = nextDb >= 0 ? rest.slice(0, nextDb) : rest;
    return subtree.some(
      (n) =>
        n.getAttribute('data-tree-node') === 'table' &&
        (n.getAttribute('data-item-name') || '').trim() === 'demo_sales',
    );
  }, dbName);
}

/** Expand a demo database → public schema → 表 category in the virtualized tree. */
async function expandDemoDbTables(dbName: string) {
  if (dbName === DEMO_PG_DB) {
    await pinDemoPgDatabase();
  }
  await browser.waitUntil(
    async () =>
      browser.execute((cn: string) => {
        const item = Array.from(document.querySelectorAll<HTMLElement>('[data-conn-item]')).find(
          (el) => (el.getAttribute('data-conn-name') || '').includes(cn),
        );
        return !!item;
      }, DEMO_PG_CONN_NAME),
    { timeout: 20000, timeoutMsg: `${DEMO_PG_CONN_NAME} not listed in the navigator` },
  );
  await browser.execute((cn: string) => {
    const item = Array.from(document.querySelectorAll<HTMLElement>('[data-conn-item]')).find((el) =>
      (el.getAttribute('data-conn-name') || '').includes(cn),
    );
    const chev = Array.from(item?.querySelectorAll<HTMLElement>('button') ?? []).find(
      (b) =>
        !!b.querySelector('svg.lucide-chevron-right') ||
        !!b.querySelector('svg.lucide-chevron-down'),
    );
    if (chev?.querySelector('svg.lucide-chevron-right')) chev.click();
  }, DEMO_PG_CONN_NAME);

  const dbIndex = () =>
    browser.execute((name: string) => {
      const all = Array.from(document.querySelectorAll<HTMLElement>('button[data-tree-node]'));
      return all.findIndex(
        (n) => n.getAttribute('data-tree-node') === 'db' && n.getAttribute('data-db-name') === name,
      );
    }, dbName);

  await browser.waitUntil(async () => (await dbIndex()) >= 0, {
    timeout: 20000,
    timeoutMsg: `${dbName} db node not rendered`,
  });

  if (!(await demoSchemaVisible(dbName))) {
    if (!(await demoNodeExpanded(dbName, 'db'))) {
      await clickDemoDbNode(dbName, 'db');
    }
    await browser.waitUntil(async () => demoSchemaVisible(dbName), {
      timeout: 20000,
      timeoutMsg: `public schema did not appear under ${dbName}`,
    });
  }

  if (!(await demoTablesVisible(dbName))) {
    if (!(await demoNodeExpanded(dbName, 'schema'))) {
      await clickDemoDbNode(dbName, 'schema');
    }
    if (!(await demoTablesVisible(dbName))) {
      await clickDemoDbNode(dbName, 'category');
    }
  }
  await browser.waitUntil(async () => demoTablesVisible(dbName), {
    timeout: 20000,
    timeoutMsg: `table rows did not appear under ${dbName}`,
  });
  await browser.pause(400);
}

/** Single-click a table node to open its data grid, then wait for rows. */
async function openDemoTable(tableName: string, minRows = 3) {
  await goToConnections();
  await expandDemoDbTables(DEMO_PG_DB);
  await pinDemoPgDatabase();

  let opened = false;
  const dl = Date.now();
  while (Date.now() - dl < 15000 && !opened) {
    opened = await browser.execute((tbl: string) => {
      const el = Array.from(
        document.querySelectorAll<HTMLElement>(
          `button[data-tree-node="table"][data-item-name="${tbl}"]`,
        ),
      )[0];
      if (!el) {
        // Virtualized list: the node may be out of the window — scroll back up.
        const scrollers = Array.from(
          document.querySelectorAll<HTMLElement>(
            '[class*="overflow-y-auto"], [class*="overflow-auto"]',
          ),
        ).filter((s) => s.scrollHeight > s.clientHeight);
        for (const s of scrollers) s.scrollTop = Math.max(0, s.scrollTop - 300);
        return !!document.querySelector(`button[data-tree-node="table"][data-item-name="${tbl}"]`);
      }
      el.scrollIntoView({ block: 'center' });
      el.dispatchEvent(new MouseEvent('click', { bubbles: true, cancelable: true }));
      return true;
    }, tableName);
    if (!opened) await browser.pause(250);
  }
  if (!opened) throw new Error(`${tableName} not found in sidebar`);

  await waitResults(minRows);
}

/** The number of cells the grid paints as dirty. */
async function dirtyCellCount(): Promise<number> {
  return browser.execute(() => document.querySelectorAll('[data-dt-dirty="true"]').length);
}

async function commitStagedValue(label: string, value: string) {
  const input = await $('[data-testid="table-edit-input"]');
  await input.waitForDisplayed({ timeout: 8000 });
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
  await browser.waitUntil(async () => !(await $('[data-testid="table-edit-input"]').isExisting()), {
    timeout: 8000,
    timeoutMsg: `等待编辑框提交并关闭超时（${label} → ${value}）`,
  });
  await browser.pause(300);
}

/** Stage a value into one cell addressed by row/column (see table-dirty-cells.ts). */
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

// ── filter-editor helpers (adapted from table-filter.ts) ─────────────────

/** Open the filter editor the robust way (see table-filter.ts openFilterPanel). */
async function openFilterPanel() {
  const toggle = await $('[data-testid="table-filter-toggle"]');
  await toggle.waitForDisplayed({ timeout: 10000 });
  const pressed = await toggle.getAttribute('aria-pressed');
  if (pressed !== 'true') {
    await toggle.click();
    await browser.pause(400);
  }
  const editor = await $('[data-testid="filter-editor"]');
  if (!(await editor.isDisplayed().catch(() => false))) {
    await toggle.click();
    await browser.pause(400);
  }
  await $('[data-testid="filter-editor"]').waitForDisplayed({ timeout: 5000 });
  // Ensure panel body is expanded
  const apply = await $('[data-testid="filter-apply"]');
  const anyApply = await $('[data-testid="any-column-apply"]');
  const rawApply = await $('[data-testid="raw-where-apply"]');
  const bodyExpanded =
    (await apply.isDisplayed().catch(() => false)) ||
    (await anyApply.isDisplayed().catch(() => false)) ||
    (await rawApply.isDisplayed().catch(() => false));
  if (!bodyExpanded) {
    const summary = await $('[data-testid="filter-summary-toggle"]');
    if (await summary.isExisting()) {
      await summary.click();
      await browser.pause(300);
    }
  }
}

async function closeFilterPanel() {
  const toggle = await $('[data-testid="table-filter-toggle"]');
  const pressed = await toggle.getAttribute('aria-pressed');
  if (pressed === 'true') {
    await toggle.click();
    await browser.pause(300);
  }
}

async function conditionCount(): Promise<number> {
  return browser.execute(
    () =>
      new Set(
        Array.from(document.querySelectorAll('[data-filter-index]')).map((el) =>
          el.getAttribute('data-filter-index'),
        ),
      ).size,
  );
}

/** Append one condition and return its index. */
async function addCondition(): Promise<number> {
  const before = await conditionCount();
  await $('[data-testid="filter-add"]').click();
  await browser.waitUntil(async () => (await conditionCount()) > before, {
    timeout: 8000,
    timeoutMsg: '新增筛选条件后行数未增加',
  });
  const index = before;
  const input = await $(`[data-filter-index="${index}"] [data-testid="filter-value"]`);
  if (!(await input.isExisting().catch(() => false))) {
    await $(`[data-filter-index="${index}"] button[title="${t('filter.editCondition')}"]`).click();
    await browser.pause(300);
  }
  await $(`[data-filter-index="${index}"] button[aria-haspopup="listbox"]`).waitForDisplayed({
    timeout: 8000,
    timeoutMsg: `条件 ${index} 展开后没有列选择器`,
  });
  return index;
}

/** Set one condition's column through its own row's listbox. */
async function setConditionColumn(index: number, column: string) {
  const row = await $(`[data-filter-index="${index}"]`);
  const triggers = await row.$$('button[aria-haspopup="listbox"]');
  if (triggers.length < 2) throw new Error(`条件 ${index} 的下拉触发器不足: ${triggers.length}`);
  await triggers[0].click();
  await browser.waitUntil(
    async () =>
      browser.execute(() =>
        Boolean(
          document.querySelector('[data-testid="select-listbox"]') ??
            document.querySelector('[id^="dz-select-listbox-"]'),
        ),
      ),
    { timeout: 5000, timeoutMsg: `条件 ${index} 的列下拉未打开` },
  );
  await browser.execute((label: string) => {
    const list =
      document.querySelector('[data-testid="select-listbox"]') ??
      document.querySelector('[id^="dz-select-listbox-"]');
    if (!list) throw new Error('listbox not open');
    const item = Array.from(list.children).find((el) => (el.textContent ?? '').includes(label));
    if (!item) throw new Error(`option not found: ${label}`);
    item.dispatchEvent(new MouseEvent('mousedown', { bubbles: true, cancelable: true }));
  }, column);
  await browser.pause(300);
}

async function fillConditionValue(index: number, value: string) {
  const input = await $(`[data-filter-index="${index}"] [data-testid="filter-value"]`);
  await input.waitForDisplayed({ timeout: 5000 });
  await input.clearValue();
  await input.setValue(value);
  await browser.waitUntil(async () => (await input.getValue()) === value, {
    timeout: 5000,
    timeoutMsg: `条件 ${index} 的值未写入: ${value}`,
  });
  await browser.pause(200);
}

/**
 * Drop the blank condition the panel seeds when it opens (the incomplete
 * `id =` draft row, see table-filter.ts removeIncompleteRows).
 */
async function removeIncompleteRows() {
  for (let guard = 0; guard < 6; guard++) {
    const hasIncomplete = await browser.execute(() =>
      Boolean(document.querySelector('[data-testid="filter-incomplete"]')),
    );
    if (!hasIncomplete) return;
    const removeBtns = await $$(`button[aria-label="${t('filter.remove')}"]`);
    if (removeBtns.length === 0) return;
    await removeBtns[removeBtns.length - 1].click();
    await browser.pause(250);
  }
}

/** Discard every staged filter/condition without ever applying it. */
async function resetFilterEditor() {
  const clear = await $('[data-testid="filter-clear"]');
  if (await clear.isExisting().catch(() => false)) {
    await clear.click();
    await browser.pause(300);
  }
  // A cleared panel may re-seed one blank draft row — drop it too, best
  // effort. No strict assertion: the next case re-opens the panel anyway.
  await removeIncompleteRows().catch(() => {});
  // Collapse the panel again (best effort).
  const toggle = await $('[data-testid="table-filter-toggle"]');
  await toggle.click().catch(() => {});
  await browser.pause(300);
}

/** The row/col of the first visibly rendered FK jump entry. */
async function firstVisibleFkEntry(): Promise<{ row: string; col: string } | null> {
  return browser.execute(() => {
    const entries = Array.from(
      document.querySelectorAll<HTMLElement>('[data-dt-fk-click="true"]'),
    ).filter((el) => el.offsetParent !== null);
    const cell = entries[0]?.closest<HTMLElement>('[data-dt-row][data-dt-col]');
    if (!cell) return null;
    return {
      row: cell.getAttribute('data-dt-row') ?? '',
      col: cell.getAttribute('data-dt-col') ?? '',
    };
  });
}

// ── screenshot cases ─────────────────────────────────────────────────────

describe('data browsing screenshots', () => {
  /** Snapshot of the persisted connections — restored in `after`. */
  let backedUpConnections: ConnectionConfig[] = [];

  before(async () => {
    // Frames come off a maximized window: 2x backing pixels, one size.
    await ensureMaximized();

    // The e2e app data dir is wiped per run, but be defensive anyway: swap in
    // demo-only state and restore whatever was there in `after`.
    backedUpConnections = await invoke<ConnectionConfig[]>('get_connections');
    for (const c of backedUpConnections) {
      await invoke('delete_connection', { id: c.id });
    }
    await invoke('delete_connection', { id: DEMO_PG_CONN_ID });
    await browser.pause(400);

    const saved = await invoke('save_connection', {
      config: {
        id: DEMO_PG_CONN_ID,
        name: DEMO_PG_CONN_NAME,
        databaseType: 'postgresql',
        host: process.env.E2E_PG_HOST || '127.0.0.1',
        port: Number(process.env.E2E_PG_PORT) || 5432,
        username: process.env.E2E_DEMO_PG_USER || 'datazen_demo',
        password: process.env.E2E_DEMO_PG_PASSWORD || 'datazen_demo',
        database: DEMO_PG_DB,
        group: 'preset:development',
        colorTag: '#3b82f6',
        sslMode: 'disable',
      },
    });
    if (saved && typeof saved === 'object' && '__error' in (saved as object)) {
      throw new Error(`save_connection(${DEMO_PG_CONN_ID}) failed: ${JSON.stringify(saved)}`);
    }
    await pinDemoPgDatabase();

    await browser.url('tauri://localhost');
    await browser.pause(2000);
    await browser.waitUntil(
      async () =>
        browser.execute(
          (connName: string) =>
            !!Array.from(document.querySelectorAll('[data-conn-item]')).find((el) =>
              (el.getAttribute('data-conn-name') || '').includes(connName),
            ),
          DEMO_PG_CONN_NAME,
        ),
      { timeout: 20000, timeoutMsg: `${DEMO_PG_CONN_NAME} not listed in tree after reload` },
    );
  });

  after(async () => {
    try {
      await invoke('delete_connection', { id: DEMO_PG_CONN_ID });
      for (const config of backedUpConnections) {
        await invoke('save_connection', { config });
      }
      await browser.url('tauri://localhost');
      await browser.pause(1200);
      console.log(`[restore] ${backedUpConnections.length} connection(s) restored`);
    } catch (e) {
      console.error('[restore] failed to restore connections:', e);
    }
  });

  it('typed cell editor + dirty marks (DB-05/DB-06)', async () => {
    await openDemoTable('demo_customers');

    // Stage one committed edit → the cell paints dirty and the pending bar
    // with its count appears. Nothing is ever written to the database.
    await stageCell(1, 'points', '9999');
    await browser.waitUntil(async () => (await dirtyCellCount()) === 1, {
      timeout: 8000,
      timeoutMsg: '脏单元格高亮未出现',
    });

    // Open the typed editor on another cell so the frame shows both states.
    await browser.execute(() => {
      const el = document.querySelector('[data-dt-row="0"][data-dt-col="name"]');
      el?.dispatchEvent(
        new MouseEvent('dblclick', { bubbles: true, cancelable: true, composed: true }),
      );
    });
    await $('[data-testid="table-edit-input"]').waitForDisplayed({ timeout: 8000 });
    await shot('db05-06-typed-edit-dirty.png');

    // Cleanup: close the editor, discard the staged edit.
    await browser.keys(['Escape']);
    await browser.pause(400);
    const rollback = await $('[data-testid="pending-rollback"]');
    await rollback.waitForDisplayed({ timeout: 5000 });
    await rollback.click();
    await browser.waitUntil(async () => (await dirtyCellCount()) === 0, {
      timeout: 8000,
      timeoutMsg: '回滚后脏标记未消失',
    });
  });

  it('Navicat-style header sort menu (three-state)', async () => {
    // demo_customers is the active tab from the previous case.
    const header = await $('[data-col-header="points"]');
    const sortButton = await header.$('[data-sort-menu]');
    await sortButton.click();
    const ascending = await $('[data-testid="web-context-item-sort-ascending"]');
    await ascending.waitForDisplayed({ timeout: 5000 });
    await ascending.click();
    // The sort state rides on the menu button itself (data-sort-state), not on
    // an icon class — the result grid and the table grid use different glyphs.
    await browser.waitUntil(
      async () =>
        browser.execute(
          () =>
            document
              .querySelector<HTMLElement>('[data-col-header="points"] [data-sort-menu]')
              ?.getAttribute('data-sort-state') === 'asc',
        ),
      { timeout: 10000, timeoutMsg: '升序排序指示器未出现' },
    );

    // Reopen the menu so the frame shows the applied state AND the menu with
    // its three-state actions (升序已选 / 降序 / 移除排序).
    const headerAgain = await $('[data-col-header="points"]');
    await (await headerAgain.$('[data-sort-menu]')).click();
    const removeSort = await $('[data-testid="web-context-item-remove-sort"]');
    await removeSort.waitForDisplayed({ timeout: 5000 });
    await shot('header-sort-menu.png');

    // Cleanup: remove the sort so the grid is pristine again.
    await removeSort.click();
    await browser.waitUntil(
      async () =>
        browser.execute(
          () =>
            document
              .querySelector<HTMLElement>('[data-col-header="points"] [data-sort-menu]')
              ?.getAttribute('data-sort-state') === 'none',
        ),
      { timeout: 8000, timeoutMsg: '移除排序后指示器未消失' },
    );
  });

  it('filter editor with per-condition connectors (DBR-04)', async () => {
    await openFilterPanel();
    // The panel seeds one blank draft row on open — drop it so the frame
    // shows only the conditions we build.
    await removeIncompleteRows();

    const first = await addCondition();
    await setConditionColumn(first, 'region');
    await fillConditionValue(first, '华东');
    const second = await addCondition();
    await setConditionColumn(second, 'points');
    await fillConditionValue(second, '1000');

    // Make the second row's connector explicit OR — the frame then shows the
    // per-condition AND/OR toggle in its "differs from request-wide" state
    // (accent border + OR highlighted). The click can land between re-renders
    // and vanish, so assert the wrapper state and retry before shooting.
    let connectorLit = false;
    for (let attempt = 0; attempt < 2 && !connectorLit; attempt++) {
      await (await $('[data-filter-index="1"][data-testid="filter-connector-or"]')).click();
      try {
        await browser.waitUntil(
          async () =>
            browser.execute(
              () =>
                document
                  .querySelector<HTMLElement>(
                    '[data-testid="filter-connector"][data-filter-index="1"]',
                  )
                  ?.getAttribute('data-connector') === 'or',
            ),
          { timeout: 4000 },
        );
        connectorLit = true;
      } catch {
        await browser.pause(500);
      }
    }
    if (!connectorLit) throw new Error('第二行连接符未能切换为 OR');
    await browser.pause(300);
    await shot('filter-connectors.png');

    await resetFilterEditor();
  });

  it('raw SQL filter pane (DBR-04)', async () => {
    // Fresh open: whatever state the previous case left, start from closed.
    const alreadyOpen = await $('[data-testid="filter-editor"]');
    if (await alreadyOpen.isDisplayed().catch(() => false)) {
      await closeFilterPanel();
      await browser.pause(300);
    }
    await openFilterPanel();

    // PostgreSQL declares rawWhereFilter; the tab renders only for drivers
    // that declare it (see table-filter.ts TC for the gated contract).
    await browser.waitUntil(
      async () =>
        browser.execute(() => !!document.querySelector('[data-testid="filter-mode-rawSql"]')),
      { timeout: 10000, timeoutMsg: 'Raw SQL 页签未渲染' },
    );
    await (await $('[data-testid="filter-mode-rawSql"]')).click();
    const input = await $('[data-testid="raw-where-input"]');
    await input.waitForDisplayed({ timeout: 5000 });
    await input.setValue("points >= 2000 AND region = '华东'");
    await browser.pause(400);
    await shot('raw-sql-filter.png');

    await resetFilterEditor();
  });

  it('FK arrows on cells (DB-07)', async () => {
    await openDemoTable('demo_order_items');
    await browser.waitUntil(
      async () => browser.execute(() => document.querySelectorAll('[data-dt-fk]').length > 0),
      { timeout: 15000, timeoutMsg: '外键单元格未渲染' },
    );
    await shot('fk-arrows.png');
  });

  it('FK jump seeds a visible filter with breadcrumb (DB-07)', async () => {
    await browser.waitUntil(async () => (await firstVisibleFkEntry()) !== null, {
      timeout: 15000,
      timeoutMsg: '外键跳转入口未渲染',
    });

    // A real WDIO click on a visible entry (the TC-FK-001 pattern), not a
    // synthetic MouseEvent. The probe behind the jump can lose a transient
    // race with the session warm-up, so allow one retry.
    let jumped = false;
    for (let attempt = 0; attempt < 2 && !jumped; attempt++) {
      const entry = await firstVisibleFkEntry();
      if (!entry) throw new Error('fk jump entry missing');
      await $(
        `[data-dt-row="${entry.row}"][data-dt-col="${entry.col}"] [data-dt-fk-click="true"]`,
      ).click();
      jumped = await $('[data-testid="table-fk-breadcrumb"]')
        .waitForDisplayed({ timeout: 12000 })
        .then(() => true)
        .catch(() => false);
      if (!jumped) await browser.pause(1500);
    }
    if (!jumped) throw new Error('FK jump did not reach the breadcrumb');
    await shot('fk-jump-breadcrumb.png');

    // Back to the source table so the workspace is clean afterwards.
    const back = await $('[data-testid="table-fk-back"]');
    await back.waitForDisplayed({ timeout: 8000 });
    await back.click();
    await browser.waitUntil(
      async () =>
        browser.execute(() => !document.querySelector('[data-testid="table-fk-breadcrumb"]')),
      { timeout: 10000, timeoutMsg: '返回后面包屑未消失' },
    );
  });

  it('value presentation settings (issue-32)', async () => {
    await openSettingsInMainWindow('dataBrowsing');
    const section = await $('[data-testid="value-presentation-settings"]');
    await section.scrollIntoView({ block: 'center' });
    await section.waitForDisplayed({ timeout: 10000 });
    await shot('value-presentation-settings.png');
    await backFromSettingsInMainWindow();
  });
});
