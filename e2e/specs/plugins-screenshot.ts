/**
 * 22-plugins.png — the Plugins (Extensions) management page.
 *
 * The image ships on the site but nothing regenerates it, so it had drifted:
 * it is a 2560x1640 WebP saved under a `.png` name, captured by hand on
 * 2026-09-01. This spec makes it reproducible from the real UI.
 *
 * The page itself is `WappManagementPage` (aliased `ExtensionManagementPage`),
 * reached through the workspace rail. `22-wapps.png` covers the same view in
 * `zz-screenshots.ts`; this spec is the standalone, always-safe producer.
 *
 * Window geometry: a capture run leaves the main window maximized, so the frame
 * lands at the display's natural 2x size. This spec never resizes anything —
 * no `set_size`, no `setWindowSize`, and no `documentElement.style.width`
 * override (the pattern `wizard-screenshot.ts` / `dashboard-screenshot.ts` still
 * carry, which is what produced the mismatched sizes in this set).
 *
 * Excluded from a plain `pnpm e2e` by the `./specs/*screenshot*.ts` glob in
 * `e2e/wdio.conf.ts`. Run it with:
 *   node e2e/run.mjs --skip-build --attach --capture -- \
 *     --spec e2e/specs/plugins-screenshot.ts
 */
import { browser, $ } from '@wdio/globals';
import { assertGallerySize, ensureMaximized } from '../lib/capture-window';
import fs from 'node:fs';
import path from 'node:path';
import url from 'node:url';

const ROOT = path.resolve(path.dirname(url.fileURLToPath(import.meta.url)), '..', '..');
const OUT = path.join(ROOT, 'e2e', 'screenshots', 'site');

const TARGET = '22-plugins.png';

let mainWindow = '';

function describeError(err: unknown): string {
  return err instanceof Error ? err.message : String(err);
}

/** Capture the current frame verbatim — no resize, no crop, no re-encode. */
async function shot(name: string, settleMs = 1200): Promise<void> {
  await browser.pause(settleMs);
  // Maximize, never resize: a `set_size` IPC is honoured 1:1 in CSS points and
  // drops devicePixelRatio, which is how this image ended up at 1920x1440.
  await ensureMaximized();
  await assertGallerySize(name);
  fs.mkdirSync(OUT, { recursive: true });
  await browser.saveScreenshot(path.join(OUT, name));
  const { size } = fs.statSync(path.join(OUT, name));
  console.log(`📸 ${name} (${size} bytes)`);
}

/**
 * `shot` that reports a skip instead of failing the run: a gallery image is
 * tooling, not an assertion, and one missing frame must not abort the rest of
 * the capture pass. Same contract as `softShot` in `zz-screenshots.ts`.
 */
async function softShot(name: string, settleMs = 1200): Promise<void> {
  try {
    await shot(name, settleMs);
  } catch (err) {
    console.warn(`[soft-shot] ${name} skipped: ${describeError(err)}`);
  }
}

/**
 * Click the Extensions entry in the workspace rail.
 *
 * The rail lives inside `ConnectionPage`, so another workspace mode (workflow /
 * dashboard / settings) has no rail to click; `workspace-nav-databases` first
 * puts the page back into a mode that renders one.
 */
async function openPluginsPage(): Promise<void> {
  await $('[data-testid="workspace-nav-databases"]')
    .waitForClickable({ timeout: 30000, timeoutMsg: '工作区导航栏未出现' })
    .catch(() => {
      // Already on the connections workspace (or a mode that still renders the
      // rail) — the Extensions entry below is the real gate.
    });

  const nav = await $('[data-testid="workspace-nav-extensions"]');
  await nav.waitForClickable({ timeout: 20000, timeoutMsg: '插件入口未出现' });
  await nav.click();

  await $('[data-testid="extension-management-page"]').waitForDisplayed({
    timeout: 20000,
    timeoutMsg: '插件管理页未打开',
  });
}

describe('Plugins Screenshot', () => {
  before(async function () {
    this.timeout(120000);
    fs.mkdirSync(OUT, { recursive: true });
    mainWindow = await browser.getWindowHandle();
  });

  after(async function () {
    this.timeout(60000);
    // Nothing to restore: this spec only navigates, it seeds no data and
    // changes no settings. Hand focus back to the main window for the spec
    // that runs next.
    await browser.switchToWindow(mainWindow).catch(() => {});
  });

  it('22-plugins: 插件（扩展）管理页', async function () {
    this.timeout(180000);
    try {
      await browser.switchToWindow(mainWindow);
      await browser.pause(800);
      await openPluginsPage();

      // The body is either the installed-card grid or the empty state; both
      // are correct renders, so settle on whichever arrives rather than
      // requiring a populated list the capture environment may not have.
      await browser
        .waitUntil(
          async () =>
            browser.execute(
              () =>
                !!document.querySelector('[data-testid="extension-card"]') ||
                !!document.querySelector('[data-testid="extension-page-empty"]'),
            ),
          { timeout: 15000, timeoutMsg: '插件列表既未渲染卡片也未渲染空状态' },
        )
        .catch((err: unknown) => {
          console.warn(`[plugins] 列表未就绪，仍按当前状态截图: ${describeError(err)}`);
        });

      // The search box and the three filter chips are the part of this page
      // worth showing, so make sure the whole toolbar is on screen before the
      // frame is taken.
      await $('[data-testid="extension-search-input"]').waitForDisplayed({
        timeout: 10000,
        timeoutMsg: '搜索框未出现',
      });
      await $('[data-testid="extension-filter-all"]').waitForDisplayed({
        timeout: 10000,
        timeoutMsg: '筛选标签未出现',
      });

      await softShot(TARGET, 1500);
    } catch (err) {
      console.warn(`[soft-shot] ${TARGET} skipped: ${describeError(err)}`);
    }
  });
});
