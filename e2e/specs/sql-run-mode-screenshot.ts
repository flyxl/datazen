import { readCatalog } from '../schema-metadata.js';
/**
 * 34-sql-run-mode.png — the SQL editor's run mode: which statement to run.
 *
 * When the editor holds a multi-statement script and the execution strategy is
 * set to "ask", the toolbar's ▶ button opens the **execution scope** dialog
 * (`ExecutionStrategyAskModal`): run the statement under the cursor, or run
 * the whole script. That decision is the run mode the gallery documents here.
 *
 * The image shipped on the site but nothing regenerated it, so it had drifted
 * into a 2590x1619 JPEG saved under a `.png` name, captured by hand on
 * 2026-09-17. This spec makes it reproducible.
 *
 * NOT the same thing as `33-sql-editor-strategy.png`: that one is the toolbar
 * dropdown (`ExecutionStrategySelect` + `web-context-menu`) produced by
 * `editor-features-screenshots.ts`, and this spec deliberately never captures
 * it — a fallback to the dropdown would silently write a different image under
 * this name.
 *
 * Window geometry: the main window is left exactly as the capture run found it
 * (maximized), so the frame is the display's natural 2x size. No `set_size`,
 * no `setWindowSize`, no `documentElement.style.width` override.
 *
 * Excluded from a plain `pnpm e2e` by the `./specs/*screenshot*.ts` glob in
 * `e2e/wdio.conf.ts`. Run it with:
 *   node e2e/run.mjs --skip-build --attach --capture -- \
 *     --spec e2e/specs/sql-run-mode-screenshot.ts
 */
import { browser, $ } from '@wdio/globals';
import { assertGallerySize, ensureMaximized } from '../lib/capture-window';
import fs from 'node:fs';
import path from 'node:path';
import url from 'node:url';

const ROOT = path.resolve(path.dirname(url.fileURLToPath(import.meta.url)), '..', '..');
const OUT = path.join(ROOT, 'site', 'assets', 'screenshots');

const TARGET = '34-sql-run-mode.png';

/** Demo PostgreSQL — see e2e/setup-demo-data.sh */
const DEMO_PG_DB = process.env.E2E_DEMO_PG_DB || 'datazen_demo';
const DEMO_PG_USER = process.env.E2E_DEMO_PG_USER || 'datazen_demo';
const DEMO_PG_PASSWORD = process.env.E2E_DEMO_PG_PASSWORD || 'datazen_demo';
const DEMO_PG_CONN_ID = 'conn_demo_pg';
const DEMO_PG_CONN_NAME = '演示 PostgreSQL';

/** Two statements, so "ask" actually has a choice to offer. */
const SCRIPT = [
  '-- 多语句脚本：光标落在哪一条，由「执行策略」决定',
  'SELECT region,',
  '       SUM(amount) AS total,',
  '       COUNT(*)    AS orders',
  'FROM demo_sales',
  'GROUP BY region',
  'ORDER BY total DESC;',
  '',
  'SELECT customer, MAX(sale_date) AS last_order',
  'FROM demo_sales',
  'WHERE amount > 1000',
  'GROUP BY customer;',
].join('\n');

/** Scratch attribute used to pin the scope dialog once it is found. */
const SCOPE_DIALOG_MARKER = 'data-e2e-scope-dialog';

let mainWindow = '';
/** Settings as they were found, so only `sqlExecutionStrategy` is put back. */
let originalSettings: Record<string, unknown> | null = null;

