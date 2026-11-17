/**
 * Suggested data-grid screenshots for article/marketing use.
 *
 * Captures the scenarios recommended in the v0.3.0 data-grid writing guide:
 * staged edits + pending-changes bar, pending-plan preview dialog, filter
 * builder (per-condition AND/OR + incomplete guard), multi-column sort,
 * FK arrows + jump breadcrumb, copy-as menu, column-stats dialog, and value
 * presentation settings. Runs against the seeded demo PostgreSQL database
 * (`datazen_demo`) and never mutates it durably (staged edits are rolled
 * back, filters/sorts cleared, dialogs closed).
 *
 * Excluded from plain `pnpm e2e` by the `*screenshot*` name pattern. Run:
 *   pnpm e2e:skip-build -- --capture --spec specs/data-grid-suggested-screenshots.ts
 * or add a suite entry for `data-grid-suggested-shots`.
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
const OUT = path.join(ROOT, 'e2e', 'screenshots', 'data-grid-suggested');

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
          __TAURI_INTERNALS__: { invoke: (cmd: string, args: unknown) => Promise<unknown> };
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

async function demoNodeExpanded(dbName: string, kind: 'db' | 'schema') {
  return browser.execute(
    (name: string, k: string) => {
      const all = Array.from(document.querySelectorAll<HTMLElement>('button[data-tree-node]'));
      const dbIdx = all.findIndex(
        (n) => n.getAttribute('data-tree-node') === 'db' && n.getAttribute('data-db-name') === name,
      );
      if (dbIdx < 0) return false;
      const target = all.slice(dbIdx).find((n) => {
        if (k === 'db')
          return n.getAttribute('data-tree-node') === 'db' && n.getAttribute('data-db-name') === name;
        return n.getAttribute('data-tree-node') === 'schema';
      });
      return !!target?.querySelector('svg.lucide-chevron-down');
    },
    dbName,
    kind,
  );
}

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
      (n) => n.getAttribute('data-tree-node') === 'schema' && n.getAttribute('data-schema-name') === 'public',
    );
  }, dbName);
}

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
      (n) => n.getAttribute('data-tree-node') === 'table' && (n.getAttribute('data-item-name') || '').trim() === 'demo_sales',
    );
  }, dbName);
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
      if (k === 'db') target = all[dbIdx];
      else if (k === 'schema') target = subtree.find((n) => n.getAttribute('data-tree-node') === 'schema');
      else
        target = subtree.find(
          (n) => n.getAttribute('data-tree-node') === 'category' && n.getAttribute('data-cat-id') === 'tables',
        );
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

async function expandDemoDbTables(dbName: string) {
  if (dbName === DEMO_PG_DB) await pinDemoPgDatabase();
  await browser.waitUntil(
    async () =>
      browser.execute((cn: string) => {
        const item = Array.from(document.querySelectorAll<HTMLElement>('[data-conn-item]')).find((el) =>
          (el.getAttribute('data-conn-name') || '').includes(cn),
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
      (b) => !!b.querySelector('svg.lucide-chevron-right') || !!b.querySelector('svg.lucide-chevron-down'),
    );
    if (chev?.querySelector('svg.lucide-chevron-right')) chev.click();
  }, DEMO_PG_CONN_NAME);
  await browser.waitUntil(
    async () =>
      browser.execute(
        (name: string) =>
          Array.from(document.querySelectorAll<HTMLElement>('button[data-tree-node]')).findIndex(
            (n) => n.getAttribute('data-tree-node') === 'db' && n.getAttribute('data-db-name') === name,
          ) >= 0,
        dbName,
      ),
    { timeout: 20000, timeoutMsg: `${dbName} db node not rendered` },
  );
  if (!(await demoSchemaVisible(dbName))) {
    if (!(await demoNodeExpanded(dbName, 'db'))) await clickDemoDbNode(dbName, 'db');
    await browser.waitUntil(async () => demoSchemaVisible(dbName), {
      timeout: 20000,
      timeoutMsg: `public schema did not appear under ${dbName}`,
    });
  }
  if (!(await demoTablesVisible(dbName))) {
    if (!(await demoNodeExpanded(dbName, 'schema'))) await clickDemoDbNode(dbName, 'schema');
    if (!(await demoTablesVisible(dbName))) await clickDemoDbNode(dbName, 'category');
  }
  await browser.waitUntil(async () => demoTablesVisible(dbName), {
    timeout: 20000,
    timeoutMsg: `table rows did not appear under ${dbName}`,
  });
  await browser.pause(400);
}

async function openDemoTable(tableName: string, minRows = 3) {
  await goToConnections();
  await expandDemoDbTables(DEMO_PG_DB);
  await pinDemoPgDatabase();
  let opened = false;
  const dl = Date.now();
  while (Date.now() - dl < 15000 && !opened) {
    opened = await browser.execute((tbl: string) => {
      const el = Array.from(
        document.querySelectorAll<HTMLElement>(`button[data-tree-node="table"][data-item-name="${tbl}"]`),
      )[0];
      if (!el) {
        const scrollers = Array.from(
          document.querySelectorAll<HTMLElement>('[class*="overflow-y-auto"], [class*="overflow-auto"]'),
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
    const editor = document.querySelector('[data-testid="table-edit-input"]') as HTMLInputElement | null;
    if (!editor) return;
    editor.dispatchEvent(new FocusEvent('focusout', { bubbles: true }));
    editor.blur();
  });
  await browser.waitUntil(async () => !(await $('[data-testid="table-edit-input"]').isExisting()), {
    timeout: 8000,
    timeoutMsg: `等待编辑框提交并关闭超时（${label} → ${value}）`,
  });
  await browser.pause(300);
}

async function stageCell(rowIndex: number, columnName: string, value: string) {
  const selector = `[data-dt-row="${rowIndex}"][data-dt-col="${columnName}"]`;
  await $(selector).waitForDisplayed({ timeout: 8000 });
  await browser.execute((sel: string) => {
    const el = document.querySelector(sel);
    if (!el) return;
    el.dispatchEvent(new MouseEvent('dblclick', { bubbles: true, cancelable: true, composed: true }));
  }, selector);
  await browser.pause(500);
  await commitStagedValue(`${columnName}[${rowIndex}]`, value);
}

async function rollbackPendingIfAny() {
  const rollback = await $('[data-testid="pending-rollback"]');
  if (await rollback.isExisting().catch(() => false)) {
    await rollback.click();
    await browser.waitUntil(async () => (await dirtyCellCount()) === 0, {
      timeout: 8000,
      timeoutMsg: '回滚后脏标记未消失',
    }).catch(() => {});
  }
}

async function openFilterPanel() {
  const toggle = await $('[data-testid="table-filter-toggle"]');
  await toggle.waitForDisplayed({ timeout: 10000 });
  const pressed = await toggle.getAttribute('aria-pressed');
  if (pressed !== 'true') {
    await toggle.click();
    await browser.pause(400);
  }
  await $('[data-testid="filter-editor"]').waitForDisplayed({ timeout: 5000 });
}

async function resetFilterEditor() {
  const clear = await $('[data-testid="filter-clear"]');
  if (await clear.isExisting().catch(() => false)) {
    await clear.click().catch(() => {});
    await browser.pause(300);
  }
  const toggle = await $('[data-testid="table-filter-toggle"]');
  if ((await toggle.getAttribute('aria-pressed')) === 'true') {
    await toggle.click().catch(() => {});
    await browser.pause(300);
  }
}

async function conditionCount(): Promise<number> {
  return browser.execute(
    () =>
      new Set(
        Array.from(document.querySelectorAll('[data-filter-index]')).map((el) => el.getAttribute('data-filter-index')),
      ).size,
  );
}

async function addCondition(): Promise<number> {
  const before = await conditionCount();
  await $('[data-testid="filter-add"]').click();
  await browser.waitUntil(async () => (await conditionCount()) > before, {
    timeout: 8000,
    timeoutMsg: '新增筛选条件后行数未增加',
  });
  return before;
}

async function setConditionColumn(index: number, column: string) {
  const row = await $(`[data-filter-index="${index}"]`);
  const triggers = await row.$$('button[aria-haspopup="listbox"]');
  if (triggers.length < 2) throw new Error(`条件 ${index} 的下拉触发器不足: ${triggers.length}`);
  await triggers[0].click();
  await browser.waitUntil(
    async () =>
      browser.execute(
        () => Boolean(document.querySelector('[data-testid="select-listbox"]') ?? document.querySelector('[id^="dz-select-listbox-"]')),
      ),
    { timeout: 5000, timeoutMsg: `条件 ${index} 的列下拉未打开` },
  );
  await browser.execute((label: string) => {
    const list = document.querySelector('[data-testid="select-listbox"]') ?? document.querySelector('[id^="dz-select-listbox-"]');
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
  await browser.pause(200);
}

async function removeIncompleteRows() {
  for (let guard = 0; guard < 6; guard++) {
    const hasIncomplete = await browser.execute(() => Boolean(document.querySelector('[data-testid="filter-incomplete"]')));
    if (!hasIncomplete) return;
    const removeBtns = await $$(`button[aria-label="${t('filter.remove')}"]`);
    if (removeBtns.length === 0) return;
    await removeBtns[removeBtns.length - 1].click();
    await browser.pause(250);
  }
}

describe('data grid suggested screenshots', () => {
  let backedUpConnections: ConnectionConfig[] = [];

  before(async () => {
    await ensureMaximized();
    backedUpConnections = await invoke<ConnectionConfig[]>('get_connections');
    for (const c of backedUpConnections) await invoke('delete_connection', { id: c.id });
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
      throw new Error(`save_connection failed: ${JSON.stringify(saved)}`);
    }
    await pinDemoPgDatabase();
    await browser.url('tauri://localhost');
    await browser.pause(2000);
  });

  after(async () => {
    try {
      await invoke('delete_connection', { id: DEMO_PG_CONN_ID });
      for (const config of backedUpConnections) await invoke('save_connection', { config });
      await browser.url('tauri://localhost');
      await browser.pause(1200);
    } catch (e) {
      console.error('[restore] failed:', e);
    }
  });

  it('01 staged edits and pending-changes bar', async () => {
    await openDemoTable('demo_customers');
    await stageCell(1, 'points', '9999');
    await browser.waitUntil(async () => (await dirtyCellCount()) === 1, { timeout: 8000 });
    await browser.execute(() => {
      const el = document.querySelector('[data-dt-row="0"][data-dt-col="name"]');
      el?.dispatchEvent(new MouseEvent('dblclick', { bubbles: true, cancelable: true, composed: true }));
    });
    await $('[data-testid="table-edit-input"]').waitForDisplayed({ timeout: 8000 });
    await shot('01-staged-edit-pending-bar.png');
    await browser.keys(['Escape']);
    await browser.pause(300);
    await rollbackPendingIfAny();
  });

  it('02 pending plan preview dialog', async () => {
    await openDemoTable('demo_customers');
    await stageCell(1, 'points', '8888');
    await browser.waitUntil(async () => (await dirtyCellCount()) === 1, { timeout: 8000 });
    const previewBtn = await $('[data-testid="pending-preview"]');
    await previewBtn.waitForDisplayed({ timeout: 8000 });
    await previewBtn.click();
    // PendingPlanDialog is a modal; wait for a dialog content container
    await browser.waitUntil(
      async () => browser.execute(() => !!document.querySelector('[role="dialog"], [data-radix-dialog-content], .fixed')),
      { timeout: 8000, timeoutMsg: 'preview dialog did not open' },
    );
    await shot('02-pending-plan-preview.png');
    await browser.keys(['Escape']);
    await browser.pause(400);
    await rollbackPendingIfAny();
  });

  it('03 filter builder with per-condition AND/OR and incomplete guard', async () => {
    await openDemoTable('demo_customers');
    await openFilterPanel();
    await removeIncompleteRows();
    const first = await addCondition();
    await setConditionColumn(first, 'region');
    await fillConditionValue(first, '华东');
    const second = await addCondition();
    await setConditionColumn(second, 'points');
    // leave value empty on purpose to show incomplete guard state when applicable
    await browser.pause(400);
    // try switching connector to OR for visual contrast
    const orBtn = await $('[data-filter-index="1"][data-testid="filter-connector-or"]');
    if (await orBtn.isExisting().catch(() => false)) await orBtn.click().catch(() => {});
    await browser.pause(400);
    await shot('03-filter-builder-and-or-incomplete.png');
    await resetFilterEditor();
  });

  it('04 header sort menu (multi-column capable)', async () => {
    await openDemoTable('demo_customers');
    const header = await $('[data-col-header="points"]');
    const sortButton = await header.$('[data-sort-menu]');
    await sortButton.click();
    const ascending = await $('[data-testid="web-context-item-sort-ascending"]');
    await ascending.waitForDisplayed({ timeout: 5000 });
    await ascending.click();
    await browser.pause(500);
    const headerAgain = await $('[data-col-header="points"]');
    await (await headerAgain.$('[data-sort-menu]')).click();
    const removeSort = await $('[data-testid="web-context-item-remove-sort"]');
    await removeSort.waitForDisplayed({ timeout: 5000 });
    await shot('04-header-sort-menu.png');
    await removeSort.click().catch(() => {});
    await browser.pause(300);
  });

  it('05 FK arrows and jump breadcrumb', async () => {
    await openDemoTable('demo_order_items');
    await browser.waitUntil(
      async () => browser.execute(() => document.querySelectorAll('[data-dt-fk]').length > 0),
      { timeout: 15000, timeoutMsg: '外键单元格未渲染' },
    );
    await shot('05-fk-arrows.png');
    // click first visible FK jump
    const jumped = await browser.execute(() => {
      const entries = Array.from(document.querySelectorAll<HTMLElement>('[data-dt-fk-click="true"]')).filter(
        (el) => el.offsetParent !== null,
      );
      if (!entries[0]) return false;
      entries[0].dispatchEvent(new MouseEvent('click', { bubbles: true, cancelable: true }));
      return true;
    });
    if (jumped) {
      const ok = await $('[data-testid="table-fk-breadcrumb"]')
        .waitForDisplayed({ timeout: 12000 })
        .then(() => true)
        .catch(() => false);
      if (ok) {
        await shot('05-fk-jump-breadcrumb.png');
        const back = await $('[data-testid="table-fk-back"]');
        if (await back.isExisting()) await back.click().catch(() => {});
        await browser.pause(600);
      }
    }
  });

  it('06 copy selected cells as submenu', async () => {
    await openDemoTable('demo_customers');
    // select a small cell range via shift-click
    const c1 = await $('[data-dt-row="0"][data-dt-col="name"]');
    const c2 = await $('[data-dt-row="1"][data-dt-col="region"]');
    if (await c1.isExisting() && await c2.isExisting()) {
      await c1.click();
      await browser.performActions([
        { type: 'key', id: 'kbd', actions: [{ type: 'keyDown', value: '\uE008' }] }, // Shift
      ]).catch(() => {});
      await c2.click().catch(() => {});
      await browser.performActions([
        { type: 'key', id: 'kbd', actions: [{ type: 'keyUp', value: '\uE008' }] },
      ]).catch(() => {});
      await browser.pause(400);
      // open context menu on selection
      await browser.execute(() => {
        const el = document.querySelector('[data-dt-row="0"][data-dt-col="name"]');
        el?.dispatchEvent(new MouseEvent('contextmenu', { bubbles: true, cancelable: true, clientX: 300, clientY: 300 }));
      });
      await browser.pause(500);
      // try to hover/open copy-as submenu by clicking/hovering its item
      await browser.execute(() => {
        const items = Array.from(document.querySelectorAll<HTMLElement>('[role="menuitem"], [data-testid^="web-context-item"]'));
        const copyAs = items.find((el) => /复制.*为|Copy.*as/i.test(el.textContent || '') || (el.getAttribute('data-testid') || '').includes('copy-selected-cells-as'));
        copyAs?.dispatchEvent(new MouseEvent('mouseenter', { bubbles: true }));
        copyAs?.dispatchEvent(new MouseEvent('mouseover', { bubbles: true }));
        copyAs?.click();
      });
      await browser.pause(600);
      await shot('06-copy-selected-cells-as-menu.png');
      await browser.keys(['Escape']);
      await browser.pause(300);
    }
  });

  it('07 column stats dialog', async () => {
    await openDemoTable('demo_customers');
    // open header menu on a numeric column and click column stats
    const header = await $('[data-col-header="points"]');
    await header.click({ button: 'right' }).catch(async () => {
      await browser.execute(() => {
        document.querySelector('[data-col-header="points"]')?.dispatchEvent(
          new MouseEvent('contextmenu', { bubbles: true, cancelable: true, clientX: 400, clientY: 200 }),
        );
      });
    });
    await browser.pause(500);
    const clicked = await browser.execute(() => {
      const items = Array.from(document.querySelectorAll<HTMLElement>('[role="menuitem"], button, [data-testid^="web-context-item"]'));
      const el = items.find((i) => /列统计|Column stats|columnStats/i.test(i.textContent || '') || /column.?stat/i.test(i.getAttribute('data-testid') || ''));
      if (!el) return false;
      el.click();
      return true;
    });
    if (clicked) {
      await browser.waitUntil(
        async () => browser.execute(() => /列的统计|列统计|Column|统计/i.test(document.body.innerText) && !!document.querySelector('[role="dialog"], [data-radix-dialog-content]')),
        { timeout: 12000, timeoutMsg: 'column stats dialog not shown' },
      ).catch(() => {});
      await shot('07-column-stats.png');
      await browser.keys(['Escape']);
      await browser.pause(300);
    }
  });

  it('08 value presentation settings', async () => {
    await openSettingsInMainWindow('dataBrowsing');
    const section = await $('[data-testid="value-presentation-settings"]');
    await section.scrollIntoView({ block: 'center' });
    await section.waitForDisplayed({ timeout: 10000 });
    await shot('08-value-presentation-settings.png');
    await backFromSettingsInMainWindow();
  });
});
