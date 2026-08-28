/**
 * Site screenshot generator (temporary tooling, not a regression spec).
 * Drives the webdriver-enabled app and captures marketing screenshots
 * directly into site/assets/screenshots/ via the WebDriver screenshot API.
 */
import { browser, $ } from '@wdio/globals';
import fs from 'node:fs';
import path from 'node:path';
import url from 'node:url';

const ROOT = path.resolve(path.dirname(url.fileURLToPath(import.meta.url)), '..', '..');
const OUT = path.join(ROOT, 'site', 'assets', 'screenshots');

/** Isolated demo databases — see e2e/setup-demo-data.sh */
const DEMO_PG_DB = process.env.E2E_DEMO_PG_DB || 'datazen_demo';
const DEMO_PG_DB2 = process.env.E2E_DEMO_PG_DB2 || 'datazen_demo_analytics';
const DEMO_MYSQL_DB = process.env.E2E_DEMO_MYSQL_DB || 'datazen_demo';
const DEMO_REDIS_DB = process.env.E2E_REDIS_DEMO_DB || 'db5';
const DEMO_PG_CONN_NAME = '演示 PostgreSQL';
const DEMO_MYSQL_CONN_NAME = '演示 MySQL · 物流';
const DEMO_REDIS_CONN_NAME = '演示 Redis';
const DEMO_PG_CONN_ID = 'conn_demo_pg';
const DEMO_MYSQL_CONN_ID = 'conn_demo_mysql';
const DEMO_REDIS_CONN_ID = 'conn_demo_redis';
const DEMO_WORKFLOW_ID = 'cross-db-order-logistics';
/** Dashboard seed (21-dashboard) — created in `before`, removed in `after`. */
const DEMO_DASHBOARD_ID = 'dash-demo-sales';
const DEMO_TREND_WF_ID = 'dash-sales-trend';
const DEMO_PIE_WF_ID = 'dash-category-share';

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
          __TAURI_INTERNALS__: { invoke: (c: string, a: string) => Promise<unknown> };
        }
      ).__TAURI_INTERNALS__
        .invoke(c, JSON.parse(a))
        .then((r: unknown) => done(r))
        .catch((e: unknown) => done({ __error: String(e) }));
    },
    cmd,
    JSON.stringify(args),
  ) as Promise<T>;
}

async function setWindowSize(w = 2400, h = 1600) {
  await invoke('plugin:window|set_size', { size: { width: w, height: h } });
  await browser.pause(600);
}

async function setEditorContent(text: string) {
  await browser.execute((t: string) => {
    const el = document.querySelector('.cm-editor .cm-content') as HTMLElement | null;
    if (!el) return;
    el.focus();
    const sel = window.getSelection();
    if (sel) {
      sel.selectAllChildren(el);
      sel.deleteFromDocument();
    }
    document.execCommand('insertText', false, t);
  }, text);
  await browser.pause(300);
}

async function clickExecute() {
  const clicked = await browser.execute(() => {
    const btn = Array.from(document.querySelectorAll('button')).find(
      (b) => (b.textContent || '').trim() === '执行',
    );
    if (btn && !btn.hasAttribute('disabled')) {
      btn.click();
      return true;
    }
    return false;
  });
  if (!clicked) throw new Error('执行 button not found');
}

/**
 * Execute the active query and wait for rows, retrying once if the first
 * run returns 0 rows. Freshly-opened panels occasionally drop the very first
 * streamed result (observed as `0 行` for the first query while the second
 * query on the same panel succeeds immediately).
 */
async function clickExecuteWithRetry(minRows: number, attempts = 3) {
  for (let i = 0; i < attempts; i++) {
    await clickExecute();
    try {
      await waitResults(minRows, 8000);
      return;
    } catch {
      console.log(`[retry] query attempt ${i + 1} did not reach ${minRows} rows`);
      await browser.pause(500);
    }
  }
  // Final wait with default timeout to surface a clear error.
  await waitResults(minRows);
}

async function countDtRows() {
  return browser.execute(
    () =>
      document.querySelectorAll('[data-dt-row]').length ||
      document.querySelectorAll('table tbody tr').length,
  );
}

async function waitResults(minRows = 1, timeout = 20000) {
  await browser.waitUntil(async () => (await countDtRows()) >= minRows, {
    timeout,
    timeoutMsg: `结果行数未达到 ${minRows}`,
  });
}

async function newQueryTab() {
  await browser.execute(() => {
    const btn = Array.from(document.querySelectorAll('button')).find((b) =>
      (b.textContent || '').includes('新建查询'),
    );
    btn?.click();
  });
  await browser.pause(600);
}

async function ensureQueryPanelReady() {
  await newQueryTab();
  await browser.waitUntil(
    async () =>
      browser.execute(() =>
        Array.from(document.querySelectorAll('button')).some(
          (b) => (b.textContent || '').trim() === '执行' && !b.hasAttribute('disabled'),
        ),
      ),
    { timeout: 15000, timeoutMsg: '执行 button not ready in query panel' },
  );
  await browser.pause(400);
}

/** Click a toolbar button by visible text or aria-label/title, waiting for it to exist. */
async function clickToolbarButton(text: string, timeout = 10000) {
  await browser.waitUntil(
    async () => {
      const found = await browser.execute((t: string) => {
        const btn = Array.from(document.querySelectorAll('button')).find(
          (b) =>
            (b.textContent || '').trim() === t ||
            (b.textContent || '').includes(t) ||
            (b.getAttribute('aria-label') || '').includes(t) ||
            (b.getAttribute('title') || '').includes(t),
        );
        if (btn) {
          btn.scrollIntoView({ block: 'center' });
          btn.click();
          return true;
        }
        return false;
      }, text);
      return found;
    },
    {
      timeout,
      timeoutMsg:
        `toolbar button ${text} not found; buttons=` + JSON.stringify(await listButtonLabels()),
    },
  );
  await browser.pause(600);
}

/** Diagnostic dump of button labels for failure messages. */
async function listButtonLabels(): Promise<string[]> {
  try {
    return await browser.execute(() =>
      Array.from(document.querySelectorAll('button'))
        .map(
          (b) =>
            b.getAttribute('aria-label') || b.getAttribute('title') || (b.textContent || '').trim(),
        )
        .filter((s): s is string => !!s && s.length > 0)
        .slice(0, 60),
    );
  } catch {
    return [];
  }
}

/** Click a button that contains a lucide icon of the given name, e.g. 'lucide-message-square'. */
async function clickIconButton(iconClass: string, within?: string) {
  const ok = await browser.execute(
    (cls: string, scopeSel: string | undefined) => {
      const scope: ParentNode = scopeSel
        ? (document.querySelector(scopeSel) ?? document)
        : document;
      // Prefer the LAST matching button: toolbar icon-only buttons sit on the
      // right side; chart NL-input toggle also uses MessageSquare but appears
      // earlier in DOM order inside ChartToolbar.
      const btns = Array.from(scope.querySelectorAll('button')).filter((b) =>
        b.querySelector(`svg.${cls}`),
      );
      const btn = btns[btns.length - 1];
      if (!btn) return false;
      btn.scrollIntoView({ block: 'center' });
      btn.click();
      return true;
    },
    iconClass,
    within,
  );
  if (!ok)
    throw new Error(`icon button svg.${iconClass} not found${within ? ` in ${within}` : ''}`);
}

/** Navigate to the connections page (icon rail). */
async function goToConnections() {
  await browser.execute(() => {
    document
      .querySelector('[data-testid="workspace-nav-connections"]')
      ?.dispatchEvent(new MouseEvent('click', { bubbles: true }));
  });
  await browser.pause(800);
}

/** Navigate to the workflow page. */
async function goToWorkflow() {
  await browser.execute(() => {
    document
      .querySelector('[data-testid="workspace-nav-workflow"]')
      ?.dispatchEvent(new MouseEvent('click', { bubbles: true }));
  });
  await browser.pause(800);
}

/** Navigate to the settings page. */
async function goToSettings() {
  await browser.execute(() => {
    document
      .querySelector('[data-testid="workspace-nav-settings"]')
      ?.dispatchEvent(new MouseEvent('click', { bubbles: true }));
  });
  await browser.pause(800);
}

/** Navigate to the dashboard page. */
async function goToDashboard() {
  await browser.execute(() => {
    document
      .querySelector('[data-testid="workspace-nav-dashboard"]')
      ?.dispatchEvent(new MouseEvent('click', { bubbles: true }));
  });
  await browser.pause(800);
}

