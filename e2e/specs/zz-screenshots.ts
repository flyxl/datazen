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

async function waitResults(minRows = 1, timeout = 20000) {
  await browser.waitUntil(
    async () => {
      // DataTable uses virtual div rows with [data-dt-row], not <table>/<tbody>/<tr>.
      const rows = await browser.execute(() => document.querySelectorAll('[data-dt-row]').length);
      return rows >= minRows;
    },
    { timeout, timeoutMsg: `结果行数未达到 ${minRows}` },
  );
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

async function clickToolbarButton(text: string) {
  const ok = await browser.execute((t: string) => {
    const btn = Array.from(document.querySelectorAll('button')).find(
      (b) => (b.textContent || '').includes(t) || (b.getAttribute('aria-label') || '').includes(t),
    );
    if (btn) {
      btn.click();
      return true;
    }
    return false;
  }, text);
  if (!ok) throw new Error(`toolbar button ${text} not found`);
  await browser.pause(600);
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

// ── Demo workflow YAML ──

const DEMO_WORKFLOW_YAML = `id: cross-db-order-logistics
name: 跨库订单物流演示
description: PostgreSQL 订单库 × MySQL 物流库联合查询，一步获取用户最新订单与物流状态
timeout_secs: 60
error_handling:
  strategy: abort
connection: conn_mq798cbn
steps:
  - type: query
    id: orders
    sql: "SELECT order_id, product_name, amount, status FROM public.test_orders WHERE uid = 'U001' ORDER BY created_at DESC"
    timeout_secs: 10
  - type: query
    id: logistics
    connection: conn_3lg2w19n
    sql: "SELECT order_id, carrier, tracking_no, status FROM datazen_test.test_logistics WHERE order_id IN ('ORD-2026-005','ORD-2026-002','ORD-2026-001')"
    timeout_secs: 10
`;

// ── Screenshot cases ──

describe('site screenshots', () => {
  let mainWindow: string;

  before(async () => {
    mainWindow = await browser.getWindowHandle();
    await setWindowSize();

    // Wait for the connection tree to be populated.
    await browser.waitUntil(async () => (await browser.$$('[data-conn-item]')).length > 0, {
      timeout: 15000,
      timeoutMsg: 'no data-conn-item found on startup',
    });

    // Remove E2E-seeded connection.
    await invoke('delete_connection', { id: 'conn_e2e_pg' });
    await browser.pause(500);

    // Ensure a Redis connection exists.
    await invoke('save_connection', {
      config: {
        id: 'conn_redis_local',
        name: 'Redis 本地',
        databaseType: 'redis',
        host: '127.0.0.1',
        port: 6379,
        username: '',
        password: '',
        database: '',
        group: 'preset:development',
        colorTag: '#ef4444',
        sslMode: 'disable',
      },
    });
    await browser.pause(600);

    // Pre-create the demo workflow.
    await invoke('workflow_save_yaml', { yaml: DEMO_WORKFLOW_YAML });
    await browser.pause(400);

    // Ensure PG-Local is connected and on `postgres` database.
    // The wdio before() may have already connected it; we re-connect to be sure.
    const connId = await invoke<string>('connect', { configId: 'conn_mq798cbn' });
    if (connId && typeof connId === 'string' && !connId.startsWith('__error')) {
      await invoke('use_database', { connectionId: connId, database: 'postgres' });
    }
    await browser.pause(500);
  });

  // ─────────────────────── 01-main-window ─────────────────────────────────

  it('01-main-window: connection manager', async () => {
    await goToConnections();
    await shot('01-main-window.png');
  });

  // ─────────────────────── 14-multidb ──────────────────────────────────────

  it('14-multidb: multi-database tree', async () => {
    await goToConnections();
    await browser.pause(500);

    // Ensure PG-Local is connected and databases are visible.
    const dbsVisible = await browser.execute(
      () => document.querySelectorAll('button[data-tree-node="db"]').length > 0,
    );
    if (!dbsVisible) {
      // Click PG-Local chevron to connect/expand.
      await browser.execute(() => {
        const items = Array.from(document.querySelectorAll('[data-conn-item]'));
        const pg = items.find((el) =>
          (el.getAttribute('data-conn-name') || '').includes('PG-Local'),
        );
        const btn = pg?.querySelector('button[aria-expanded]');
        btn?.click();
      });
      await browser.waitUntil(
        async () =>
          browser.execute(
            () => document.querySelectorAll('button[data-tree-node="db"]').length > 0,
          ),
        { timeout: 20000, timeoutMsg: 'databases not visible after expand' },
      );
      await browser.pause(500);
    }

    // Click postgres database to expand tables. Use scrollIntoView to ensure
    // the button is rendered by the virtual list before clicking.
    await browser.execute(() => {
      const btn = document.querySelector(
        'button[data-tree-node="db"][data-db-name="postgres"]',
      ) as HTMLElement | null;
      if (btn) {
        btn.scrollIntoView({ block: 'center' });
      }
    });
    await browser.pause(300);

    // Check if tables are already visible under postgres (virtual list may
    // only show a few rows, so check the first table button exists).
    const hasTables = await browser.execute(
      () => !!document.querySelector('button[data-tree-node="table"]'),
    );
    if (!hasTables) {
      // Click to expand.
      await browser.execute(() => {
        const btn = document.querySelector(
          'button[data-tree-node="db"][data-db-name="postgres"]',
        ) as HTMLElement | null;
        btn?.click();
      });
      await browser.waitUntil(
        async () =>
          browser.execute(() => !!document.querySelector('button[data-tree-node="table"]')),
        { timeout: 20000, timeoutMsg: 'tables not listed under postgres' },
      );
    }
    await shot('14-multidb.png');
  });

  // ─────────────────────── 17-sql-editor ───────────────────────────────────

  it('17-sql-editor', async () => {
    await goToConnections();
    await browser.pause(500);

    // Ensure PG-Local connection tab is active by clicking it in the sidebar.
    // Without an active tab, the toolbar with 新建查询 won't be visible.
    await browser.execute(() => {
      const items = Array.from(document.querySelectorAll('[data-conn-item]'));
      const pg = items.find((el) => (el.getAttribute('data-conn-name') || '').includes('PG-Local'));
      if (pg) pg.click();
    });
    await browser.pause(600);

    await newQueryTab();
    await setEditorContent(
      '-- 近期销售明细\nSELECT sale_date,\n       category,\n       region,\n       amount,\n       quantity\nFROM demo_sales\nORDER BY sale_date DESC, amount DESC\nLIMIT 20;',
    );
    await clickExecute();
    await waitResults(10);
    await shot('17-sql-editor.png');
  });

  // ─────────────────────── 02-query-chart + 10-chart-types ─────────────────

  it('02-query-chart + 10-chart-types', async () => {
    await newQueryTab();
    await setEditorContent(
      'SELECT sale_date,\n       SUM(amount) AS total_amount,\n       SUM(quantity) AS total_quantity\nFROM demo_sales\nGROUP BY sale_date\nORDER BY sale_date;',
    );
    await clickExecute();
    await waitResults(15);
    await clickToolbarButton('图表');
    await shot('02-query-chart.png');
    await clickToolbarButton('柱状图');
    await shot('10-chart-types.png');
  });

  // ─────────────────────── 11-chart-export: pie ────────────────────────────

  it('11-chart-export: pie by category', async () => {
    await newQueryTab();
    await setEditorContent(
      'SELECT category,\n       SUM(amount) AS total_amount\nFROM demo_sales\nGROUP BY category\nORDER BY total_amount DESC;',
    );
    await clickExecute();
    await waitResults(4);
    await clickToolbarButton('图表');
    await clickToolbarButton('饼图');
    await shot('11-chart-export.png');
  });

  // ─────────────────────── 16-er ───────────────────────────────────────────

  it('16-er: ER diagram with relations', async () => {
    await clickToolbarButton('ER 图');
    await browser.waitUntil(
      async () => {
        const n = await browser.execute(
          () => document.querySelectorAll('.react-flow__node').length,
        );
        return n >= 6;
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

  // ─────────────────────── 08-ai-filter ────────────────────────────────────

  it('08-ai-filter: natural language filter', async () => {
    // Ensure connection is active and sidebar is visible.
    await goToConnections();
    await browser.pause(300);
    await browser.execute(() => {
      const items = Array.from(document.querySelectorAll('[data-conn-item]'));
      const pg = items.find((el) => (el.getAttribute('data-conn-name') || '').includes('PG-Local'));
      if (pg) pg.click();
    });
    await browser.pause(400);

    // Ensure postgres DB is expanded and demo_sales table is visible.
    await browser.execute(() => {
      const btn = document.querySelector(
        'button[data-tree-node="db"][data-db-name="postgres"]',
      ) as HTMLElement | null;
      if (btn) btn.click();
    });
    await browser.pause(500);

    // Click demo_sales table in the sidebar.
    const clicked = await browser.execute(() => {
      const nodes = Array.from(
        document.querySelectorAll('button[data-tree-node="table"], button[data-tree-node="view"]'),
      );
      const el = nodes.find(
        (n) => (n.getAttribute('data-item-name') || '').trim() === 'demo_sales',
      );
      if (!el) return false;
      (el as HTMLElement).click();
      return true;
    });
    if (!clicked) throw new Error('demo_sales not found in sidebar');
    await browser.pause(1500);

    // The NL filter starts collapsed (Sparkles icon). Click it to expand.
    await browser.execute(() => {
      const sparkles = document.querySelector(
        'button[aria-label*="筛选"], button[aria-label*="Filter"], button[title*="筛选"], button[title*="Filter"]',
      ) as HTMLElement | null;
      sparkles?.click();
    });
    await browser.pause(500);

    const typed = await browser.execute(() => {
      const inputs = Array.from(document.querySelectorAll('input')) as HTMLInputElement[];
      const input = inputs.find(
        (i) =>
          (i.placeholder || '').includes('自然语言') ||
          (i.placeholder || '').includes('筛选') ||
          (i.placeholder || '').includes('Describe'),
      );
      if (!input) return false;
      const setter = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value')?.set;
      setter?.call(input, '金额大于 800 且地区是华东');
      input.dispatchEvent(new Event('input', { bubbles: true }));
      return true;
    });
    if (!typed) throw new Error('NL filter input not found');
    await browser.pause(300);
    await clickToolbarButton('筛选');
    await browser.waitUntil(
      async () => browser.execute(() => (document.body.textContent || '').includes('amount gt')),
      { timeout: 60000, timeoutMsg: '筛选条件未解析' },
    );
    await browser.waitUntil(
      async () => {
        const rows = await browser.execute(
          () =>
            document.querySelectorAll('[data-dt-row]').length ||
            document.querySelectorAll('table tbody tr').length,
        );
        return rows > 0 && rows < 50;
      },
      { timeout: 30000, timeoutMsg: '筛选结果未出现' },
    );
    await shot('08-ai-filter.png');
  });

  // ─────────────────────── 07-ai-chat ──────────────────────────────────────

  it('07-ai-chat: assistant conversation', async () => {
    // Ensure the connection toolbar is visible.
    await goToConnections();
    await browser.pause(300);
    await browser.execute(() => {
      const items = Array.from(document.querySelectorAll('[data-conn-item]'));
      const pg = items.find((el) => (el.getAttribute('data-conn-name') || '').includes('PG-Local'));
      if (pg) pg.click();
    });
    await browser.pause(500);

    // The AI chat button is a MessageSquare icon button in the toolbar.
    // It's the second-to-last button in the toolbar row (before detail panel toggle).
    await browser.execute(() => {
      const toolbar = document.querySelector('[class*="bg-surface-alt"][class*="border-b"]');
      if (!toolbar) return;
      const btns = Array.from(toolbar.querySelectorAll('button'));
      // AI chat button is typically the second-to-last (before detail toggle).
      const aiBtn = btns.length >= 2 ? btns[btns.length - 2] : btns[btns.length - 1];
      aiBtn?.click();
    });
    await browser.pause(800);
    const asked = await browser.execute(() => {
      const ta = Array.from(document.querySelectorAll('textarea')) as HTMLTextAreaElement[];
      const input = ta.find(
        (t) => (t.placeholder || '').includes('输入') || (t.placeholder || '').includes('问'),
      );
      if (!input) return false;
      input.focus();
      const setter = Object.getOwnPropertyDescriptor(HTMLTextAreaElement.prototype, 'value')?.set;
      setter?.call(input, 'demo_sales 里哪个分类的销售额最高？总额是多少？');
      input.dispatchEvent(new Event('input', { bubbles: true }));
      return true;
    });
    if (!asked) throw new Error('AI input not found');
    await browser.keys('Enter');
    await browser.waitUntil(
      async () => {
        const st = await browser.execute(() => {
          const text = document.body.textContent || '';
          const streaming = text.includes('停止') || !!document.querySelector('.animate-spin');
          return { done: text.includes('电子产品') && !streaming, streaming };
        });
        return st.done;
      },
      { timeout: 90000, timeoutMsg: 'AI 回复超时' },
    );
    await browser.pause(1500);
    await shot('07-ai-chat.png');
  });

  // ─────────────────────── 04-workflow ─────────────────────────────────────

  it('04-workflow: cross-db workflow with results', async () => {
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

    // Click Execute.
    await browser.execute(() => {
      const btn = Array.from(document.querySelectorAll('button')).find(
        (b) => (b.textContent || '').trim() === '执行' && !b.hasAttribute('disabled'),
      );
      btn?.click();
    });
    await browser.waitUntil(
      async () => {
        const rows = await browser.execute(
          () =>
            document.querySelectorAll('[data-dt-row]').length ||
            document.querySelectorAll('table tbody tr').length,
        );
        return rows >= 3;
      },
      { timeout: 30000, timeoutMsg: 'workflow 结果未出现' },
    );
    await shot('04-workflow.png', 1000);
  });

  // ─────────────────────── 19-backup-sync ──────────────────────────────────

  it('19-backup-sync: backup window prefilled', async () => {
    await browser.switchToWindow(mainWindow);
    await goToConnections();
    await browser.pause(500);

    // Ensure postgres DB node is visible in tree.
    const postgresVisible = await browser.execute(
      () => !!document.querySelector('button[data-tree-node="db"][data-db-name="postgres"]'),
    );
    if (!postgresVisible) {
      // Expand PG-Local to show databases.
      await browser.execute(() => {
        const items = Array.from(document.querySelectorAll('[data-conn-item]'));
        const pg = items.find((el) =>
          (el.getAttribute('data-conn-name') || '').includes('PG-Local'),
        );
        const btn = pg?.querySelector('button[aria-expanded]');
        btn?.click();
      });
      await browser.waitUntil(
        async () =>
          browser.execute(
            () => !!document.querySelector('button[data-tree-node="db"][data-db-name="postgres"]'),
          ),
        { timeout: 20000 },
      );
    }

    // Right-click the postgres database node.
    const ctx = await browser.execute(() => {
      const node = document.querySelector(
        'button[data-tree-node="db"][data-db-name="postgres"]',
      ) as HTMLElement | null;
      if (!node) return false;
      node.dispatchEvent(
        new MouseEvent('contextmenu', { bubbles: true, clientX: 200, clientY: 200 }),
      );
      return true;
    });
    if (!ctx) throw new Error('postgres db node not found');
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
