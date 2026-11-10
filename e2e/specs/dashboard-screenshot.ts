/**
 * Dashboard Screenshot — Add to Dashboard with chart + data views.
 *
 * 1. Run SQL → result chart
 * 2. Add to Dashboard
 * 3. Screenshot: dashboard with chart widget
 * 4. Switch widget to data view
 * 5. Screenshot: dashboard with data table widget
 *
 * Outputs to e2e/screenshots/site/
 */
import { browser, $ } from '@wdio/globals';
import { assertGallerySize, ensureMaximized } from '../lib/capture-window';
import fs from 'node:fs';
import path from 'node:path';
import url from 'node:url';

const ROOT = path.resolve(path.dirname(url.fileURLToPath(import.meta.url)), '..', '..');
const OUT = path.join(ROOT, 'e2e', 'screenshots', 'site');

async function invoke<T = unknown>(cmd: string, args: Record<string, unknown> = {}): Promise<T> {
  return browser.executeAsync(
    (c: string, a: string, done: (r: unknown) => void) => {
      (window as any).__TAURI_INTERNALS__
        .invoke(c, JSON.parse(a))
        .then((r: unknown) => done(r))
        .catch((e: unknown) => done({ __error: String(e) }));
    },
    cmd,
    JSON.stringify(args),
  ) as Promise<T>;
}

async function shot(name: string, settleMs = 1200) {
  await browser.pause(settleMs);
  // Maximize, never resize: a `set_size` IPC is honoured 1:1 in CSS points and
  // drops devicePixelRatio, which is how pro-11/pro-12 ended up at 2560x1648.
  await ensureMaximized();
  await assertGallerySize(name);
  fs.mkdirSync(OUT, { recursive: true });
  const buf = Buffer.from(await browser.takeScreenshot(), 'base64');
  fs.writeFileSync(path.join(OUT, name), buf);
  console.log(`📸 ${name} (${buf.length} bytes)`);
}

async function clickTestId(id: string) {
  const el = await $(`[data-testid="${id}"]`);
  await el.waitForDisplayed({ timeout: 10000 });
  await el.click();
  await browser.pause(300);
}

async function setEditorContent(sql: string) {
  await browser.execute((s: string) => {
    const el = document.querySelector('[data-testid="sql-editor"]');
    const view = (el as any)?.__cmView ?? (el as any)?.cmView?.view;
    if (!view) return;
    view.dispatch({
      changes: { from: 0, to: view.state.doc.length, insert: s },
    });
  }, sql);
  await browser.pause(300);
}

async function waitForResults(timeoutMs = 30000) {
  const queryPanel = await $('[data-testid="query-panel"]');
  const prevSeq = Number((await queryPanel.getAttribute('data-execution-seq')) ?? '0');
  await browser.waitUntil(
    async () => {
      const seq = Number((await queryPanel.getAttribute('data-execution-seq')) ?? '0');
      if (seq > prevSeq) return true;
      const body = await $('body').getText();
      if (/\d+\s*(行|rows?|records?)\b/i.test(body)) return true;
      if (/Query failed|error returned from database/i.test(body)) return true;
      return false;
    },
    { timeout: timeoutMs, timeoutMsg: `Results did not appear within ${timeoutMs}ms` },
  );
  await browser.pause(600);
}