/** Navigate to the plugins management page. */
async function goToPlugins() {
  await browser.execute(() => {
    document
      .querySelector('[data-testid="workspace-nav-plugins"]')
      ?.dispatchEvent(new MouseEvent('click', { bubbles: true }));
  });
  await browser.pause(800);
}

/** Open a specific settings section via its sidebar nav testid. */
async function openSettingsSection(sectionId: string) {
  await goToSettings();
  await browser.execute((id: string) => {
    document
      .querySelector(`[data-testid="settings-nav-${id}"]`)
      ?.dispatchEvent(new MouseEvent('click', { bubbles: true }));
  }, sectionId);
  await browser.pause(700);
}

/** True when an AI provider is configured (ai_get_config returns a config). */
async function aiConfigured(): Promise<boolean> {
  try {
    const cfg = await invoke<unknown>('ai_get_config');
    return !!cfg && typeof cfg === 'object' && !('__error' in (cfg as object));
  } catch {
    return false;
  }
}

/** Right-click a database node and click the context item containing `itemText`. */
async function openDbContextMenu(dbName: string, itemText: string, timeout = 10000) {
  const opened = await browser.execute((name: string) => {
    const node = document.querySelector(
      `button[data-tree-node="db"][data-db-name="${name}"]`,
    ) as HTMLElement | null;
    if (!node) return false;
    node.dispatchEvent(
      new MouseEvent('contextmenu', { bubbles: true, clientX: 300, clientY: 250 }),
    );
    return true;
  }, dbName);
  if (!opened) throw new Error(`${dbName} db node not found for ctx menu`);
  await browser.pause(600);
  await browser.waitUntil(
    async () => {
      const hit = await browser.execute((text: string) => {
        const items = Array.from(
          document.querySelectorAll('[data-testid="web-context-menu"] *, [role="menuitem"]'),
        );
        const item = items.find((el) => (el.textContent || '').includes(text));
        if (!item) return false;
        (item as HTMLElement).click();
        return true;
      }, itemText);
      return hit;
    },
    { timeout, timeoutMsg: `context menu item ${itemText} not found` },
  );
  await browser.pause(500);
}

/** Best-effort screenshot that never fails the run. */
async function softShot(name: string, settleMs = 800) {
  try {
    await shot(name, settleMs);
  } catch (e) {
    console.warn(`[soft-shot] ${name} failed: ${e}`);
  }
}

/**
 * Select a database via the Query Panel's multidb dropdown
 * ([data-testid="query-context-selectors"] → Host Select listbox).
 * This drives the real user path: the panel switches its session to the
 * target database through schemaStore.switchDatabase.
 */
async function selectQueryPanelDatabase(dbName: string) {
  // Open the dropdown trigger (only click when currently closed to avoid
  // toggle-flapping between retries).
  await browser.waitUntil(
    async () => {
      const opened = await browser.execute(() => {
        if (document.getElementById('dz-select-listbox')) return true;
        const host = document.querySelector('[data-testid="query-context-selectors"]');
        const btn = host?.querySelector('button[aria-haspopup="listbox"]') as HTMLElement | null;
        if (!btn) return false;
        btn.click();
        return false; // portal mounts asynchronously; re-check next poll
      });
      return opened;
    },
    { timeout: 10000, timeoutMsg: `db selector trigger not found for ${dbName}` },
  );
  // Pick the option whose text matches EXACTLY (demo vs demo_analytics!).
  // The selected option renders a trailing "✓" marker inside its text, so
  // strip it before comparing. If the target is ALREADY selected we're done.
  let picked = false;
  let alreadySelected = false;
  const dl = Date.now();
  while (Date.now() - dl < 8000 && !picked && !alreadySelected) {
    const probe = await browser.execute((target: string) => {
      const list = document.getElementById('dz-select-listbox');
      if (!list) return { state: 'closed' as const };
      for (const el of Array.from(list.children)) {
        const raw = (el.textContent || '').replace(/✓/g, '').trim();
        if (raw !== target) continue;
        if ((el.textContent || '').includes('✓')) return { state: 'selected' as const };
        el.dispatchEvent(new MouseEvent('mousedown', { bubbles: true, cancelable: true }));
        return { state: 'picked' as const };
      }
      return { state: 'missing' as const };
    }, dbName);
    if (probe.state === 'selected') alreadySelected = true;
    else if (probe.state === 'picked') picked = true;
    else {
      await browser.pause(300);
      await browser.execute(() => {
        if (document.getElementById('dz-select-listbox')) return;
        const host = document.querySelector('[data-testid="query-context-selectors"]');
        const btn = host?.querySelector('button[aria-haspopup="listbox"]') as HTMLElement | null;
        btn?.click();
      });
      await browser.pause(200);
    }
  }
  if (!picked && !alreadySelected) {
    // Diagnostic dump: what's actually inside the listbox?
    const diag = await browser.execute(() => {
      const list = document.getElementById('dz-select-listbox');
      const host = document.querySelector('[data-testid="query-context-selectors"]');
      return {
        listExists: !!list,
        childCount: list ? list.children.length : -1,
        childTexts: list
          ? Array.from(list.children)
              .map((c) => (c.textContent || '').trim())
              .slice(0, 15)
          : [],
        hostExists: !!host,
        hostHtmlLen: host ? host.innerHTML.length : -1,
      };
    });
    throw new Error(
      `database option "${dbName}" not found in query selector; diag=${JSON.stringify(diag)}`,
    );
  }
  await browser.pause(900); // allow get_tables refreshes to settle
}

/**
 * Verify the primary demo database is reachable so SQL / DataTable panels
 * query demo_sales (F1: get_tables pins the database explicitly — the session
 * is switched lazily by query/stream/explain carrying `database`).
 */
