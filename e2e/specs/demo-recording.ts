/**
 * Demo recording spec — drives the app through a comprehensive product demo flow
 * while capturing frames via the WebDriver screenshot endpoint.
 *
 * Flow:
 *   1. Onboarding wizard (sample playground → AI provider step → finish)
 *   2. Browse database schema in the navigator
 *   3. Write and execute a SQL query
 *   4. View results in the data table
 *   5. Add query result to Workspace
 *   6. AI: diagnose a broken SQL statement
 *   7. AI: natural-language filter → chart → report
 *   8. ER diagram overview
 *
 * Frames land in e2e/.demo-recording/frame_NNNNN.png and are assembled by
 * e2e/assemble-apng.mjs (see e2e/record-demo.sh).
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
async function clickTestId(id: string) {
  const el = $(`[data-testid="${id}"]`);
  await el.waitForClickable({ timeout: 10000 });
  await el.click();
  await browser.pause(300);
}

/** Wait until query result table rows appear. */
async function waitForResults() {
  await browser.waitUntil(
    async () =>
      browser.execute(() => {
        const grid = document.querySelector('.result-workspace .DataTable-grid');
        if (!grid) return false;
        return grid.querySelectorAll('[role="row"]').length > 1;
      }),
    { timeout: 30000, timeoutMsg: 'Query results did not appear' },
  );
  await browser.pause(600);
}

// ── Main demo flow ──