describe('Dashboard Screenshot', () => {
  it('captures dashboard with chart and data views', async function () {
    this.timeout(300000);
    await ensureMaximized();
    await assertGallerySize('dashboard start');

    // ── Setup: wizard → main → query tab ──
    await browser.url('tauri://localhost/window.html?window=onboarding');
    await $('[data-testid="onboarding-wizard"]').waitForDisplayed({ timeout: 30000 });

    await $('[data-testid="onboarding-entry-sample"]').waitForDisplayed({ timeout: 15000 });
    await browser.pause(1500);
    await $('[data-testid="onboarding-entry-sample"]').click();
    await $('[data-testid="onboarding-step-s1-sample"]').waitForDisplayed({ timeout: 15000 });
    await browser.pause(2000);
    await browser.waitUntil(
      async () =>
        (await $('[data-testid="onboarding-sample-path"]').isExisting()) ||
        (await $('[data-testid="onboarding-sample-error"]').isExisting()),
      { timeout: 30000, timeoutMsg: 'Sample data seeding timeout' },
    );
    await browser.pause(1000);
    const continueBtn = $('[data-testid="onboarding-continue"]');
    await browser.waitUntil(async () => await continueBtn.isEnabled(), { timeout: 10000 });
    await continueBtn.click();
    await $('[data-testid="onboarding-step-s2-ai"]').waitForDisplayed({ timeout: 15000 });
    await browser.pause(1500);
    await $('[data-testid="onboarding-skip"]').click();
    await $('[data-testid="onboarding-step-s3"]').waitForDisplayed({ timeout: 15000 });
    await browser.pause(1000);

    // Switch to main window
    const wizardHandle = await browser.getWindowHandle();
    await $('[data-testid="onboard-open-datazen"]').click();
    await browser.waitUntil(
      async () => {
        const handles = await browser.getWindowHandles().catch(() => [] as string[]);
        const next = handles.find((h) => h !== wizardHandle);
        if (!next) return false;
        await browser.switchToWindow(next).catch(() => {});
        return browser
          .execute(() => !!document.querySelector('[data-testid="workspace-nav-databases"]'))
          .catch(() => false);
      },
      { timeout: 30000, timeoutMsg: 'Main window did not appear' },
    );
    await $('[data-testid="workspace-nav-databases"]').waitForDisplayed({ timeout: 15000 });
    await browser.pause(1500);

    // Ensure connected
    await browser.execute(() => {
      const nav =
        document.querySelector('[data-testid="connection-navigator-aside"]') ??
        Array.from(document.querySelectorAll('aside')).find((a) =>
          a.querySelector('[data-conn-item]'),
        );
      const items = nav?.querySelectorAll('[data-conn-item]') ?? [];
      for (const item of items) {
        if (item.textContent?.includes('Sample Playground')) {
          (item as HTMLElement).dispatchEvent(
            new MouseEvent('dblclick', { bubbles: true, cancelable: true }),
          );
          return;
        }
      }
    });

    // Open query tab
    {
      const deadline = Date.now() + 60_000;
      let opened = false;
      while (Date.now() < deadline && !opened) {
        try {
          let btn = await $('[data-testid="conn-toolbar-new-query"]');
          if (!(await btn.isExisting()) || !(await btn.isDisplayed().catch(() => false))) {
            btn = await $('[data-testid="home-quick-new-query"]');
          }
          if (!(await btn.isExisting()) || !(await btn.isDisplayed().catch(() => false))) {
            await browser.pause(300);
            continue;
          }
          await btn.waitForClickable({ timeout: Math.max(1000, deadline - Date.now()) });
          await btn.click();
          await browser.pause(250);
          opened = await $('[data-testid="editor-execute-button"]')
            .isExisting()
            .catch(() => false);
        } catch {
          await browser.pause(300);
        }
      }
    }
    await $('[data-testid="editor-execute-button"]').waitForDisplayed({ timeout: 20000 });
    await browser.pause(1500);

    // ── Run chartable SQL ──
    await $('[data-testid="query-panel"]').waitForDisplayed({ timeout: 15000 });
    await setEditorContent(
      'SELECT region, SUM(amount) AS total FROM demo_sales GROUP BY region ORDER BY total DESC;',
    );
    await clickTestId('editor-execute-button');
    await waitForResults();
    await browser.pause(1500);

    // ── Switch to chart view ──
    await clickTestId('result-workspace-view-chart');
    await browser.pause(1500);

    // ── Click "Add to Dashboard" button ──
    await clickTestId('query-add-to-dashboard');
    await browser.pause(1000);

    // Wait for dialog
    await $('[data-testid="add-to-dashboard-dialog"]').waitForDisplayed({ timeout: 10000 });
    await browser.pause(500);

    // Fill widget name
    await browser.execute(() => {
      const inputs = document.querySelectorAll('[data-testid="add-to-dashboard-dialog"] input');
      for (const input of inputs) {
        if ((input as HTMLInputElement).type === 'text' || !(input as HTMLInputElement).type) {
          const nativeSetter = Object.getOwnPropertyDescriptor(
            window.HTMLInputElement.prototype,
            'value',
          )?.set;
          nativeSetter?.call(input, '销售总览');
          input.dispatchEvent(new Event('input', { bubbles: true }));
          input.dispatchEvent(new Event('change', { bubbles: true }));
          break;
        }
      }
    });
    await browser.pause(500);

    // Confirm
    await clickTestId('add-to-dashboard-confirm');
    await browser.pause(2000);

    // ── Wait for dashboard panel to load ──
    await $('[data-testid="dashboard-panel"]').waitForDisplayed({ timeout: 15000 });
    await browser.pause(2000);

    // Wait for dashboard tile to appear
    await browser.waitUntil(async () => (await $$('[data-testid="dashboard-tile"]')).length >= 1, {
      timeout: 30000,
      timeoutMsg: 'Dashboard tile did not appear',
    });
    await browser.pause(1500);

    // ── Refresh the widget to load query data ──
    await clickTestId('dashboard-tile-refresh');
    await browser.pause(4000); // Wait for query execution + chart render

    // Wait for chart SVG to appear (chart has data)
    await browser
      .waitUntil(
        async () => {
          return browser.execute(() => {
            const chart = document.querySelector('[data-testid="dashboard-tile-chart"]');
            if (!chart) return false;
            // Check for SVG chart elements or canvas
            return !!(
              chart.querySelector('svg') ||
              chart.querySelector('canvas') ||
              chart.querySelector('[class*="chart"]')
            );
          });
        },
        { timeout: 20000, timeoutMsg: 'Chart did not render' },
      )
      .catch(() => {});
    await browser.pause(2000);

    // ── Screenshot 1: Dashboard with chart widget ──
    await shot('pro-11-dashboard-chart.png', 2000);

    // ── Switch tile to data/table view ──
    await clickTestId('widget-view-table');
    await browser.pause(3000);

    // ── Screenshot 2: Dashboard with data table widget ──
    await shot('pro-12-dashboard-data.png', 2000);

    console.log('✅ Dashboard screenshots captured');
  });
});
