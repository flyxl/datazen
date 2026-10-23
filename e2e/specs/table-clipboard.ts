import { expect, browser, $ } from '@wdio/globals';
import { closeWorkspacePanels } from '../lib/testDataLifecycle.js';
import {
  clickTableInSidebar,
  closeExtraWindows,
  executeSQLChecked,
  openQueryTab,
  setSafeMode,
  waitForNewQueryButton,
  waitForTableInSidebar,
} from '../helpers.js';

/**
 * Clipboard copy / paste E2E (DB-03 §9.5).
 *
 * **Known WebKit/WebDriver limitations — recorded, not worked around by changing the product.**
 *
 * 1. `e2e/specs/table-edit.ts` already documents that this WebView's WebDriver cannot
 *    reliably drive keyboard input, so `⌘C` / `⌘V` are *dispatched* (`copy` / `paste` events)
 *    rather than typed. The product keeps the native listeners; only the test synthesizes the
 *    gesture. Nothing in the grid was moved back onto `useKeyboardShortcuts` for the test.
 * 2. `new ClipboardEvent('paste', { clipboardData })` is not guaranteed on WebKit: the
 *    constructor may hand back an event whose `clipboardData` is `null` and read-only. The
 *    helper below therefore builds a plain `Event` and defines `clipboardData` on it; if that
 *    define fails, the step fails loudly rather than silently asserting nothing (U-6).
 * 3. The real OS clipboard is never read: the write path is asserted through
 *    `clipboardData.setData`, which is what the grid calls.
 *
 * The journey mirrors the unit-level one (`clipboard.journey.test.tsx`) but against the real
 * application: select → paste → staged → paste again → copy → compare text → rollback.
 */

const TEST_TABLE = '_e2e_clipboard_test';

type DispatchOutcome = {
  ok: boolean;
  text: string | null;
  prevented: boolean;
  reason?: string;
};

/**
 * Dispatch a `copy` or `paste` on the grid surface with a synthetic `clipboardData`.
 *
 * Returns what the *grid* did: the text it read (`paste`), the text it wrote (`copy`) and
 * whether it called `preventDefault` (i.e. whether the gesture was treated as its own).
 */
async function dispatchClipboard(type: 'copy' | 'paste', text: string): Promise<DispatchOutcome> {
  return (await browser.execute(
    (eventType: 'copy' | 'paste', payload: string): DispatchOutcome => {
      const surface = document.querySelector('[data-dt-surface]');
      if (!surface) return { ok: false, text: null, prevented: false, reason: 'no grid surface' };
      const store: Record<string, string> = { 'text/plain': payload, 'text/html': '' };
      const clipboardData = {
        getData: (mime: string) => store[mime] ?? '',
        setData: (mime: string, value: string) => {
          store[mime] = value;
        },
        types: Object.keys(store),
      };
      const event = new Event(eventType, { bubbles: true, cancelable: true });
      try {
        Object.defineProperty(event, 'clipboardData', { value: clipboardData, configurable: true });
      } catch (error) {
        return {
          ok: false,
          text: null,
          prevented: false,
          reason: `clipboardData is not definable on this WebKit: ${String(error)}`,
        };
      }
      surface.dispatchEvent(event);
      return {
        ok: true,
        text: eventType === 'copy' ? (store['text/plain'] ?? null) : payload,
        prevented: event.defaultPrevented,
      };
    },
    type,
    text,
  )) as DispatchOutcome;
}

/** The staged-change count the store holds for the open panel, straight from the DEV probe. */
async function pendingRowCount(): Promise<number> {
  return (await browser.execute(() => {
    const probe = (window as unknown as {
      __tableDataStore?: { getState: () => { byPanel: Map<string, { pendingChanges: Map<string, unknown> }> } };
    }).__tableDataStore;
    if (!probe) return -1;
    const panels = [...probe.getState().byPanel.values()];
    return panels.reduce((total, panel) => total + panel.pendingChanges.size, 0);
  })) as number;
}

/** Click one cell by its `data-dt-*` coordinates (never by geometry). */
async function clickCell(rowIndex: number, columnName: string) {
  const cell = await $(`[data-dt-row="${rowIndex}"][data-dt-col="${columnName}"]`);
  await cell.waitForDisplayed({ timeout: 8000 });
  await cell.click();
  await browser.pause(200);
}

