/**
 * Editor Features Screenshot Generator.
 *
 * Drives the webdriver-enabled app to capture marketing and documentation
 * screenshots of the latest SQL Editor features in DataZen v0.2.0.
 *
 * Outputs directly to:
 * - site/assets/screenshots/
 * - docs/release-notes/screenshots/
 */
import { browser, $ } from '@wdio/globals';
import fs from 'node:fs';
import path from 'node:path';
import url from 'node:url';

const ROOT = path.resolve(path.dirname(url.fileURLToPath(import.meta.url)), '..', '..');
const OUT_SITE = path.join(ROOT, 'site', 'assets', 'screenshots');
const OUT_DOCS = path.join(ROOT, 'docs', 'release-notes', 'screenshots');

const DEMO_PG_DB = process.env.E2E_DEMO_PG_DB || 'datazen_demo';
const DEMO_PG_USER = process.env.E2E_DEMO_PG_USER || 'datazen_demo';
const DEMO_PG_PASSWORD = process.env.E2E_DEMO_PG_PASSWORD || 'datazen_demo';
const DEMO_PG_CONN_ID = 'conn_demo_pg';
const DEMO_PG_CONN_NAME = '演示 PostgreSQL';

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

async function shot(name: string, settleMs = 800) {
  await browser.pause(settleMs);
  fs.mkdirSync(OUT_SITE, { recursive: true });
  fs.mkdirSync(OUT_DOCS, { recursive: true });

  const sitePath = path.join(OUT_SITE, name);
  const docsPath = path.join(OUT_DOCS, name);

  await browser.saveScreenshot(sitePath);
  fs.copyFileSync(sitePath, docsPath);

  const size = fs.statSync(sitePath).size;
  console.log(`[shot] ${name} (${size} bytes) -> saved to site & docs`);
}

async function setWindowSize(w = 2200, h = 1400) {
  await invoke('plugin:window|set_size', { size: { width: w, height: h } });
  await browser.pause(600);
}

async function setEditorContent(text: string) {
  const editor = $('.cm-editor .cm-content');
  await editor.waitForDisplayed({ timeout: 10000 });
  await editor.click();
  await browser.waitUntil(
    async () =>
      browser.execute(
        () =>
          document.activeElement?.closest('.cm-editor .cm-content') != null &&
          document.querySelector('[id^="dz-select-listbox-"]') == null,
      ),
    { timeout: 3000, timeoutMsg: 'editor focus did not settle' },
  );
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
  await browser.pause(400);
}

async function clickExecute() {
  const btn = await $('[data-testid="editor-execute-button"]');
  await btn.waitForDisplayed({ timeout: 5000 });
  await btn.click();
}

async function waitResults(minRows = 1, timeout = 10000) {
  await browser.waitUntil(
    async () => {
      const probe = await browser.execute(() => {
        const table = document.querySelector('[data-testid="result-workspace-table"]');
        if (!table) return { ok: false };
        const text = document.body.textContent || '';
        const match = text.match(/(\d+)\s*行/);
        const rows = match ? parseInt(match[1], 10) : 0;
        return { ok: rows >= 1 || text.includes('行'), rows };
      });
      return probe.ok;
    },
    { timeout, timeoutMsg: `结果未渲染` },
  );
}

async function clickExecuteWithRetry(minRows: number, attempts = 3) {
  for (let i = 0; i < attempts; i++) {
    await clickExecute();
    try {
      await waitResults(minRows, 8000);
      return;
    } catch (e) {
      if (i === attempts - 1) throw e;
      await browser.pause(500);
    }
  }
}