async function pinDemoPgDatabase() {
  const connId = await invoke<string>('connect', { connectionId: DEMO_PG_CONN_ID });
  if (typeof connId !== 'string' || connId.startsWith('__error')) {
    throw new Error(`connect(${DEMO_PG_CONN_NAME}) failed: ${JSON.stringify(connId)}`);
  }
  const tables = await invoke('get_tables', { dbSessionId: connId, database: DEMO_PG_DB });
  if (tables && typeof tables === 'object' && '__error' in (tables as object)) {
    throw new Error(`get_tables(${DEMO_PG_DB}) failed: ${JSON.stringify(tables)}`);
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

async function ensureDemoPgConnectedInTree(timeout = 30000) {
  await goToConnections();

  const ready = await browser.execute(
    (connName: string, dbName: string) => {
      const hasDb = !!document.querySelector(
        `button[data-tree-node="db"][data-db-name="${dbName}"]`,
      );
      return hasDb;
    },
    DEMO_PG_CONN_NAME,
    DEMO_PG_DB,
  );
  if (ready) return;

  await browser.execute((connName: string) => {
    const items = Array.from(document.querySelectorAll('[data-conn-item]'));
    const pg = items.find((el) => (el.getAttribute('data-conn-name') || '').includes(connName));
    const chev = Array.from(pg?.querySelectorAll('button') ?? []).find(
      (b) =>
        !!b.querySelector('svg.lucide-chevron-right') ||
        !!b.querySelector('svg.lucide-chevron-down'),
    );
    if (chev) (chev as HTMLElement).click();
  }, DEMO_PG_CONN_NAME);
  await browser.waitUntil(
    async () =>
      browser.execute(
        (dbName: string) =>
          !!document.querySelector(`button[data-tree-node="db"][data-db-name="${dbName}"]`),
        DEMO_PG_DB,
      ),
    { timeout, timeoutMsg: `${DEMO_PG_DB} not visible after expanding ${DEMO_PG_CONN_NAME}` },
  );
  await browser.pause(500);
}

/**
 * Expand a demo database → public schema → 表 category in the virtualized tree.
 * Only operates on the subtree rooted at `dbName` — never touches unrelated DBs.
 */
async function expandDemoDbTables(dbName: string) {
  if (dbName === DEMO_PG_DB) {
    await pinDemoPgDatabase();
  }
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

/** Expand a demo database down to schema level (for multi-db screenshots). */
async function expandDemoDbToSchema(dbName: string) {
  if (!(await demoSchemaVisible(dbName))) {
    if (!(await demoNodeExpanded(dbName, 'db'))) {
      await clickDemoDbNode(dbName, 'db');
    }
    await browser.waitUntil(async () => demoSchemaVisible(dbName), {
      timeout: 20000,
      timeoutMsg: `public schema did not appear under ${dbName}`,
    });
  }
  await browser.pause(300);
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

/** True when a tree node of the given kind and attribute exists. */
async function treeNodeExists(kind: 'db' | 'schema', attr: string, value: string) {
  return browser.execute(
    (k: string, a: string, v: string) =>
      !!document.querySelector(`button[data-tree-node="${k}"][${a}="${v}"]`),
    kind,
    attr,
    value,
  );
}

/**
 * True when the node's chevron points down (expanded). The tree renders
 * ChevronDown/ChevronRight lucide icons inside the node button.
 */
async function nodeIsExpanded(kind: 'db' | 'schema', attr: string, value: string) {
  return browser.execute(
    (k: string, a: string, v: string) => {
      const el = document.querySelector<HTMLElement>(`button[data-tree-node="${k}"][${a}="${v}"]`);
      if (!el) return false;
      return !!el.querySelector('svg.lucide-chevron-down');
    },
    kind,
    attr,
    value,
  );
}

/** Scroll the virtualized container so the node is mounted, then click it. */
async function scrollAndClickNode(kind: 'db' | 'schema', attr: string, value: string) {
  const clicked = await browser.execute(
    (k: string, a: string, v: string) => {
      const find = () =>
        document.querySelector<HTMLElement>(`button[data-tree-node="${k}"][${a}="${v}"]`);
      let el = find();
      if (!el) {
        // Nudge nearby scrollers so virtualization mounts the node.
        const scrollers = Array.from(
          document.querySelectorAll<HTMLElement>(
            '[class*="overflow-y-auto"], [class*="overflow-auto"]',
          ),
        ).filter((s) => s.scrollHeight > s.clientHeight);
        for (const s of scrollers) s.scrollTop = Math.max(0, s.scrollTop - 400);
        el = find();
      }
      if (!el) return false;
      el.scrollIntoView({ block: 'center' });
      el.click();
      return true;
    },
    kind,
    attr,
    value,
  );
  if (!clicked) throw new Error(`tree node ${kind}[${attr}=${value}] not clickable`);
}

// ── Demo workflow YAML ──

const DEMO_WORKFLOW_YAML = `id: ${DEMO_WORKFLOW_ID}
name: 跨库订单物流演示
description: PostgreSQL 订单库 × MySQL 物流库联合查询，一步获取用户最新订单与物流状态
timeout_secs: 60
error_handling:
  strategy: abort
connection: ${DEMO_PG_CONN_ID}
steps:
  - type: query
    id: orders
    sql: "SELECT order_id, product_name, amount, status FROM public.test_orders WHERE uid = 'U001' ORDER BY created_at DESC"
    timeout_secs: 10
  - type: query
    id: logistics
    connection: ${DEMO_MYSQL_CONN_ID}
    sql: "SELECT order_id, carrier, tracking_no, status FROM ${DEMO_MYSQL_DB}.test_logistics WHERE order_id IN ('ORD-2026-005','ORD-2026-002','ORD-2026-001')"
    timeout_secs: 10
`;

const DEMO_CONN_IDS = [DEMO_PG_CONN_ID, DEMO_MYSQL_CONN_ID, DEMO_REDIS_CONN_ID, 'conn_e2e_pg'];

// ── Dashboard seed workflows + dashboard JSON (21-dashboard) ──

const DEMO_TREND_WF_YAML = `id: ${DEMO_TREND_WF_ID}
name: 每日销售额趋势
description: demo_sales 按日汇总销售额
connection: ${DEMO_PG_CONN_ID}
steps:
  - type: query
    id: trend
    database: ${DEMO_PG_DB}
    sql: "SELECT sale_date, SUM(amount) AS total_amount FROM demo_sales GROUP BY sale_date ORDER BY sale_date"
    timeout_secs: 10
`;

const DEMO_PIE_WF_YAML = `id: ${DEMO_PIE_WF_ID}
name: 品类销售额占比
description: demo_sales 按品类汇总销售额
connection: ${DEMO_PG_CONN_ID}
steps:
  - type: query
    id: share
    database: ${DEMO_PG_DB}
    sql: "SELECT category, SUM(amount) AS total_amount FROM demo_sales GROUP BY category ORDER BY total_amount DESC"
    timeout_secs: 10
`;

function chartConfig(partial: {
  chartType: 'line' | 'pie';
  xAxis: string;
  yAxes: string[];
}): Record<string, unknown> {
  return {
    chartType: partial.chartType,
    xAxis: partial.xAxis,
    yAxes: partial.yAxes,
    groupBy: null,
    aggregation: 'none',
    sortBy: 'x_asc',
    showLegend: true,
    showGrid: true,
    showValues: false,
    colorScheme: 'default',
  };
}

function buildDemoDashboard(now: string): Record<string, unknown> {
  return {
    id: DEMO_DASHBOARD_ID,
    name: '销售监控演示',
    createdAt: now,
    updatedAt: now,
    layout: { cols: 12, rowHeight: 80 },
    enabled: true,
    widgets: [
      {
        id: 'w-demo-trend',
        title: '每日销售额趋势',
        workflowId: DEMO_TREND_WF_ID,
        viewMode: 'chart',
        chartConfig: chartConfig({
          chartType: 'line',
          xAxis: 'sale_date',
          yAxes: ['total_amount'],
        }),
        layout: { x: 0, y: 0, w: 8, h: 5 },
        refresh: { mode: 'onOpen' },
        enabled: true,
      },
      {
        id: 'w-demo-share',
        title: '品类销售额占比',
        workflowId: DEMO_PIE_WF_ID,
        viewMode: 'chart',
        chartConfig: chartConfig({ chartType: 'pie', xAxis: 'category', yAxes: ['total_amount'] }),
        layout: { x: 8, y: 0, w: 4, h: 5 },
        refresh: { mode: 'onOpen' },
        enabled: true,
      },
      {
        id: 'w-demo-orders',
        title: '最新订单物流',
        workflowId: DEMO_WORKFLOW_ID,
        viewMode: 'table',
        layout: { x: 0, y: 5, w: 12, h: 4 },
        refresh: { mode: 'manual' },
        enabled: true,
      },
    ],
  };
}

async function deleteConnectionsByIds(ids: string[]) {
  for (const id of ids) {
    await invoke('delete_connection', { id });
  }
}

async function restoreConnections(configs: ConnectionConfig[]) {
  for (const config of configs) {
    const r = await invoke('save_connection', { config });
    if (r && typeof r === 'object' && '__error' in (r as object)) {
      console.warn(`[restore] save_connection(${config.id}) failed: ${JSON.stringify(r)}`);
    }
  }
}

// ── Screenshot cases ──

describe('site screenshots', () => {
  let mainWindow: string;
  /** Snapshot of the user's persisted connections — restored in `after`. */
  let backedUpConnections: ConnectionConfig[] = [];
  /** Whether the demo workflow existed before this spec ran. */
  let demoWorkflowExisted = false;
  /** Dashboard seed workflow ids that already existed before this run. */
  const preExistingSeedWorkflows = new Set<string>();
  /** Whether the demo dashboard existed before this spec ran. */
  let demoDashboardExisted = false;

  before(async () => {
    mainWindow = await browser.getWindowHandle();
    await setWindowSize();

    await browser.waitUntil(
      async () => browser.execute(() => document.querySelectorAll('[data-conn-item]').length > 0),
      {
        timeout: 15000,
        timeoutMsg: 'no data-conn-item found on startup',
      },
    );

    // ── Backup current connections & demo workflow, then swap in demo-only state ──
    backedUpConnections = await invoke<ConnectionConfig[]>('get_connections');
    const workflows = await invoke<{ id: string }[]>('workflow_list');
    demoWorkflowExisted = workflows.some((w) => w.id === DEMO_WORKFLOW_ID);
    for (const id of [DEMO_TREND_WF_ID, DEMO_PIE_WF_ID]) {
      if (workflows.some((w) => w.id === id)) preExistingSeedWorkflows.add(id);
    }
    demoDashboardExisted = (await invoke<{ id: string }[]>('list_dashboards').catch(() => [])).some(
      (d) => d.id === DEMO_DASHBOARD_ID,
    );

    for (const c of backedUpConnections) {
      await invoke('delete_connection', { id: c.id });
    }
    await deleteConnectionsByIds(['conn_e2e_pg']);
    await browser.pause(400);

    // ── 00-welcome: first-run page with zero connections ──
    try {
      await browser.url('tauri://localhost');
      await browser.waitUntil(
        async () =>
          browser.execute(
            () =>
              !!document.querySelector('[data-testid="welcome-page"]') &&
              document.querySelectorAll('[data-conn-item]').length === 0,
          ),
        { timeout: 15000, timeoutMsg: 'welcome page not shown with zero connections' },
      );
      await softShot('00-welcome.png', 900);
    } catch (e) {
      console.warn(`[warn] 00-welcome capture skipped: ${e}`);
    }

    const demoPgUser = process.env.E2E_DEMO_PG_USER || 'datazen_demo';
    const demoPgPassword = process.env.E2E_DEMO_PG_PASSWORD || 'datazen_demo';
    const demoMysqlUser = process.env.E2E_DEMO_MYSQL_USER || 'datazen_demo';
    const demoMysqlPassword = process.env.E2E_DEMO_MYSQL_PASSWORD || 'datazen_demo';

    const demoConnections: ConnectionConfig[] = [
      {
        id: DEMO_PG_CONN_ID,
        name: DEMO_PG_CONN_NAME,
        databaseType: 'postgresql',
        host: process.env.E2E_PG_HOST || '127.0.0.1',
        port: Number(process.env.E2E_PG_PORT) || 5432,
        username: demoPgUser,
        password: demoPgPassword,
        database: '',
        group: 'preset:development',
        colorTag: '#3b82f6',
        sslMode: 'disable',
      },
      {
        id: DEMO_MYSQL_CONN_ID,
        name: '演示 MySQL · 物流',
        databaseType: 'mysql',
        host: process.env.E2E_MYSQL_HOST || '127.0.0.1',
        port: Number(process.env.E2E_MYSQL_PORT) || 3306,
        username: demoMysqlUser,
        password: demoMysqlPassword,
        database: DEMO_MYSQL_DB,
        group: 'preset:development',
        colorTag: '#22c55e',
        sslMode: 'disable',
      },
      {
        id: DEMO_REDIS_CONN_ID,
        name: '演示 Redis',
        databaseType: 'redis',
        host: process.env.E2E_REDIS_HOST || '127.0.0.1',
        port: Number(process.env.E2E_REDIS_PORT) || 6379,
        username: '',
        password: process.env.E2E_REDIS_PASSWORD || '',
        database: '',
        group: 'preset:development',
        colorTag: '#ef4444',
        sslMode: 'disable',
      },
    ];
    for (const config of demoConnections) {
      const r = await invoke('save_connection', { config });
      if (r && typeof r === 'object' && '__error' in (r as object)) {
        throw new Error(`save_connection(${config.id}) failed: ${JSON.stringify(r)}`);
      }
    }
    await browser.pause(600);

    await invoke('workflow_save_yaml', { yaml: DEMO_WORKFLOW_YAML });
    await invoke('workflow_save_yaml', { yaml: DEMO_TREND_WF_YAML });
    await invoke('workflow_save_yaml', { yaml: DEMO_PIE_WF_YAML });
    await browser.pause(400);

    const now = new Date().toISOString();
    await invoke('save_dashboard', { dashboard: buildDemoDashboard(now) });
    await browser.pause(300);

    const connId = await invoke<string>('connect', { connectionId: DEMO_PG_CONN_ID });
    if (typeof connId !== 'string' || connId.startsWith('__error')) {
      throw new Error(`connect(${DEMO_PG_CONN_NAME}) failed: ${JSON.stringify(connId)}`);
    }
    const tables = await invoke('get_tables', { dbSessionId: connId, database: DEMO_PG_DB });
    if (tables && typeof tables === 'object' && '__error' in (tables as object)) {
      throw new Error(`get_tables(${DEMO_PG_DB}) failed: ${JSON.stringify(tables)}`);
    }

    await invoke('connect', { connectionId: DEMO_MYSQL_CONN_ID });
    await browser.pause(800);

    await browser.url('tauri://localhost');
    await browser.pause(2500);
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
      await deleteConnectionsByIds(DEMO_CONN_IDS);
      await restoreConnections(backedUpConnections);
      if (!demoWorkflowExisted) {
        await invoke('workflow_delete', { workflowId: DEMO_WORKFLOW_ID });
      }
      if (!demoDashboardExisted) {
        await invoke('delete_dashboard', { id: DEMO_DASHBOARD_ID }).catch(() => {});
      }
      for (const id of [DEMO_TREND_WF_ID, DEMO_PIE_WF_ID]) {
        if (!preExistingSeedWorkflows.has(id)) {
          await invoke('workflow_delete', { workflowId: id }).catch(() => {});
        }
      }
      await browser.url('tauri://localhost');
      await browser.pause(1500);
      console.log(`[restore] ${backedUpConnections.length} connection(s) restored`);
    } catch (e) {
      console.error('[restore] failed to restore user connections:', e);
      throw e;
    }
  });

  // ─────────────────────── 01-main-window ─────────────────────────────────

  it('01-main-window: connection manager', async () => {
    await goToConnections();
    await shot('01-main-window.png');
  });

  // ─────────────────────── 29-new-connection ──────────────────────────────

  it('29-new-connection: new connection dialog', async () => {
    await goToConnections();
    const opened = await browser.execute(() => {
      const btn = document.querySelector('button[title="新建连接"]') as HTMLElement | null;
      if (!btn) return false;
      btn.click();
      return true;
    });
    if (!opened) throw new Error('新建连接 button not found');
    await browser.waitUntil(
      async () =>
        browser.execute(
          () =>
            !!document.querySelector(
              '[role="dialog"], .fixed.inset-0, [data-testid*="connection-dialog"]',
            ),
        ),
      { timeout: 8000, timeoutMsg: '新建连接 dialog not rendered' },
    );
    await softShot('29-new-connection.png', 700);
    // Close the dialog without saving.
    await browser.execute(() => {
      const btns = Array.from(document.querySelectorAll('button'));
      const cancel = btns.find(
        (b) => (b.textContent || '').trim() === '取消' || (b.textContent || '').trim() === '关闭',
      );
      cancel?.click();
    });
    await browser.pause(500);
  });

  // ─────────────────────── 14-multidb ──────────────────────────────────────

  it('14-multidb: multi-database tree', async () => {
    await ensureDemoPgConnectedInTree();

    // Expand both isolated demo databases — never unrelated E2E/personal DBs.
    await expandDemoDbTables(DEMO_PG_DB);
    await expandDemoDbToSchema(DEMO_PG_DB2);
    await shot('14-multidb.png');
    await pinDemoPgDatabase();
  });

  // ─────────────────────── 17-sql-editor ───────────────────────────────────

  it('17-sql-editor', async () => {
    // Connect + expand the demo database so currentDatabase is datazen_demo.
    // IMPORTANT: expandDemoDbTables only expands the sidebar tree — it does NOT
    // switch the SQL session. After 14-multidb expanded datazen_demo_analytics,
    // the session stayed there and queries failed with "demo_sales does not
    // exist". Always pin the session to DEMO_PG_DB before running SQL.
    await ensureDemoPgConnectedInTree();
    await expandDemoDbTables(DEMO_PG_DB);
    await pinDemoPgDatabase();
    await browser.pause(300);

    // Start a fresh query tab, then switch the session via the panel's own
    // multidb dropdown — the reliable user path (sidebar reloads can flip the
    // session back to datazen_demo_analytics after pinning).
    await newQueryTab();
    await selectQueryPanelDatabase(DEMO_PG_DB);
    await setEditorContent(
      'SELECT sale_date,\n       category,\n       region,\n       amount,\n       quantity\nFROM demo_sales\nORDER BY sale_date DESC, amount DESC\nLIMIT 20;',
    );
    await clickExecuteWithRetry(10);
    await shot('17-sql-editor.png');
  });

  // ─────────────────────── 02-query-chart + 10-chart-types ─────────────────

  it('02-query-chart + 10-chart-types', async () => {
    await ensureDemoPgConnectedInTree();
    await expandDemoDbTables(DEMO_PG_DB);
    await newQueryTab();
    await setEditorContent(
      'SELECT sale_date,\n       SUM(amount) AS total_amount,\n       SUM(quantity) AS total_quantity\nFROM demo_sales\nGROUP BY sale_date\nORDER BY sale_date;',
    );
    await clickExecuteWithRetry(15);
    await clickToolbarButton('图表');
    await shot('02-query-chart.png');
    await clickToolbarButton('柱状图');
    await shot('10-chart-types.png');
  });

  // ─────────────────────── 11-chart-export: pie ────────────────────────────

  it('11-chart-export: pie by category', async () => {
    await ensureDemoPgConnectedInTree();
    await expandDemoDbTables(DEMO_PG_DB);
    await ensureQueryPanelReady();
    await setEditorContent(
      'SELECT category,\n       SUM(amount) AS total_amount\nFROM demo_sales\nGROUP BY category\nORDER BY total_amount DESC;',
    );
    await clickExecuteWithRetry(4);
    await clickToolbarButton('图表');
    await clickToolbarButton('饼图');
    await shot('11-chart-export.png');
  });

  // ─────────────────────── 16-er ───────────────────────────────────────────

  it('16-er: ER diagram with relations', async () => {
    await ensureDemoPgConnectedInTree();
    await expandDemoDbTables(DEMO_PG_DB);
    await pinDemoPgDatabase(); // ER reads the session database — pin to demo db
    await browser.pause(400);
    // Open the panel first, then switch via the multidb dropdown so the ER
    // reads datazen_demo (sidebar reloads may have flipped the session).
    await ensureQueryPanelReady();
    await selectQueryPanelDatabase(DEMO_PG_DB);
    await clickToolbarButton('ER 图');
    await browser.waitUntil(
      async () => {
        const n = await browser.execute(
          () => document.querySelectorAll('.react-flow__node').length,
        );
        return n >= 3;
      },
      { timeout: 20000, timeoutMsg: 'ER 节点未渲染' },
    );
    await browser.pause(1500);
    await shot('16-er.png');
    // Back to first query tab.
    const clicked = await browser.execute(() => {
      const btns = Array.from(document.querySelectorAll('button'));
      const btn = btns.find((b) => (b.textContent || '').includes('查询'));
      if (btn) {
        btn.click();
        return true;
      }
      return false;
    });
    if (!clicked) console.log('[warn] could not find query tab button to go back');
    await browser.pause(500);
  });

  // ─────────────────────── 03-ai-nl2sql / 06-explain / 05-diagnosis ────────

  it('03-ai-nl2sql + 06-ai-explain + 05-ai-diagnosis', async () => {
    const hasAi = await aiConfigured();
    if (!hasAi) {
      console.log('[skip] AI not configured — 03/05/06 screenshots skipped');
      return;
    }
    await ensureDemoPgConnectedInTree();
    await expandDemoDbTables(DEMO_PG_DB);
    await ensureQueryPanelReady();
    // NL2SQL/EXPLAIN read the session database — pin it via the dropdown.
    await selectQueryPanelDatabase(DEMO_PG_DB);

    // ── 03: NL2SQL panel with a typed prompt ──
    try {
      await clickToolbarButton('AI 生成 SQL');
      // The textarea may take a moment; poll for it robustly.
      let found = false;
      const dl = Date.now();
      while (Date.now() - dl < 15000 && !found) {
        found = await browser.execute(() => {
          const host = document.querySelector('[data-testid="query-context-selectors"]');
          const tas = Array.from(document.querySelectorAll('textarea'));
          return tas.some((t) => {
            const ph = t.getAttribute('placeholder') || '';
            return ph.includes('自然语言') || ph.includes('@');
          });
        });
        if (!found) await browser.pause(400);
      }
      if (!found) throw new Error('NL2SQL textarea not found after toggle');
      await browser.execute(() => {
        const ta = Array.from(document.querySelectorAll('textarea')) as HTMLTextAreaElement[];
        const input =
          ta.find((t) => (t.getAttribute('placeholder') || '').includes('自然语言')) ?? ta[0];
        input.focus();
        const setter = Object.getOwnPropertyDescriptor(HTMLTextAreaElement.prototype, 'value')?.set;
        setter?.call(input, '统计每个地区的销售总额，并找出销售额最高的地区');
        input.dispatchEvent(new Event('input', { bubbles: true }));
      });
      await softShot('03-ai-nl2sql.png', 700);
    } catch (e) {
      console.warn(`[warn] 03-ai-nl2sql skipped: ${e}`);
    } finally {
      // Collapse the panel again if it was toggled on.
      await browser.execute(() => {
        const btn = Array.from(document.querySelectorAll('button')).find(
          (b) =>
            (b.textContent || '').trim() === 'AI 生成 SQL' &&
            b.className.includes('bg-surface-raised'),
        );
        (btn as HTMLElement | undefined)?.click();
      });
      await browser.pause(300);
    }

    // ── 06: EXPLAIN analysis on a valid query ──
    try {
      await setEditorContent(
        'SELECT region, SUM(amount) AS total_amount\nFROM demo_sales\nGROUP BY region\nORDER BY total_amount DESC;',
      );
      await clickToolbarButton('EXPLAIN 分析', 15000);
      await browser.waitUntil(
        async () =>
          browser.execute(() => {
            const text = document.body.textContent || '';
            return (
              text.includes('EXPLAIN') &&
              (text.includes('Seq Scan') ||
                text.includes('Index Scan') ||
                text.includes('cost=') ||
                text.includes('执行计划') ||
                text.includes('分析'))
            );
          }),
        { timeout: 30000, timeoutMsg: 'EXPLAIN result did not render' },
      );
      await shot('06-ai-explain.png', 900);
    } catch (e) {
      console.warn(`[warn] 06-ai-explain skipped: ${e}`);
      await softShot('06-ai-explain.png', 500);
    }

    // ── 05: run a failing query → AI diagnosis dialog ──
    try {
      await ensureQueryPanelReady();
      await selectQueryPanelDatabase(DEMO_PG_DB);
      await setEditorContent('SELECT * FROM demo_not_exist_table;');
      await clickExecute();
      // Wait for the error panel with the 诊断 button.
      await browser.waitUntil(
        async () =>
          browser.execute(() => {
            const btn = Array.from(document.querySelectorAll('button')).find(
              (b) => (b.textContent || '').trim() === '诊断',
            );
            return !!btn;
          }),
        { timeout: 20000, timeoutMsg: 'error panel with 诊断 button not found' },
      );
      await browser.execute(() => {
        const btn = Array.from(document.querySelectorAll('button')).find(
          (b) => (b.textContent || '').trim() === '诊断',
        );
        (btn as HTMLElement | undefined)?.click();
      });
      // Diagnosis content renders 错误原因 section when done.
      await browser.waitUntil(
        async () => browser.execute(() => (document.body.textContent || '').includes('错误原因')),
        { timeout: 90000, timeoutMsg: 'diagnosis content did not render' },
      );
      await shot('05-ai-diagnosis.png', 900);
    } catch (e) {
      console.warn(`[warn] 05-ai-diagnosis skipped: ${e}`);
    } finally {
      // Close any open dialog.
      await browser.execute(() => {
        const btns = Array.from(document.querySelectorAll('button'));
        const close =
          btns.find((b) => (b.textContent || '').trim() === '关闭') ??
          btns.find((b) => b.querySelector('svg.lucide-x'));
        (close as HTMLElement | undefined)?.click();
      });
      await browser.pause(500);
    }
  });

  // ─────────────────────── 08-ai-filter ────────────────────────────────────

  it('08-ai-filter: natural language filter', async () => {
    await pinDemoPgDatabase();
    await goToConnections();
    await browser.pause(300);
    await browser.execute((connName: string) => {
      const items = Array.from(document.querySelectorAll('[data-conn-item]'));
      const pg = items.find((el) => (el.getAttribute('data-conn-name') || '').includes(connName));
      if (pg) (pg as HTMLElement).click();
    }, DEMO_PG_CONN_NAME);
    await browser.pause(400);

    await expandDemoDbTables(DEMO_PG_DB);

    // Click demo_sales table in the sidebar (robust against virtualization:
    // scroll available scrollers and retry until the node mounts).
    let demo = false;
    const dl = Date.now();
    while (Date.now() - dl < 15000 && !demo) {
      demo = await browser.execute(() => {
        const nodes = Array.from(
          document.querySelectorAll<HTMLElement>(
            'button[data-tree-node="table"], button[data-tree-node="view"]',
          ),
        );
        let el = nodes.find(
          (n) => (n.getAttribute('data-item-name') || '').trim() === 'demo_sales',
        );
        if (!el) {
          const scrollers = Array.from(
            document.querySelectorAll<HTMLElement>(
              '[class*="overflow-y-auto"], [class*="overflow-auto"]',
            ),
          ).filter((s) => s.scrollHeight > s.clientHeight);
          for (const s of scrollers) s.scrollTop = Math.max(0, s.scrollTop - 300);
          el = Array.from(
            document.querySelectorAll<HTMLElement>(
              'button[data-tree-node="table"], button[data-tree-node="view"]',
            ),
          ).find((n) => (n.getAttribute('data-item-name') || '').trim() === 'demo_sales');
        }
        if (!el) return false;
        el.scrollIntoView({ block: 'center' });
        el.dispatchEvent(new MouseEvent('dblclick', { bubbles: true, cancelable: true }));
        return true;
      });
      if (!demo) await browser.pause(250);
    }
    if (!demo) {
      const names: string[] = await browser.execute(() =>
        Array.from(document.querySelectorAll<HTMLElement>('button[data-tree-node]')).map((n) => {
          const k = n.getAttribute('data-tree-node');
          const nm =
            n.getAttribute('data-item-name') ||
            n.getAttribute('data-schema-name') ||
            n.getAttribute('data-db-name') ||
            n.getAttribute('data-cat-id') ||
            '';
          return `${k}:${nm}`;
        }),
      );
      console.log('[diag-08] tree nodes=' + JSON.stringify(names));
      throw new Error('demo_sales not found in sidebar');
    }
    // Give the DataTable time to fully render the loaded rows.
    await browser.pause(2500);

    // The NL filter starts collapsed (Sparkles icon). Two variants exist:
    // configured → title=智能筛选; unconfigured → title=请先在设置中配置AI服务.
    // The store's isConfigured may lag, so accept either and click the
    // configured one once it appears (poll instead of one-shot check).
    let sparklesReady = false;
    const sdl = Date.now();
    while (Date.now() - sdl < 30000 && !sparklesReady) {
      sparklesReady = await browser.execute(() => {
        const configured = document.querySelector(
          'button[aria-label="智能筛选"], button[title="智能筛选"]',
        );
        if (configured) {
          (configured as HTMLElement).scrollIntoView({ block: 'center' });
          (configured as HTMLElement).click();
          return true;
        }
        return false;
      });
      if (!sparklesReady) await browser.pause(500);
    }
    if (!sparklesReady) {
      console.log('[warn] 智能筛选 button not found (AI store not ready); skipping 08');
    }
    await browser.pause(600);

    if (sparklesReady) {
      const typed = await browser.execute(() => {
        const inputs = Array.from(document.querySelectorAll('input')) as HTMLInputElement[];
        const input = inputs.find((i) => (i.placeholder || '').includes('自然语言'));
        if (!input) return false;
        input.focus();
        const setter = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value')?.set;
        setter?.call(input, '金额大于 800 且地区是华东');
        input.dispatchEvent(new Event('input', { bubbles: true }));
        return true;
      });
      if (!typed) throw new Error('NL filter input not found');
      await browser.pause(300);

      // 筛选 parse button next to the input.
      await clickToolbarButton('筛选');
      await browser.waitUntil(
        async () =>
          browser.execute(() => {
            const text = document.body.textContent || '';
            return text.includes('amount gt') || text.includes('已解析');
          }),
        { timeout: 90000, timeoutMsg: '筛选条件未解析' },
      );
      await browser.waitUntil(
        async () => {
          const rows = await countDtRows();
          return rows > 0 && rows < 50;
        },
        { timeout: 30000, timeoutMsg: '筛选结果未出现' },
      );
      await shot('08-ai-filter.png');
    }
  });

  // ─────────────────────── 07-ai-chat ──────────────────────────────────────

  it('07-ai-chat: assistant conversation', async () => {
    // Open a query panel so the ContentToolbar (with the AI chat toggle)
    // renders — the toolbar only appears when an active panel exists.
    await ensureDemoPgConnectedInTree();
    await expandDemoDbTables(DEMO_PG_DB);
    await newQueryTab();
    await selectQueryPanelDatabase(DEMO_PG_DB);
    await browser.pause(500);

    // The AI chat toggle is the MessageSquare icon button in ContentToolbar.
    await clickIconButton('lucide-message-square');
    await browser.pause(800);

    const asked = await browser.execute(() => {
      const ta = Array.from(document.querySelectorAll('textarea')) as HTMLTextAreaElement[];
      const input = ta.find((t) => (t.placeholder || '').includes('输入消息'));
      if (!input) return false;
      input.focus();
      const setter = Object.getOwnPropertyDescriptor(HTMLTextAreaElement.prototype, 'value')?.set;
      setter?.call(input, 'demo_sales 里哪个分类的销售额最高？总额是多少？');
      input.dispatchEvent(new Event('input', { bubbles: true }));
      return true;
    });
    if (!asked) throw new Error('AI input not found');
    await browser.keys('Enter');
    // Wait until the chat content settles (no longer growing for two
    // consecutive polls). During streaming the dialog text keeps growing, so
    // a stable length means the assistant reply finished rendering.
    let prevLen = -1;
    let stableCount = 0;
    await browser.waitUntil(
      async () => {
        const cur = await browser.execute(() => (document.body.textContent || '').length);
        if (prevLen < 0) {
          prevLen = cur;
          return false;
        }
        stableCount = cur === prevLen ? stableCount + 1 : 0;
        prevLen = cur;
        return stableCount >= 2;
      },
      { timeout: 180000, timeoutMsg: 'AI 回复超时' },
    );
    await browser.pause(800);
    // After settling, confirm the reply mentions the top category.
    const mention = await browser.execute(() =>
      (document.body.textContent || '').includes('电子产品'),
    );
    if (!mention) console.log('[warn] AI reply did not mention 电子产品; capturing anyway');
    await shot('07-ai-chat.png');
  });

  // ─────────────────────── 04-workflow + 12-crossdb + 13-run ──────────────

  it('04-workflow + 12-workflow-crossdb + 13-workflow-run', async () => {
    await goToWorkflow();
    await browser.pause(500);

    // Click the workflow list item.
    const sel = await browser.execute(() => {
      const items = Array.from(document.querySelectorAll('div, button, span'));
      const matches = items.filter((el) => (el.textContent || '').includes('跨库订单物流演示'));
      const deepest = matches.filter((el) => !matches.some((o) => o !== el && el.contains(o)));
      const target = deepest[deepest.length - 1] as HTMLElement | undefined;
      if (!target) return false;
      target.click();
      return true;
    });
    if (!sel) throw new Error('workflow item not found');
    await browser.pause(1000);

    // 12: editor view showing the cross-db steps before running.
    await softShot('12-workflow-crossdb.png', 600);

    // Click Execute.
    await browser.execute(() => {
      const btn = Array.from(document.querySelectorAll('button')).find(
        (b) => (b.textContent || '').trim() === '执行' && !b.hasAttribute('disabled'),
      );
      btn?.click();
    });
    await browser.waitUntil(
      async () => {
        const rows = await countDtRows();
        return rows >= 3;
      },
      { timeout: 30000, timeoutMsg: 'workflow 结果未出现' },
    );
    await shot('04-workflow.png', 1000);

    // 13: run history side-tab — the execution above guarantees one entry.
    try {
      await browser.execute(() => {
        const tab = Array.from(document.querySelectorAll('button')).find((b) =>
          (b.textContent || '').includes('执行记录'),
        );
        tab?.click();
      });
      await browser.waitUntil(
        async () =>
          browser.execute(() => !(document.body.textContent || '').includes('暂无执行记录')),
        { timeout: 8000, timeoutMsg: 'run history did not load' },
      );
      // Open the newest entry if the list is clickable items.
      await browser.execute(() => {
        const items = Array.from(document.querySelectorAll('[data-testid*="history"], li, button'))
          .filter((el) => /202\d|秒前|分钟前|刚刚/.test(el.textContent || ''))
          .filter((el) => el.getClientRects().length > 0);
        (items[0] as HTMLElement | undefined)?.click();
      });
      await browser.pause(1200);
      await shot('13-workflow-run.png', 500);
      // Back to the workflows list for subsequent cases.
      await browser.execute(() => {
        const tab = Array.from(document.querySelectorAll('button')).find(
          (b) => (b.textContent || '').trim() === 'Workflows',
        );
        tab?.click();
      });
      await browser.pause(400);
    } catch (e) {
      console.warn(`[warn] 13-workflow-run capture skipped: ${e}`);
      await goToWorkflow();
    }
  });

  // ─────────────────────── 21-dashboard ────────────────────────────────────

  it('21-dashboard: seeded widgets render charts and table', async () => {
    await goToDashboard();
    // Select the seeded dashboard tab if the page did not open it by default.
    await browser.execute((id: string) => {
      const tab = document.querySelector(
        `[data-testid="dashboard-tab"][data-dashboard-id="${id}"]`,
      ) as HTMLElement | null;
      tab?.click();
    }, DEMO_DASHBOARD_ID);
    // Widgets refresh onOpen; wait until at least one chart SVG or widget tile renders.
    let rendered = false;
    const dl = Date.now();
    while (Date.now() - dl < 30000 && !rendered) {
      rendered = await browser.execute(() => {
        const text = document.body.textContent || '';
        return (
          ((document.querySelectorAll('.recharts-wrapper, .recharts-surface').length >= 1 ||
            document.querySelectorAll('svg').length >= 3) &&
            text.includes('每日销售额趋势')) ||
          (text.includes('品类销售额占比') && text.includes('每日销售额趋势'))
        );
      });
      if (!rendered) await browser.pause(500);
    }
    if (!rendered) console.log('[warn] dashboard widgets did not render in time; capturing anyway');
    await shot('21-dashboard.png', 1000);
    await goToConnections();
  });

  // ─────────────────────── 22-plugins ──────────────────────────────────────

  it('22-plugins: plugin management page', async () => {
    await goToPlugins();
    await browser.waitUntil(
      async () =>
        browser.execute(
          () =>
            !!document.querySelector('[data-testid="plugin-management-page"]') &&
            (!!document.querySelector('[data-testid="plugin-card"]') ||
              !!document.querySelector('[data-testid="plugin-install-button"]')),
        ),
      { timeout: 15000, timeoutMsg: 'plugin management page not ready' },
    );
    await shot('22-plugins.png');
    await goToConnections();
  });

  // ─────────────────────── 19-backup-sync ──────────────────────────────────

  it('19-backup-sync: backup window prefilled', async () => {
    await browser.switchToWindow(mainWindow);
    await goToConnections();
    await browser.pause(500);

    // Ensure demo DB node is visible in tree.
    await ensureDemoPgConnectedInTree();

    // Right-click the demo database node.
    const ctx = await browser.execute((dbName: string) => {
      const node = document.querySelector(
        `button[data-tree-node="db"][data-db-name="${dbName}"]`,
      ) as HTMLElement | null;
      if (!node) return false;
      node.dispatchEvent(
        new MouseEvent('contextmenu', { bubbles: true, clientX: 200, clientY: 200 }),
      );
      return true;
    }, DEMO_PG_DB);
    if (!ctx) throw new Error(`${DEMO_PG_DB} db node not found`);
    await browser.pause(600);
    const menuHit = await browser.execute(() => {
      const items = Array.from(
        document.querySelectorAll('[data-testid="web-context-menu"] *, [role="menuitem"]'),
      );
      const item = items.find((el) => (el.textContent || '').includes('备份数据库'));
      if (!item) return false;
      (item as HTMLElement).click();
      return true;
    });
    if (!menuHit) throw new Error('备份数据库 menu item not found');
    await browser.waitUntil(async () => (await browser.getWindowHandles()).length > 1, {
      timeout: 10000,
      timeoutMsg: '备份窗口未打开',
    });
    const handles = await browser.getWindowHandles();
    await browser.switchToWindow(handles.find((h) => h !== mainWindow)!);
    await browser.pause(1200);
    await browser.execute(() => {
      const input = Array.from(document.querySelectorAll('input')) as HTMLInputElement[];
      const nameInput = input.find((i) => (i.value || '') === 'untitled');
      if (nameInput) {
        const setter = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value')?.set;
        setter?.call(nameInput, 'test_orders_2026-08-23');
        nameInput.dispatchEvent(new Event('input', { bubbles: true }));
      }
    });
    await browser.pause(500);
    await shot('19-backup-sync.png');
    await browser.closeWindow();
    await browser.switchToWindow(mainWindow);
  });

  // ─────────────────────── 26/27/28 sub-windows ────────────────────────────

  it('26-data-sync / 27-schema-diff / 28-data-transfer windows', async () => {
    await goToConnections();
    await ensureDemoPgConnectedInTree();

    // 数据同步 (compare data): demo PG primary ↔ analytics DB.
    try {
      await openDbContextMenu(DEMO_PG_DB, '比较数据');
      let handles = await browser.getWindowHandles();
      await browser.waitUntil(async () => (await browser.getWindowHandles()).length > 1, {
        timeout: 10000,
        timeoutMsg: '数据同步窗口未打开',
      });
      handles = await browser.getWindowHandles();
      await browser.switchToWindow(handles.find((h) => h !== mainWindow)!);
      await browser.pause(1200);
      await shot('26-data-sync.png');
      await browser.closeWindow();
      await browser.switchToWindow(mainWindow);
    } catch (e) {
      console.warn(`[warn] 26-data-sync skipped: ${e}`);
      if ((await browser.getWindowHandles()).length > 1) {
        await browser.closeWindow();
        await browser.switchToWindow(mainWindow);
      }
    }

    // 结构对比 (schema diff): hidden from ctx menu when schemaDiff flag is off — open sub-window via IPC.
    try {
      await browser.switchToWindow(mainWindow);
      await goToConnections();
      await invoke('create_sub_window', {
        options: {
          label: 'schema-diff-singleton',
          url: 'window.html?window=schema-diff',
          title: '结构对比 - DataZen',
          width: 900,
          height: 640,
        },
      });
      await browser.waitUntil(async () => (await browser.getWindowHandles()).length > 1, {
        timeout: 10000,
        timeoutMsg: '结构对比窗口未打开',
      });
      const hs = await browser.getWindowHandles();
      await browser.switchToWindow(hs.find((h) => h !== mainWindow)!);
      await browser.pause(1200);
      await shot('27-schema-diff.png');
      await browser.closeWindow();
      await browser.switchToWindow(mainWindow);
    } catch (e) {
      console.warn(`[warn] 27-schema-diff skipped: ${e}`);
      if ((await browser.getWindowHandles()).length > 1) {
        await browser.closeWindow();
        await browser.switchToWindow(mainWindow);
      }
    }

    // 数据传输 (data transfer).
    try {
      await openDbContextMenu(DEMO_PG_DB, '数据传输');
      await browser.waitUntil(async () => (await browser.getWindowHandles()).length > 1, {
        timeout: 10000,
        timeoutMsg: '数据传输窗口未打开',
      });
      const hs = await browser.getWindowHandles();
      await browser.switchToWindow(hs.find((h) => h !== mainWindow)!);
      await browser.pause(1200);
      await shot('28-data-transfer.png');
      await browser.closeWindow();
      await browser.switchToWindow(mainWindow);
    } catch (e) {
      console.warn(`[warn] 28-data-transfer skipped: ${e}`);
      if ((await browser.getWindowHandles()).length > 1) {
        await browser.closeWindow();
        await browser.switchToWindow(mainWindow);
      }
    }
  });

  // ─────────────────────── 24-server-status + 25-processes ─────────────────

  it('24-server-status + 25-processes panels', async () => {
    await goToConnections();
    await ensureDemoPgConnectedInTree();

    // Both are connection-level panels opened from the connection ctx menu;
    // they render inside the main window as regular panels.
    const openPanelViaConnMenu = async (itemText: string) => {
      const clicked = await browser.execute((name: string) => {
        const conn = Array.from(document.querySelectorAll('[data-conn-item]')).find((el) =>
          (el.getAttribute('data-conn-name') || '').includes(name),
        ) as HTMLElement | null;
        if (!conn) return false;
        conn.dispatchEvent(
          new MouseEvent('contextmenu', { bubbles: true, clientX: 220, clientY: 180 }),
        );
        return true;
      }, DEMO_PG_CONN_NAME);
      if (!clicked) throw new Error(`${DEMO_PG_CONN_NAME} row not found`);
      await browser.pause(600);
      const hit = await browser.execute((text: string) => {
        const items = Array.from(
          document.querySelectorAll('[data-testid="web-context-menu"] *, [role="menuitem"]'),
        );
        const item = items.find((el) => (el.textContent || '').includes(text));
        if (!item) return false;
        (item as HTMLElement).click();
        return true;
      }, itemText);
      if (!hit) throw new Error(`connection menu item ${itemText} not found`);
      await browser.pause(1500);
    };

    try {
      await openPanelViaConnMenu('服务器状态');
      await shot('24-server-status.png', 800);
    } catch (e) {
      console.warn(`[warn] 24-server-status skipped: ${e}`);
    }

    try {
      await openPanelViaConnMenu('进程列表');
      await shot('25-processes.png', 800);
    } catch (e) {
      console.warn(`[warn] 25-processes skipped: ${e}`);
    }
    await goToConnections();
  });

  // ─────────────────────── 23-table-structure ──────────────────────────────

  it('23-table-structure: structure view with indexes and DDL', async () => {
    await ensureDemoPgConnectedInTree();
    await expandDemoDbTables(DEMO_PG_DB);

    // Right-click the ER-rich demo_products table → 打开结构.
    let opened = false;
    const dl = Date.now();
    while (Date.now() - dl < 15000 && !opened) {
      opened = await browser.execute(() => {
        const nodes = Array.from(
          document.querySelectorAll<HTMLElement>(
            'button[data-tree-node="table"][data-item-name="demo_products"]',
          ),
        );
        let el = nodes[0];
        if (!el) {
          const scrollers = Array.from(
            document.querySelectorAll<HTMLElement>(
              '[class*="overflow-y-auto"], [class*="overflow-auto"]',
            ),
          ).filter((s) => s.scrollHeight > s.clientHeight);
          for (const s of scrollers) s.scrollTop = Math.max(0, s.scrollTop - 300);
          el = Array.from(
            document.querySelectorAll<HTMLElement>(
              'button[data-tree-node="table"][data-item-name="demo_products"]',
            ),
          )[0];
        }
        if (!el) return false;
        el.scrollIntoView({ block: 'center' });
        el.dispatchEvent(
          new MouseEvent('contextmenu', { bubbles: true, clientX: 320, clientY: 320 }),
        );
        return true;
      });
      if (!opened) await browser.pause(250);
    }
    if (!opened) throw new Error('demo_products node not found for structure view');
    await browser.pause(600);
    const hit = await browser.execute(() => {
      const items = Array.from(
        document.querySelectorAll('[data-testid="web-context-menu"] *, [role="menuitem"]'),
      );
      const item = items.find((el) => (el.textContent || '').includes('打开结构'));
      if (!item) return false;
      (item as HTMLElement).click();
      return true;
    });
    if (!hit) throw new Error('打开结构 menu item not found');
    // Structure tab mounts with column grid + DDL section; settle generously.
    await browser.waitUntil(
      async () =>
        browser.execute(() => {
          const text = document.body.textContent || '';
          return text.includes('DDL') && text.includes('id');
        }),
      { timeout: 20000, timeoutMsg: 'structure view did not render' },
    );
    await shot('23-table-structure.png', 900);
  });

  // ─────────────────────── 18-data-editor ──────────────────────────────────

  it('18-data-editor: inline cell editing state', async () => {
    await pinDemoPgDatabase();
    await goToConnections();
    await expandDemoDbTables(DEMO_PG_DB);
    await pinDemoPgDatabase(); // re-pin after tree expansion switched session

    // Open demo_customers data grid (double-click its tree node).
    let opened = false;
    const dl = Date.now();
    while (Date.now() - dl < 15000 && !opened) {
      opened = await browser.execute(() => {
        const el = Array.from(
          document.querySelectorAll<HTMLElement>(
            'button[data-tree-node="table"][data-item-name="demo_customers"]',
          ),
        )[0];
        if (!el) {
          const scrollers = Array.from(
            document.querySelectorAll<HTMLElement>(
              '[class*="overflow-y-auto"], [class*="overflow-auto"]',
            ),
          ).filter((s) => s.scrollHeight > s.clientHeight);
          for (const s of scrollers) s.scrollTop = Math.max(0, s.scrollTop - 300);
          return !!document.querySelector(
            'button[data-tree-node="table"][data-item-name="demo_customers"]',
          );
        }
        el.scrollIntoView({ block: 'center' });
        el.dispatchEvent(new MouseEvent('dblclick', { bubbles: true, cancelable: true }));
        return true;
      });
      if (!opened) await browser.pause(250);
    }
    if (!opened) throw new Error('demo_customers not found in sidebar');

    // Wait for rows, then double-click a numeric cell to enter edit mode.
    await browser.waitUntil(async () => (await countDtRows()) >= 3, {
      timeout: 20000,
      timeoutMsg: 'demo_customers rows not rendered',
    });
    // Double-click the first editable data cell of the first row (skip any
    // leading checkbox/select column). Generic on purpose — cell rendering
    // differs between table implementations.
    const editing = await browser.execute(() => {
      const row =
        document.querySelector<HTMLElement>('[data-dt-row]') ??
        document.querySelector<HTMLElement>('table tbody tr');
      if (!row) return false;
      const cells = Array.from(row.querySelectorAll('td, span[title]'));
      const target =
        cells.find((c) => /^\d[\d,]*$/.test((c.textContent || '').trim())) ??
        cells.find((c) => c.tagName === 'TD') ??
        cells[1] ??
        cells[0];
      if (!target) return false;
      target.scrollIntoView({ block: 'center' });
      target.dispatchEvent(new MouseEvent('dblclick', { bubbles: true, cancelable: true }));
      return true;
    });
    if (!editing) throw new Error('no editable cell found in first data row');
    await browser.waitUntil(
      async () => browser.execute(() => !!document.querySelector('input.font-mono')),
      { timeout: 8000, timeoutMsg: 'inline edit input did not appear' },
    );
    await shot('18-data-editor.png');
    // Escape without saving to keep demo data intact.
    await browser.keys(['Escape']);
    await browser.pause(400);
  });

  // ─────────────────────── 15-redis ────────────────────────────────────────

  it('15-redis: KV browser with seeded demo keys', async () => {
    // Connect the Redis demo connection directly. The local dev instance may
    // require a password — fall back to E2E_REDIS_PASSWORD from e2e/.env.
    let connId: unknown = await invoke<string>('connect', { connectionId: DEMO_REDIS_CONN_ID });
    if (typeof connId !== 'string' || String(connId).startsWith('__error')) {
      const config: Record<string, unknown> = {
        id: DEMO_REDIS_CONN_ID,
        name: DEMO_REDIS_CONN_NAME,
        databaseType: 'redis',
        host: process.env.E2E_REDIS_HOST || '127.0.0.1',
        port: Number(process.env.E2E_REDIS_PORT) || 6379,
        username: '',
        password: process.env.E2E_REDIS_PASSWORD || '',
        database: DEMO_REDIS_DB,
      };
      const saved = await invoke('save_connection', { config });
      if (saved && typeof saved === 'object' && '__error' in (saved as object)) {
        throw new Error(`connect(${DEMO_REDIS_CONN_ID}) failed: ${JSON.stringify(connId)}`);
      }
      connId = await invoke<string>('connect', { connectionId: DEMO_REDIS_CONN_ID });
      if (typeof connId !== 'string' || String(connId).startsWith('__error')) {
        throw new Error(`connect(${DEMO_REDIS_CONN_ID}) failed: ${JSON.stringify(connId)}`);
      }
    }
    await goToConnections();
    await browser.pause(500);

    // Expand the Redis connection so kv-db leaf nodes mount.
    await browser.execute((connName: string) => {
      const items = Array.from(document.querySelectorAll('[data-conn-item]'));
      const target = items.find((el) =>
        (el.getAttribute('data-conn-name') || '').includes(connName),
      );
      const chev = Array.from(target?.querySelectorAll('button') ?? []).find(
        (b) =>
          !!b.querySelector('svg.lucide-chevron-right') ||
          !!b.querySelector('svg.lucide-chevron-down'),
      );
      (chev as HTMLElement | undefined)?.click();
    }, DEMO_REDIS_CONN_NAME);

    // Click db5 (or the first mounted kv-db node).
    let openedDb = false;
    const dl = Date.now();
    while (Date.now() - dl < 15000 && !openedDb) {
      openedDb = await browser.execute((prefDb: string) => {
        const preferred = document.querySelector<HTMLElement>(
          `button[data-tree-node="kv-db"][data-db-name="${prefDb}"]`,
        );
        const any =
          preferred ?? document.querySelector<HTMLElement>('button[data-tree-node="kv-db"]');
        if (!any) {
          const scrollers = Array.from(
            document.querySelectorAll<HTMLElement>(
              '[class*="overflow-y-auto"], [class*="overflow-auto"]',
            ),
          ).filter((s) => s.scrollHeight > s.clientHeight);
          for (const s of scrollers) s.scrollTop = Math.max(0, s.scrollTop - 300);
          return false;
        }
        any.scrollIntoView({ block: 'center' });
        any.click();
        return true;
      }, DEMO_REDIS_DB);
      if (!openedDb) await browser.pause(400);
    }
    if (!openedDb) throw new Error('no kv-db node rendered for redis connection');

    // Wait for keys to list (demo: prefix) then shoot.
    await browser.waitUntil(
      async () =>
        browser.execute(() => (document.body.textContent || '').includes('demo:app:name')),
      { timeout: 20000, timeoutMsg: 'redis demo keys not listed' },
    );
    await shot('15-redis.png', 900);
    await goToConnections();
  });

  // ─────────────────────── 20-security (行为/安全设置) ─────────────────────

  it('20-security: behavior & safe-mode settings', async () => {
    await openSettingsSection('behavior');
    await shot('20-security.png');
  });

  // ─────────────────────── 09-ai-settings ──────────────────────────────────

  it('09-ai-more: AI settings', async () => {
    await goToSettings();
    await browser.execute(() => {
      const items = Array.from(document.querySelectorAll('button, [role="button"], a, div'));
      const item = items.find((el) => (el.textContent || '').trim() === 'AI 助手');
      (item as HTMLElement | undefined)?.click();
    });
    await browser.pause(1000);
    await shot('09-ai-more.png');
  });
});