function describeError(err: unknown): string {
  return err instanceof Error ? err.message : String(err);
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

/** `shot` that reports a skip instead of failing the run. */
async function softShot(name: string, settleMs = 1200): Promise<void> {
  try {
    await shot(name, settleMs);
  } catch (err) {
    console.warn(`[soft-shot] ${name} skipped: ${describeError(err)}`);
  }
}

/**
 * The scope dialog has **no `data-testid`**: `ExecutionStrategyAskModal` renders
 * `Dialog` without the `testId` prop, and its two options are `role="button"`
 * divs rather than testid'd controls. It is therefore located structurally — a
 * `[role="dialog"]` rendering at least two SQL `<pre>` previews, which is what
 * the "current statement" and "entire script" options each show. That
 * discriminator is stable across UI languages, unlike matching the translated
 * title.
 *
 * Returns whether the dialog is present, and pins it with
 * `SCOPE_DIALOG_MARKER` when it is.
 */
async function locateScopeDialog(): Promise<boolean> {
  return browser.execute((marker: string) => {
    document.querySelector(`[${marker}]`)?.removeAttribute(marker);
    const dialog = Array.from(document.querySelectorAll<HTMLElement>('[role="dialog"]')).find(
      (el) => el.querySelectorAll('pre').length >= 2,
    );
    if (!dialog) return false;
    dialog.setAttribute(marker, 'true');
    return true;
  }, SCOPE_DIALOG_MARKER);
}

/** Labels of the dialog's two options, for the capture log and skip reasons. */
async function scopeDialogOptions(): Promise<string[]> {
  return browser.execute((marker: string) => {
    const dialog = document.querySelector<HTMLElement>(`[${marker}]`);
    return Array.from(dialog?.querySelectorAll('[role="button"]') ?? []).map((el) =>
      (el.textContent || '').replace(/\s+/g, ' ').trim().slice(0, 80),
    );
  }, SCOPE_DIALOG_MARKER);
}

/**
 * Dismiss the dialog. The modal's only `<button>` after the header's close X is
 * Cancel, so the last one in DOM order is it.
 */
async function closeScopeDialog(): Promise<void> {
  await browser.execute((marker: string) => {
    const dialog = document.querySelector<HTMLElement>(`[${marker}]`);
    const buttons = Array.from(dialog?.querySelectorAll<HTMLElement>('button') ?? []);
    buttons[buttons.length - 1]?.click();
    dialog?.removeAttribute(marker);
  }, SCOPE_DIALOG_MARKER);
  await browser.pause(400);
}

/**
 * Replace the editor buffer.
 *
 * `[data-testid="sql-editor-content"]` is the CodeMirror content DOM node
 * (`SqlEditor.tsx`), so the locator is a data attribute rather than a class
 * chain into the editor's internals.
 */
async function setEditorContent(text: string): Promise<void> {
  const editor = await $('[data-testid="sql-editor-content"]');
  await editor.waitForDisplayed({ timeout: 20000, timeoutMsg: 'SQL 编辑器未出现' });
  await editor.click();
  await browser.waitUntil(
    async () =>
      browser.execute(
        () =>
          document.activeElement?.closest('[data-testid="sql-editor-content"]') != null &&
          document.querySelector('[id^="dz-select-listbox-"]') == null,
      ),
    { timeout: 5000, timeoutMsg: 'editor focus did not settle' },
  );
  await browser.execute((t: string) => {
    const el = document.querySelector<HTMLElement>('[data-testid="sql-editor-content"]');
    if (!el) return;
    el.focus();
    const sel = window.getSelection();
    if (sel) {
      sel.selectAllChildren(el);
      sel.deleteFromDocument();
    }
    document.execCommand('insertText', false, t);
  }, text);
  await browser.pause(500);
}

/**
 * A non-empty selection outranks the strategy entirely — the gate runs the
 * selection and never reaches the scope dialog — so the caret is collapsed to
 * the end of the document first.
 */
async function collapseEditorSelection(): Promise<void> {
  await browser.execute(() => {
    const el = document.querySelector<HTMLElement>('[data-testid="sql-editor-content"]');
    if (!el) return;
    const sel = window.getSelection();
    if (!sel) return;
    const range = document.createRange();
    range.selectNodeContents(el);
    range.collapse(false);
    sel.removeAllRanges();
    sel.addRange(range);
  });
  await browser.pause(200);
}

async function clickExecute(): Promise<void> {
  const btn = await $('[data-testid="editor-execute-button"]');
  await btn.waitForClickable({ timeout: 15000, timeoutMsg: '执行按钮不可用' });
  await btn.click();
}

/** Click the connection item in the navigator so its tree is reachable. */
async function focusDemoConnection(): Promise<void> {
  // The connection is written to the store by `before`, not necessarily into
  // the tree that is already rendered, so refresh the navigator first. A
  // missing refresh button is not a failure.
  await $('[data-testid="navigator-refresh"]')
    .click()
    .catch(() => {});
  await browser.pause(800);

  await browser.waitUntil(
    async () =>
      browser.execute(
        (connName: string) =>
          Array.from(document.querySelectorAll('[data-conn-item]')).some((el) =>
            (el.getAttribute('data-conn-name') || '').includes(connName),
          ),
        DEMO_PG_CONN_NAME,
      ),
    { timeout: 20000, timeoutMsg: `${DEMO_PG_CONN_NAME} not in navigator` },
  );
  await browser.execute((connName: string) => {
    const item = Array.from(document.querySelectorAll<HTMLElement>('[data-conn-item]')).find((el) =>
      (el.getAttribute('data-conn-name') || '').includes(connName),
    );
    item?.click();
  }, DEMO_PG_CONN_NAME);
  await browser.pause(900);
}

/** Expand database → public schema → tables, so the tree is not an empty rail. */
async function expandDemoDbTables(): Promise<void> {
  await browser.execute((db: string) => {
    const node = Array.from(
      document.querySelectorAll<HTMLElement>('button[data-tree-node="db"]'),
    ).find((el) => el.getAttribute('data-db-name') === db);
    if (node && !node.querySelector('svg.lucide-chevron-down')) node.click();
  }, DEMO_PG_DB);
  await browser.pause(600);
  await browser.execute(() => {
    const node = Array.from(
      document.querySelectorAll<HTMLElement>('button[data-tree-node="schema"]'),
    ).find((el) => el.getAttribute('data-schema-name') === 'public');
    if (node && !node.querySelector('svg.lucide-chevron-down')) node.click();
  });
  await browser.pause(600);
}

/** Open a new query tab through the connection toolbar's own button. */
async function newQueryTab(): Promise<void> {
  await browser.waitUntil(
    async () =>
      browser.execute(() => {
        const btn = Array.from(document.querySelectorAll('button')).find((b) =>
          (b.textContent || '').includes('新建查询'),
        );
        if (!btn) return false;
        btn.click();
        return true;
      }),
    { timeout: 20000, timeoutMsg: '新建查询按钮未出现' },
  );
  await $('[data-testid="editor-execute-button"]').waitForDisplayed({
    timeout: 20000,
    timeoutMsg: '查询编辑器未打开',
  });
  await browser.pause(800);
}

describe('SQL Run Mode Screenshot', () => {
  before(async function () {
    this.timeout(180000);
    fs.mkdirSync(OUT, { recursive: true });
    mainWindow = await browser.getWindowHandle();

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

    // Best effort: without a live session the editor still opens and the scope
    // dialog still opens (the gate asks before it talks to the database), so a
    // dead demo database downgrades this spec to a skip rather than a failure.
    const dbSessionId = await invoke<string>('connect', { connectionId: DEMO_PG_CONN_ID });
    if (typeof dbSessionId === 'string' && !dbSessionId.startsWith('__error')) {
      await readCatalog({ dbSessionId, database: DEMO_PG_DB });
    } else {
      console.warn(`[run-mode] demo PostgreSQL not reachable: ${String(dbSessionId)}`);
    }

    // The strategy lives in settings, read once at app start, so it only takes
    // effect after a reload. Safe Mode is turned off because the script is
    // read-only SELECTs and the dialog should be the only thing on screen.
    originalSettings = await invoke<Record<string, unknown>>('get_settings');
    await invoke('save_settings', {
      settings: {
        ...originalSettings,
        language: 'zh-CN',
        safeMode: false,
        sqlExecutionStrategy: 'ask',
      },
    });

    await browser.url('tauri://localhost');
    await browser.pause(2500);
  });

  after(async function () {
    // Only the strategy this spec changed is restored: the capture pass as a
    // whole deliberately runs the UI in zh-CN with Safe Mode off, which the
    // other screenshot specs depend on.
    try {
      const current = await invoke<Record<string, unknown>>('get_settings');
      await invoke('save_settings', {
        settings: {
          ...current,
          sqlExecutionStrategy: originalSettings?.sqlExecutionStrategy ?? 'current_statement',
        },
      });
    } catch (err) {
      console.warn(`[run-mode] 未能还原执行策略设置: ${describeError(err)}`);
    }
    await browser.switchToWindow(mainWindow).catch(() => {});
  });

  it('34-sql-run-mode: 多语句脚本下的执行范围选择对话框', async function () {
    this.timeout(240000);
    try {
      await browser.switchToWindow(mainWindow);
      await $('[data-testid="workspace-nav-databases"]')
        .waitForClickable({ timeout: 30000, timeoutMsg: '工作区导航栏未出现' })
        .catch(() => {});
      await focusDemoConnection();
      await expandDemoDbTables();
      await newQueryTab();

      await setEditorContent(SCRIPT);
      await collapseEditorSelection();
      await clickExecute();

      // The gate only asks when the script has more than one statement, so a
      // dialog that never appears means the buffer or the strategy did not
      // take — capture nothing rather than a wrong frame under this name.
      const opened = await browser
        .waitUntil(async () => locateScopeDialog(), {
          timeout: 20000,
          timeoutMsg: '执行范围对话框未出现',
        })
        .then(() => true)
        .catch(() => false);

      if (!opened) {
        console.warn(
          `[soft-shot] ${TARGET} skipped: 执行范围对话框未出现（策略未生效或脚本未解析为多条语句）`,
        );
        return;
      }

      console.log(`[run-mode] 执行范围选项: ${JSON.stringify(await scopeDialogOptions())}`);
      await softShot(TARGET, 1200);
      await closeScopeDialog();
    } catch (err) {
      console.warn(`[soft-shot] ${TARGET} skipped: ${describeError(err)}`);
    }
  });
});