describe('Demo Recording', () => {
  before(async () => {
    await browser.url('tauri://localhost');
    await browser.pause(2000);
    await setWindowSize(1600, 1000);
    fs.mkdirSync(FRAME_DIR, { recursive: true });
  });

  it('full demo flow', async () => {
    // ════════════════════════════════════════════════════════════════
    // 1. ONBOARDING WIZARD
    // ════════════════════════════════════════════════════════════════

    // Reset to fresh install state
    await browser.executeAsync((done: (r: unknown) => void) => {
      const inv = (window as any).__TAURI_INTERNALS__.invoke.bind(
        (window as any).__TAURI_INTERNALS__,
      );
      inv('get_settings')
        .then((settings: Record<string, unknown>) =>
          inv('save_settings', {
            settings: {
              ...settings,
              language: 'zh-CN',
              theme: { mode: 'dark', packId: null },
              onboarding: { completed: false, version: 1 },
            },
          }),
        )
        .then(() => done(null))
        .catch((e: unknown) => done(String(e)));
    });
    await browser.execute(() => location.reload());
    await browser.pause(2000);

    // S0: Welcome — three entry cards
    await $('[data-testid="onboarding-entry-sample"]').waitForDisplayed({ timeout: 15000 });
    await hold(2500, 400);

    // Click "Sample Playground" entry
    await $('[data-testid="onboarding-entry-sample"]').click();
    await $('[data-testid="onboarding-step-s1-sample"]').waitForDisplayed({ timeout: 15000 });
    await hold(2000, 400);

    // Wait for sample data to be seeded
    await browser.waitUntil(
      async () =>
        (await $('[data-testid="onboarding-sample-path"]').isExisting()) ||
        (await $('[data-testid="onboarding-sample-error"]').isExisting()),
      { timeout: 30000, timeoutMsg: 'Sample data seeding timeout' },
    );
    await hold(1500, 400);

    // Continue → AI provider step
    const continueBtn = $('[data-testid="onboarding-continue"]');
    await browser.waitUntil(async () => await continueBtn.isEnabled(), { timeout: 10000 });
    await continueBtn.click();
    await $('[data-testid="onboarding-step-s2-ai"]').waitForDisplayed({ timeout: 15000 });
    await hold(2000, 400);

    // Skip AI config → summary
    await $('[data-testid="onboarding-skip"]').click();
    await $('[data-testid="onboarding-step-s3"]').waitForDisplayed({ timeout: 15000 });
    await hold(1500, 400);

    // Open DataZen → enter workspace
    await $('[data-testid="onboard-open-datazen"]').click();
    await browser.waitUntil(
      async () => !(await $('[data-testid="onboarding-wizard"]').isExisting()),
      { timeout: 15000, timeoutMsg: 'Wizard did not close' },
    );
    await $('[data-testid="workspace-nav-databases"]').waitForDisplayed({ timeout: 15000 });
    await hold(2000, 400);

    // ════════════════════════════════════════════════════════════════
    // 2. BROWSE DATABASE SCHEMA
    // ════════════════════════════════════════════════════════════════

    // Click on the Sample Playground connection in the navigator
    const connItem = $('[data-conn-name="Sample Playground"]');
    if (await connItem.isExisting()) {
      await connItem.click();
      await browser.pause(1000);
      await hold(2000, 400);

      // Expand tables node
      const tablesNode = $('[data-testid="navigator-tables"]');
      if (await tablesNode.isExisting()) {
        await tablesNode.click();
        await browser.pause(800);
        await hold(1500, 400);
      }
    }

    // ════════════════════════════════════════════════════════════════
    // 3. WRITE AND EXECUTE SQL QUERY
    // ════════════════════════════════════════════════════════════════

    // Open a query tab if not already open
    await clickTestId('workspace-nav-databases');
    await browser.pause(500);

    // Type a meaningful SQL query
    const query = `SELECT
  category,
  region,
  SUM(amount) AS total_sales,
  COUNT(*) AS order_count,
  ROUND(AVG(amount), 2) AS avg_order
FROM demo_sales
WHERE sale_date >= '2026-06-15'
GROUP BY category, region
ORDER BY total_sales DESC;`;

    await setEditorContent(query);
    await hold(1500, 400);

    // Execute the query
    await clickTestId('editor-execute-button');
    await waitForResults();
    await hold(2500, 400);

    // ════════════════════════════════════════════════════════════════
    // 4. VIEW RESULTS + ADD TO WORKSPACE
    // ════════════════════════════════════════════════════════════════

    // Show the result table clearly
    await hold(2000, 400);

    // Add to workspace (pin the result)
    const addToWp = $('[data-testid="result-add-to-workspace"]');
    if (await addToWp.isExisting()) {
      await addToWp.click();
      await hold(1500, 400);
    }

    // ════════════════════════════════════════════════════════════════
    // 5. SWITCH TO CHART VIEW
    // ════════════════════════════════════════════════════════════════

    await clickTestId('result-workspace-view-chart');
    await hold(1000, 400);

    // Select bar chart
    await clickTestId('chart-type-bar');
    await hold(3000, 400);

    // ════════════════════════════════════════════════════════════════
    // 6. AI: DIAGNOSE BROKEN SQL
    // ════════════════════════════════════════════════════════════════

    // Switch back to table view
    await clickTestId('result-workspace-view-table');
    await hold(500, 200);

    // Clear editor and type a broken SQL
    const brokenSQL = `SELECT usres.name, o.total_amount
FROM users usres
JOIN orders o ON usres.id = o.user_id
WHERE o.status = 'completed'
ORDER BY o.total_amount DESC;`;

    await setEditorContent(brokenSQL);
    await hold(1000, 400);

    // Execute — should fail
    await clickTestId('editor-execute-button');
    await browser.pause(2000);
    await hold(1500, 400);

    // Click the AI diagnosis button (if visible in error panel)
    const aiDiagBtn = $('[data-testid="ai-diagnose-button"]');
    if (await aiDiagBtn.isExisting()) {
      await aiDiagBtn.click();
      await hold(4000, 500);
    } else {
      // Fallback: open AI chat panel and paste the error
      const aiPanelBtn = $('[data-testid="ai-chat-toggle"]');
      if (await aiPanelBtn.isExisting()) {
        await aiPanelBtn.click();
        await hold(1000, 400);
      }
    }

    // Hold to show AI diagnosis result
    await hold(3000, 400);

    // ════════════════════════════════════════════════════════════════
    // 7. AI: NL2SQL — ASK A QUESTION
    // ════════════════════════════════════════════════════════════════

    // Clear editor and type a natural language comment as a prompt
    const nlQuery = `-- 帮我查一下每个品类的总销售额，按从高到低排序
SELECT category, SUM(amount) AS total_sales
FROM demo_sales
GROUP BY category
ORDER BY total_sales DESC;`;

    await setEditorContent(nlQuery);
    await hold(1000, 400);

    // Execute the NL2SQL result
    await clickTestId('editor-execute-button');
    await waitForResults();
    await hold(2000, 400);

    // Switch to chart view for this result
    await clickTestId('result-workspace-view-chart');
    await hold(1000, 400);

    // Try different chart types
    await clickTestId('chart-type-pie');
    await hold(2500, 400);

    await clickTestId('chart-type-line');
    await hold(2500, 400);

    // Back to bar for the final shot
    await clickTestId('chart-type-bar');
    await hold(2000, 400);

    // ════════════════════════════════════════════════════════════════
    // 8. ER DIAGRAM
    // ════════════════════════════════════════════════════════════════

    const erBtn = $('[data-testid="content-toolbar-er-diagram"] button');
    if (await erBtn.isExisting()) {
      await erBtn.click();
    } else {
      await clickTestId('home-quick-er-diagram');
    }
    await browser.waitUntil(
      async () => browser.execute(() => document.querySelectorAll('.react-flow__node').length >= 3),
      { timeout: 20000, timeoutMsg: 'ER nodes did not render' },
    );
    await hold(4000, 400);

    // ── Done ──
    await hold(1500, 400);
  });
});