describe('表数据剪贴板复制与粘贴 (DB-03)', () => {
  let mainWindow: string;

  before(async () => {
    mainWindow = await browser.getWindowHandle();
    await setSafeMode(false);
    await executeSQLChecked(`DROP TABLE IF EXISTS ${TEST_TABLE}`);

    // One query tab owns one retained session; the fixture reuses it, exactly like the
    // table-edit spec does (a fresh tab per statement exhausts the session quota).
    const tabs = await $$('[data-testid="panel-tab-select"]');
    if (tabs.length === 0) {
      await waitForNewQueryButton(20000);
      await openQueryTab();
    }
    await $('[data-testid="editor-execute-button"]').waitForDisplayed({ timeout: 10000 });

    await executeSQLChecked(
      `CREATE TABLE ${TEST_TABLE} (
        id SERIAL PRIMARY KEY,
        name VARCHAR(100) NOT NULL,
        score INT NOT NULL DEFAULT 0
      )`,
    );
    await executeSQLChecked(
      `INSERT INTO ${TEST_TABLE} (name, score) VALUES ('Alice', 100), ('Bob', 200)`,
    );
    await browser.pause(1200);
    const refreshBtn = await $('[data-testid="navigator-refresh"]');
    await refreshBtn.waitForDisplayed({ timeout: 10000 });
    await refreshBtn.click();
    await waitForTableInSidebar(TEST_TABLE);

    await clickTableInSidebar(TEST_TABLE);
    await $('[data-testid="sub-tab-data"]').waitForDisplayed({ timeout: 10000 });
    await browser.pause(1200);
  });

  after(async () => {
    try {
      await browser.switchToWindow(mainWindow);
      await executeSQLChecked(`DROP TABLE IF EXISTS ${TEST_TABLE}`);
    } catch {
      // best-effort cleanup
    }
    await setSafeMode(true);
    await closeExtraWindows(mainWindow);
    await closeWorkspacePanels(browser);
  });

  it('粘贴一格即进入待提交状态 (DB-03 A3)', async () => {
    await clickCell(0, 'name');
    expect(await pendingRowCount()).toBe(0);

    const outcome = await dispatchClipboard('paste', 'Alicia');
    expect(outcome.ok).toBe(true);
    // The grid consumed the gesture: the browser must not also paste into the page.
    expect(outcome.prevented).toBe(true);

    // A cold session has no recorded copy source, so the paste asks first.
    await $('[data-testid="confirm-dialog-ok"]').waitForDisplayed({ timeout: 8000 });
    await $('[data-testid="confirm-dialog-ok"]').click();
    await browser.pause(600);

    expect(await pendingRowCount()).toBe(1);
    const status = await $('[data-testid="table-clipboard-status"]');
    await status.waitForDisplayed({ timeout: 5000 });
    expect(await status.getAttribute('data-dt-paste-staged')).toBe('1');
    expect(await $(`[data-dt-row="0"][data-dt-col="name"]`).getAttribute('data-dt-dirty')).toBe(
      'true',
    );
  });

  it('再次粘贴覆盖同一行的暂存修改，而不是新增一条', async () => {
    await clickCell(0, 'name');
    await dispatchClipboard('paste', 'Alicia2');
    await $('[data-testid="confirm-dialog-ok"]').waitForDisplayed({ timeout: 8000 });
    await $('[data-testid="confirm-dialog-ok"]').click();
    await browser.pause(600);

    expect(await pendingRowCount()).toBe(1);
    const status = await $('[data-testid="table-clipboard-status"]');
    expect(await status.getAttribute('data-dt-paste-staged')).toBe('1');
  });

  it('复制选中区域，文本与源区域逐字符相等 (DB-03 A3)', async () => {
    // One cell, already carrying the staged value: the copy must read the row as it is now.
    await clickCell(0, 'name');
    const outcome = await dispatchClipboard('copy', '');
    expect(outcome.ok).toBe(true);
    expect(outcome.prevented).toBe(true);
    expect(outcome.text).toBe('Alicia2');
  });

  it('行内粘贴不回写只读列，且整批只产生一次状态变更 (DB-03 §3.4)', async () => {
    // A two-row, two-column block landed on the first cell: the id column is a primary key
    // and therefore writable, so nothing is refused here — the point is the batch semantics.
    const before = await pendingRowCount();
    await clickCell(1, 'name');
    const outcome = await dispatchClipboard('paste', 'Bob2\t42');
    expect(outcome.ok).toBe(true);
    expect(outcome.prevented).toBe(true);
    await $('[data-testid="confirm-dialog-ok"]').waitForDisplayed({ timeout: 8000 });
    await $('[data-testid="confirm-dialog-ok"]').click();
    await browser.pause(700);

    expect(await pendingRowCount()).toBe(before + 1);
    expect(await $(`[data-dt-row="1"][data-dt-col="name"]`).getAttribute('data-dt-dirty')).toBe(
      'true',
    );
  });

  it('拒绝畸形块时不改动任何暂存修改 (DB-03 §7)', async () => {
    const before = await pendingRowCount();
    const outcome = await dispatchClipboard('paste', 'a\tb\nc');
    expect(outcome.ok).toBe(true);
    // Ragged input is refused before planning: no dialog, no write.
    await browser.pause(400);
    expect(await pendingRowCount()).toBe(before);
    const status = await $('[data-testid="table-clipboard-status"]');
    await status.waitForDisplayed({ timeout: 5000 });
    expect(await status.getText()).not.toBe('');
  });

  it('回滚后剪贴板暂存的修改全部消失', async () => {
    const rollback = await $('[data-testid="pending-rollback"]');
    await rollback.waitForDisplayed({ timeout: 8000 });
    await rollback.click();
    await browser.waitUntil(async () => (await pendingRowCount()) === 0, {
      timeout: 15000,
      timeoutMsg: '回滚后仍有暂存修改',
    });
  });
});