async function selectQueryPanelDatabase(dbName: string) {
  await browser.pause(500);
  const hostExists = await browser.execute(() => {
    return document.querySelector('[data-testid="query-context-selectors"]') != null;
  });
  if (!hostExists) {
    // Single database mode — already on configured database
    return;
  }

  // If already selected, skip
  const alreadySelected = await browser.execute((target: string) => {
    const host = document.querySelector('[data-testid="query-context-selectors"]');
    const text = host?.textContent || '';
    return text.includes(target);
  }, dbName);
  if (alreadySelected) {
    await browser.pause(400);
    return;
  }

  // Open dropdown and pick target
  await browser.waitUntil(
    async () => {
      const opened = await browser.execute(() => {
        if (document.querySelector('[id^="dz-select-listbox-"]')) return true;
        const host = document.querySelector('[data-testid="query-context-selectors"]');
        const btn = host?.querySelector('button[aria-haspopup="listbox"]') as HTMLElement | null;
        if (!btn) return false;
        btn.click();
        return false;
      });
      return opened;
    },
    { timeout: 5000, timeoutMsg: `db selector trigger not found for ${dbName}` },
  );

  let picked = false;
  const dl = Date.now();
  while (Date.now() - dl < 8000 && !picked) {
    const probe = await browser.execute((target: string) => {
      const list = document.querySelector('[id^="dz-select-listbox-"]');
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
    if (probe.state === 'selected' || probe.state === 'picked') picked = true;
    else {
      await browser.pause(300);
      await browser.execute(() => {
        if (document.querySelector('[id^="dz-select-listbox-"]')) return;
        const host = document.querySelector('[data-testid="query-context-selectors"]');
        const btn = host?.querySelector('button[aria-haspopup="listbox"]') as HTMLElement | null;
        btn?.click();
      });
      await browser.pause(200);
    }
  }
  await browser.pause(800);
}

async function newQueryTab() {
  await browser.execute(() => {
    const btn = Array.from(document.querySelectorAll('button')).find((b) =>
      (b.textContent || '').includes('新建查询'),
    );
    btn?.click();
  });
  await browser.pause(800);
}

async function ensureDemoPgConnectedInTree() {
  await browser.waitUntil(
    async () =>
      browser.execute(
        (connName: string) =>
          !!Array.from(document.querySelectorAll('[data-conn-item]')).find((el) =>
            (el.getAttribute('data-conn-name') || '').includes(connName),
          ),
        DEMO_PG_CONN_NAME,
      ),
    { timeout: 15000, timeoutMsg: `${DEMO_PG_CONN_NAME} not in navigator` },
  );

  // Click connection item to activate/expand
  await browser.execute((name: string) => {
    const item = Array.from(document.querySelectorAll<HTMLElement>('[data-conn-item]')).find((el) =>
      (el.getAttribute('data-conn-name') || '').includes(name),
    );
    item?.click();
  }, DEMO_PG_CONN_NAME);
  await browser.pause(800);
}

async function expandDemoDbTables() {
  // Expand demo DB node if not expanded
  await browser.execute((db: string) => {
    const dbBtn = Array.from(
      document.querySelectorAll<HTMLElement>('button[data-tree-node="db"]'),
    ).find((el) => el.getAttribute('data-db-name') === db);
    if (dbBtn && !dbBtn.querySelector('svg.lucide-chevron-down')) {
      dbBtn.click();
    }
  }, DEMO_PG_DB);
  await browser.pause(600);

  // Expand public schema
  await browser.execute(() => {
    const schemaBtn = Array.from(
      document.querySelectorAll<HTMLElement>('button[data-tree-node="schema"]'),
    ).find((el) => el.getAttribute('data-schema-name') === 'public');
    if (schemaBtn && !schemaBtn.querySelector('svg.lucide-chevron-down')) {
      schemaBtn.click();
    }
  });
  await browser.pause(600);

  // Expand 表 category
  await browser.execute(() => {
    const catBtn = Array.from(
      document.querySelectorAll<HTMLElement>('button[data-tree-node="category"]'),
    ).find((el) => (el.textContent || '').includes('表'));
    if (catBtn && !catBtn.querySelector('svg.lucide-chevron-down')) {
      catBtn.click();
    }
  });
  await browser.pause(600);
}

describe('SQL Editor Latest Features Screenshots', () => {
  before(async () => {
    fs.mkdirSync(OUT_SITE, { recursive: true });
    fs.mkdirSync(OUT_DOCS, { recursive: true });

    // Save demo PostgreSQL connection explicitly pointing to DEMO_PG_DB
    await invoke('save_connection', {
      config: {
        id: DEMO_PG_CONN_ID,
        name: DEMO_PG_CONN_NAME,
        databaseType: 'postgresql',
        host: process.env.E2E_PG_HOST || '127.0.0.1',
        port: Number(process.env.E2E_PG_PORT) || 5432,
        username: DEMO_PG_USER,
        password: DEMO_PG_PASSWORD,
        database: DEMO_PG_DB,
        group: 'preset:development',
        colorTag: '#3b82f6',
        sslMode: 'disable',
      },
    });

    const connId = await invoke<string>('connect', { connectionId: DEMO_PG_CONN_ID });
    if (typeof connId === 'string' && !connId.startsWith('__error')) {
      await invoke('get_tables', { dbSessionId: connId, database: DEMO_PG_DB });
    }

    // Set UI language to zh-CN, disable safeMode, and set strategy to current_statement
    const settings = await invoke<Record<string, unknown>>('get_settings');
    await invoke('save_settings', {
      settings: {
        ...settings,
        language: 'zh-CN',
        safeMode: false,
        sqlExecutionStrategy: 'current_statement',
      },
    });

    await browser.url('tauri://localhost');
    await browser.pause(1500);
    await setWindowSize(2200, 1400);
    await browser.pause(1000);
  });

  // ── 1. 最新 SQL 编辑器主界面 ──────────────────────────────────
  it('17-sql-editor: captures main SQL editor with modern toolbar and results', async () => {
    await ensureDemoPgConnectedInTree();
    await expandDemoDbTables();
    await newQueryTab();
    await selectQueryPanelDatabase(DEMO_PG_DB);
    await browser.pause(500);

    const sql = `-- 1. 销售趋势分析：按日期与金额降序排序
SELECT sale_date,
       category,
       region,
       amount,
       quantity
FROM demo_sales
ORDER BY sale_date DESC, amount DESC
LIMIT 20;`;

    await setEditorContent(sql);
    await clickExecuteWithRetry(1);
    await browser.pause(1200);
    await shot('17-sql-editor.png');
  });

  // ── 2. 执行策略选择器菜单 ──────────────────────────────────────
  it('33-sql-editor-strategy: captures execution strategy dropdown menu', async () => {
    const strategyBtn = await $('[data-testid="editor-execution-strategy-button"]');
    if (await strategyBtn.isDisplayed()) {
      await strategyBtn.click();
      await browser.pause(500);
      // Wait for context menu popup
      await browser
        .waitUntil(
          async () =>
            browser.execute(
              () => document.querySelector('[data-testid="web-context-menu"]') != null,
            ),
          { timeout: 5000, timeoutMsg: 'context menu did not open' },
        )
        .catch(() => {});
      await browser.pause(400);
      await shot('33-sql-editor-strategy.png');

      // Dismiss menu
      const editor = await $('.cm-editor .cm-content');
      await editor.click();
      await browser.pause(400);
    }
  });

  // ── 3. 代码片段 (Snippets) 菜单 ───────────────────────────────
  it('32-sql-editor-snippets: captures snippets dropdown menu', async () => {
    const snippetsBtn = await $('[data-testid="editor-snippets-button"]');
    if (await snippetsBtn.isDisplayed()) {
      await snippetsBtn.click();
      await browser.pause(500);
      // Wait for context menu popup
      await browser
        .waitUntil(
          async () =>
            browser.execute(
              () => document.querySelector('[data-testid="web-context-menu"]') != null,
            ),
          { timeout: 5000, timeoutMsg: 'snippets menu did not open' },
        )
        .catch(() => {});
      await browser.pause(400);
      await shot('32-sql-editor-snippets.png');

      // Dismiss menu
      const editor = await $('.cm-editor .cm-content');
      await editor.click();
      await browser.pause(400);
    }
  });

  // ── 4. 安全参数绑定面板 ───────────────────────────────────────
  it('31-sql-editor-bind-params: captures parameter input panel', async () => {
    const paramSql = `-- 参数化安全查询：输入参数防 SQL 注入与隐式传 NULL
SELECT sale_date, category, region, amount, quantity
FROM demo_sales
WHERE category = :category AND amount >= :min_amount
ORDER BY amount DESC
LIMIT 10;`;

    await setEditorContent(paramSql);
    await browser.pause(800);

    // Look for parameter panel by testid or class
    await browser.waitUntil(
      async () =>
        browser.execute(() => {
          const p =
            document.querySelector('[data-testid="bind-param-panel"]') ||
            document.querySelector('.border-b.border-edge.bg-surface');
          return !!p;
        }),
      { timeout: 10000, timeoutMsg: 'bind-param-panel not mounted' },
    );

    // Input parameter values
    await browser.execute(() => {
      const inputs = Array.from(document.querySelectorAll<HTMLInputElement>('input'));
      const textInputs = inputs.filter((i) => i.type === 'text' || !i.type);
      for (const input of textInputs) {
        if ((input.placeholder || '').includes('category') || input.name === 'category') {
          input.value = '电子产品';
          input.dispatchEvent(new Event('input', { bubbles: true }));
          input.dispatchEvent(new Event('change', { bubbles: true }));
        } else if ((input.placeholder || '').includes('amount') || input.name === 'min_amount') {
          input.value = '500';
          input.dispatchEvent(new Event('input', { bubbles: true }));
          input.dispatchEvent(new Event('change', { bubbles: true }));
        }
      }
    });
    await browser.pause(600);
    await shot('31-sql-editor-bind-params.png');

    // Execute query with bound params
    await clickExecute();
    await waitResults(1, 8000).catch(() => {});
    await browser.pause(800);
  });

  // ── 5. 高危操作安全拦截 ───────────────────────────────────────
  it('30-sql-editor-danger-guard: captures dangerous SQL confirmation warning', async () => {
    const dangerSql = `-- 危险操作：全表更新无 WHERE 条件（自动触发安全门网拦截）
UPDATE demo_sales SET amount = amount * 1.05;`;

    await setEditorContent(dangerSql);
    await clickExecute();

    // Wait for safety dialog to appear (ConfirmDialog or ResultMessageDialog)
    const dialog = await $('[role="dialog"]');
    await dialog.waitForDisplayed({ timeout: 10000 });
    await browser.pause(800);
    await shot('30-sql-editor-danger-guard.png');

    // Click dismiss/cancel button on dialog
    await browser.execute(() => {
      const btns = Array.from(document.querySelectorAll('button'));
      const cancelBtn = btns.find(
        (b) =>
          (b.textContent || '').trim() === '取消' ||
          (b.textContent || '').trim() === '关闭' ||
          (b.textContent || '').trim() === '确定',
      );
      cancelBtn?.click();
    });
    await browser.pause(500);
  });

  // ── 6. 语法错误与 AI 错误诊断 ─────────────────────────────────
  it('05-ai-diagnosis: captures error feedback with AI diagnosis entry', async () => {
    const errorSql = `-- 语法错误示例：触发智能错误诊断与一键修复
SELEC sale_date, category, amount FROM demo_sales;`;

    await setEditorContent(errorSql);
    await clickExecute();

    // Wait for error message
    const errorBox = await $('[data-testid="query-error-message"]');
    await errorBox.waitForDisplayed({ timeout: 15000 });
    await browser.pause(800);
    await shot('05-ai-diagnosis.png');
  });
});
