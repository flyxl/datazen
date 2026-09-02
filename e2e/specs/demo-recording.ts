/**
 * Demo recording spec — drives the app through a product demo flow while
 * capturing frames via the WebDriver screenshot endpoint (in-webview render;
 * no macOS Screen Recording permission, no ffmpeg).
 *
 * Frames land in e2e/.demo-recording/frame_NNNNN.png and are assembled into an
 * animated PNG by e2e/assemble-apng.mjs (see e2e/record-demo.sh).
 *
 * Locators are stable post-refactor handles:
 *   - data-testid attributes (vite-gated via src/lib/tid.ts)
 *   - data-conn-item / data-conn-name on navigator tree rows
 *
 * Usage (via wrapper):
 *   bash e2e/record-demo.sh [--skip-build]
 */
import { browser, $ } from '@wdio/globals';
import fs from 'node:fs';
import path from 'node:path';
import url from 'node:url';

const ROOT = path.resolve(path.dirname(url.fileURLToPath(import.meta.url)), '..', '..');
const FRAME_DIR = path.join(ROOT, 'e2e', '.demo-recording');

const DEMO_PG_DB = process.env.E2E_DEMO_PG_DB || 'datazen_demo';
const DEMO_PG_CONN_ID = 'conn_demo_pg';
const DEMO_PG_CONN_NAME = '演示 PostgreSQL';

// ── WebDriver frame capture ──
let seq = 0;
function pad(n: number): string {
  return String(n).padStart(5, '0');
}

async function snap(): Promise<void> {
  const b64 = (await browser.takeScreenshot()) as string;
  seq += 1;
  fs.writeFileSync(path.join(FRAME_DIR, `frame_${pad(seq)}.png`), Buffer.from(b64, 'base64'));
}

/** Hold for `ms`, snapping frames throughout so playback stays fluid. */
async function hold(ms: number, interval = 250): Promise<void> {
  const end = Date.now() + ms;
  // eslint-disable-next-line no-await-in-loop
  while (Date.now() < end) {
    // eslint-disable-next-line no-await-in-loop
    await snap();
    // eslint-disable-next-line no-await-in-loop
    await browser.pause(interval);
  }
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

async function setWindowSize(w = 1600, h = 1000) {
  await invoke('plugin:window|set_size', { size: { width: w, height: h } });
  await browser.pause(500);
}

async function setEditorContent(text: string) {
  const editor = $('.cm-editor .cm-content');
  await editor.waitForDisplayed({ timeout: 10000 });
  // Drive the same pointer/focus transition as a user. A JS-only focus does
  // not close the portaled database selector used by the query toolbar.
  await editor.click();
  await browser.waitUntil(
    async () =>
      browser.execute(
        () =>
          document.activeElement?.closest('.cm-editor .cm-content') != null &&
          document.querySelector('[id^="dz-select-listbox-"]') == null,
      ),
    { timeout: 2000, timeoutMsg: 'editor focus/selector cleanup did not settle' },
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
  await browser.pause(300);
}

/** Click a [data-testid] element once it exists. */
async function clickTestId(testId: string, timeout = 10000): Promise<void> {
  const el = $(`[data-testid="${testId}"]`);
  await el.waitForDisplayed({ timeout });
  await el.click();
  await browser.pause(600);
}

async function waitForResults(timeout = 20000) {
  await browser.waitUntil(
    async () =>
      browser.execute(
        () =>
          document.querySelectorAll('[data-dt-row]').length > 0 ||
          document.querySelectorAll('table tbody tr').length > 0,
      ),
    { timeout, timeoutMsg: 'query results did not appear' },
  );
}

/**
 * Delete all existing connections so the demo starts clean.
 * Backs them up first and returns them; call restoreConnections() in after()
 * (defense for binaries without DATAZEN_DATA_DIR isolation, where the app
 * would otherwise touch real user data).
 */
let backedUpConnections: Array<Record<string, unknown>> = [];
async function clearConnections(): Promise<void> {
  backedUpConnections = await invoke<Array<Record<string, unknown>>>('get_connections');
  for (const c of backedUpConnections) {
    await invoke('delete_connection', { id: (c as { id: string }).id });
  }
}

async function restoreConnections(): Promise<void> {
  for (const config of backedUpConnections) {
    await invoke('save_connection', { config }).catch((e: unknown) =>
      console.error('[restore] failed:', config, e),
    );
  }
  if (backedUpConnections.length > 0) {
    console.log(`[restore] ${backedUpConnections.length} connection(s) restored`);
  }
  backedUpConnections = [];
}

/**
 * Create the demo PG connection and connect to it.
 * Post-refactor (decision 1): `use_database` no longer exists — the session is
 * pinned through config.database and every query carries its target database.
 */
async function setupDemoConnection() {
  const config = {
    id: DEMO_PG_CONN_ID,
    name: DEMO_PG_CONN_NAME,
    databaseType: 'postgresql',
    host: process.env.E2E_PG_HOST || '127.0.0.1',
    port: Number(process.env.E2E_PG_PORT || 5432),
    username: process.env.E2E_DEMO_PG_USER || 'datazen_demo',
    password: process.env.E2E_DEMO_PG_PASSWORD || 'datazen_demo',
    database: DEMO_PG_DB,
    group: 'preset:development',
    colorTag: '#3b82f6',
    sslMode: 'disable',
  };
  const r = await invoke('save_connection', { config });
  if (r && typeof r === 'object' && '__error' in (r as object)) {
    throw new Error(`save_connection failed: ${JSON.stringify(r)}`);
  }

  const dbSessionId = await invoke<string>('connect', { connectionId: DEMO_PG_CONN_ID });
  if (typeof dbSessionId !== 'string' || dbSessionId.startsWith('__error')) {
    throw new Error(`connect failed: ${JSON.stringify(dbSessionId)}`);
  }
}

describe('demo recording', () => {
  before(async () => {
    fs.rmSync(FRAME_DIR, { recursive: true, force: true });
    fs.mkdirSync(FRAME_DIR, { recursive: true });
    await setWindowSize(1600, 1000);
    await clearConnections();
    await setupDemoConnection();
    // Reload so the sidebar lists the new connection
    await browser.url('tauri://localhost');
    await browser.waitUntil(
      async () => browser.execute(() => document.querySelectorAll('[data-conn-item]').length > 0),
      { timeout: 15000, timeoutMsg: 'connections not visible after reload' },
    );
  });

  after(async () => {
    await restoreConnections();
  });

  it('full demo flow', async () => {
    // ── 1. Welcome screen — show the connection list ──
    await hold(2500);

    // ── 2. Open the demo PostgreSQL connection ──
    await browser.execute((connName: string) => {
      const items = Array.from(document.querySelectorAll('[data-conn-item]'));
      const pg = items.find((el) => (el.getAttribute('data-conn-name') || '').includes(connName));
      if (pg) {
        pg.dispatchEvent(new MouseEvent('dblclick', { bubbles: true, cancelable: true }));
      }
    }, DEMO_PG_CONN_NAME);
    await hold(3000);

    // ── 3. Wait for schema tree ──
    await browser.waitUntil(
      async () =>
        browser.execute(() => {
          const text = document.body.textContent || '';
          return text.includes('Tables') || text.includes('表');
        }),
      { timeout: 20000, timeoutMsg: 'schema tree did not load' },
    );
    await hold(2000);

    // ── 4. Open new query tab ──
    // Post-refactor the workspace may land on ConnectionWorkspaceHome first;
    // prefer its quick-action card, fall back to the content toolbar button.
    const homeQuick = $('[data-testid="home-quick-new-query"]');
    if (await homeQuick.isExisting()) {
      await homeQuick.click();
    } else {
      await clickTestId('conn-toolbar-new-query');
    }
    await hold(1500);

    // ── 5. Run a query ──
    const sql = `SELECT sale_date, category, region, amount, quantity
FROM demo_sales
ORDER BY sale_date DESC, amount DESC
LIMIT 20;`;
    await setEditorContent(sql);
    await hold(1000);

    await snap();
    await clickTestId('editor-execute-button');
    await waitForResults();
    await hold(3000);

    // ── 6. Chart view ──
    await clickTestId('result-workspace-view-chart');
    await hold(3000);

    // ── 7. Aggregate query → bar chart ──
    await clickTestId('result-workspace-view-table');
    await hold(500);

    const aggSql = `SELECT category, SUM(amount) AS total_amount
FROM demo_sales
GROUP BY category
ORDER BY total_amount DESC;`;
    await setEditorContent(aggSql);
    await hold(500);
    await snap();
    await clickTestId('editor-execute-button');
    await waitForResults();
    await hold(1500);

    await clickTestId('result-workspace-view-chart');
    await hold(1000);
    await clickTestId('chart-type-bar');
    await hold(3000);

    // ── 8. ER diagram ──
    const erBtn = $('[data-testid="content-toolbar-er-diagram"] button');
    if (await erBtn.isExisting()) {
      await erBtn.click();
    } else {
      // Fall back to the workspace-home quick action when no panel is open
      await clickTestId('home-quick-er-diagram');
    }
    await browser.waitUntil(
      async () => browser.execute(() => document.querySelectorAll('.react-flow__node').length >= 3),
      { timeout: 20000, timeoutMsg: 'ER nodes did not render' },
    );
    await hold(4000);

    // ── Done ──
    await hold(1500);
  });
});
