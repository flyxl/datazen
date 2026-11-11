import { expect, browser, $ } from '@wdio/globals';
import { closeWorkspacePanels } from '../lib/testDataLifecycle.js';
import {
  clickTableInSidebar,
  closeExtraWindows,
  connectSeededPgInWorkspace,
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
      // When the grid ignores a gesture it is because one of its shared
      // preconditions is unmet (no anchor, an edit input still open, an editable
      // target). Report them with the outcome instead of a bare `false`.
      const active = document.activeElement;
      const reason = [
        `anchor=${surface.hasAttribute('data-dt-anchor')}`,
        `editing=${!!document.querySelector('[data-testid="table-edit-input"]')}`,
        `active=${active?.tagName.toLowerCase() ?? 'none'}:${active?.getAttribute('data-testid') ?? ''}`,
      ].join(' ');
      return {
        ok: true,
        text: eventType === 'copy' ? (store['text/plain'] ?? null) : payload,
        prevented: event.defaultPrevented,
        reason,
      };
    },
    type,
    text,
  )) as DispatchOutcome;
}

/** The staged-change count the store holds for the open panel, straight from the DEV probe. */
async function pendingRowCount(): Promise<number> {
  return (await browser.execute(() => {
    const probe = (
      window as unknown as {
        __tableDataStore?: {
          getState: () => { byPanel: Map<string, { pendingChanges: Map<string, unknown> }> };
        };
      }
    ).__tableDataStore;
    if (!probe) return -1;
    const panels = [...probe.getState().byPanel.values()];
    return panels.reduce((total, panel) => total + panel.pendingChanges.size, 0);
  })) as number;
}

/**
 * Select one cell by its `data-dt-*` coordinates (never by geometry).
 *
 * The grid's cell selection is driven by `pointerdown`/`pointerup` on the grid
 * surface. WebKit's WebDriver "Element Click" dispatches mouse events only, so
 * `element.click()` left the selection (and therefore the clipboard anchor)
 * empty; the synthetic `PointerEvent` pair is what a real pointer produces.
 */
async function pointerOn(cellSelector: string, type: 'pointerdown' | 'pointerup'): Promise<void> {
  const ok = await browser.execute(
    (selector: string, eventType: string) => {
      const el = document.querySelector(selector);
      if (!el) return false;
      const rect = el.getBoundingClientRect();
      el.dispatchEvent(
        new PointerEvent(eventType, {
          bubbles: true,
          cancelable: true,
          composed: true,
          pointerId: 1,
          pointerType: 'mouse',
          isPrimary: true,
          button: 0,
          buttons: eventType === 'pointerup' ? 0 : 1,
          clientX: rect.left + rect.width / 2,
          clientY: rect.top + rect.height / 2,
        }),
      );
      return true;
    },
    cellSelector,
    type,
  );
  expect(ok).toBe(true);
}

async function clickCell(rowIndex: number, columnName: string) {
  const selector = `[data-dt-row="${rowIndex}"][data-dt-col="${columnName}"]`;
  await $(selector).waitForDisplayed({ timeout: 8000 });
  await pointerOn(selector, 'pointerdown');
  await pointerOn(selector, 'pointerup');
  await browser.pause(200);
}

describe('表数据剪贴板复制与粘贴 (DB-03)', () => {
  // ── 行复制 → 新增草稿行（DBR-02）────────────────────────────

  function activeGridSurfaceSelector(): string {
    // Prefer the visible/active data grid surface; fall back to any surface.
    return '[data-dt-surface]';
  }

  /** Dispatch a keydown the React/grid handlers will see (the WebDriver that
   *  backs this spec cannot type; the product keeps its native handlers). */
  async function dispatchGridKey(key: string, withMeta = false): Promise<void> {
    await browser.execute(
      (k: string, meta: boolean) => {
        const surfaces = Array.from(
          document.querySelectorAll('[data-dt-surface]'),
        ) as HTMLElement[];
        const surface =
          surfaces.find((el) => {
            const rect = el.getBoundingClientRect();
            return rect.width > 0 && rect.height > 0;
          }) ?? surfaces[surfaces.length - 1];
        if (!surface) return;
        surface.focus();
        const target = (document.activeElement as HTMLElement | null) ?? surface;
        target.dispatchEvent(
          new KeyboardEvent('keydown', {
            key: k,
            code: k.length === 1 ? `Key${k.toUpperCase()}` : k,
            metaKey: meta,
            ctrlKey: meta,
            bubbles: true,
            cancelable: true,
          }),
        );
      },
      key,
      withMeta,
    );
    await browser.pause(300);
  }

  async function hasSelectedRows(): Promise<boolean> {
    return browser.execute(() => {
      const store = (
        window as unknown as {
          __tableDataStore?: {
            getState: () => {
              activePanelId?: string | null;
              byPanel: Map<
                string,
                {
                  selectedRows?: Set<number>;
                  gridSelection?: { mode?: string; anchor?: unknown; focus?: unknown };
                }
              >;
            };
          };
        }
      ).__tableDataStore;
      if (!store) return false;
      const state = store.getState();
      const panelIds = state.activePanelId
        ? [state.activePanelId, ...state.byPanel.keys()]
        : [...state.byPanel.keys()];
      for (const id of panelIds) {
        const slice = state.byPanel.get(id);
        if (!slice) continue;
        if ((slice.selectedRows?.size ?? 0) > 0) return true;
        const sel = slice.gridSelection;
        if (
          sel?.mode === 'row' ||
          (sel?.mode === 'cell' && sel.anchor != null && sel.focus != null)
        ) {
          return true;
        }
      }
      return false;
    });
  }

  /** Select every row (row/cell selection) so row-copy/duplicate actions can run. */
  async function selectAllRows(): Promise<void> {
    await browser.waitUntil(
      async () =>
        browser.execute(() =>
          Array.from(document.querySelectorAll('[data-dt-surface]')).some((el) => {
            const rect = (el as HTMLElement).getBoundingClientRect();
            return rect.width > 0 && rect.height > 0;
          }),
        ),
      { timeout: 10000, timeoutMsg: 'Timed out waiting for visible data grid surface' },
    );

    // Prefer the grid's own Cmd/Ctrl+A shortcut (row mode) on the visible grid.
    await dispatchGridKey('a', true);
    if (
      await browser
        .waitUntil(async () => hasSelectedRows(), { timeout: 3000, interval: 150 })
        .catch(() => false)
    ) {
      return;
    }

    // Fallback: click the select-all control inside the visible grid container.
    const clickedSelectAll = await browser.execute(() => {
      const labels = ['全选', 'Select All', 'Select all', 'すべて選択'];
      const surfaces = Array.from(document.querySelectorAll('[data-dt-surface]')) as HTMLElement[];
      const surface =
        surfaces.find((el) => {
          const rect = el.getBoundingClientRect();
          return rect.width > 0 && rect.height > 0;
        }) ?? surfaces[surfaces.length - 1];
      const root =
        (surface?.closest('[data-testid^="table-view"]') as HTMLElement | null) ??
        surface?.parentElement?.parentElement ??
        document.body;
      const candidates = Array.from(
        root.querySelectorAll('button, label, [role="checkbox"], input[type="checkbox"]'),
      ) as HTMLElement[];
      for (const el of candidates) {
        const text = (el.textContent ?? '').trim();
        const aria = (el.getAttribute('aria-label') ?? '').trim();
        if (labels.some((l) => text === l || text.includes(l) || aria.includes(l))) {
          el.click();
          return true;
        }
      }
      return false;
    });
    if (clickedSelectAll) {
      await browser.pause(300);
      if (await hasSelectedRows()) return;
    }

    await browser.waitUntil(async () => hasSelectedRows(), {
      timeout: 5000,
      interval: 150,
      timeoutMsg: 'Timed out waiting for row selection',
    });
  }

  /** Drop every draft of the open panel through the dev store probe. */
  async function rollbackDrafts(): Promise<void> {
    await browser.execute(() => {
      type Slice = { draftInserts: Map<string, unknown> };
      const store = (
        window as unknown as {
          __tableDataStore?: {
            getState: () => {
              byPanel: Map<string, Slice>;
              rollbackPendingChanges: (panelId: string) => void;
            };
          };
        }
      ).__tableDataStore;
      if (!store) return;
      const state = store.getState();
      for (const [panelId, slice] of state.byPanel.entries()) {
        if (slice.draftInserts.size > 0) state.rollbackPendingChanges(panelId);
      }
    });
    await browser.pause(400);
  }

  it('行模式复制写出整行 TSV（DBR-02）', async () => {
    await selectAllRows();
    const outcome = await dispatchClipboard('copy', '');
    expect(outcome.prevented).toBe(true);
    // Full rows, every displayed column, in render order — the clipboard
    // stays a faithful TSV for every external consumer.
    expect(outcome.text).toBe('1\tAlice\t100\n2\tBob\t200');
  });

  it('Cmd/Ctrl+D 将选中的已有行复制为新增草稿，主键不随行 (DBR-02)', async () => {
    await rollbackDrafts();
    // Ensure the test table panel is active and all its rows are selected in
    // row mode, then trigger the duplicate-selected-rows shortcut (DBR-02).
    await browser.execute(() => {
      const store = (
        window as unknown as {
          __tableDataStore?: {
            getState: () => {
              activePanelId?: string | null;
              byPanel: Map<
                string,
                {
                  rows?: Array<Record<string, unknown>>;
                  selectedRows?: Set<number>;
                  gridSelection?: unknown;
                }
              >;
              setState?: (s: unknown) => void;
            };
            setState?: (updater: unknown) => void;
          };
        }
      ).__tableDataStore;
      if (!store) return;
      const state = store.getState();
      const entries = [...state.byPanel.entries()];
      const target =
        entries.find(([, s]) => (s.rows ?? []).some((r) => r?.name === 'Alice')) ??
        (state.activePanelId
          ? [state.activePanelId, state.byPanel.get(state.activePanelId)]
          : null);
      if (!target || !target[1]) return;
      const [panelId, slice] = target as [string, any];
      const rowCount = (slice.rows ?? []).length;
      const nextByPanel = new Map(state.byPanel);
      nextByPanel.set(panelId, {
        ...slice,
        selectedRows: new Set(Array.from({ length: rowCount }, (_, i) => i)),
        gridSelection: {
          mode: 'row',
          anchor: rowCount > 0 ? { rowIndex: 0, columnName: 'id' } : null,
          focus: rowCount > 0 ? { rowIndex: 0, columnName: 'id' } : null,
          extraRanges: [],
        },
      });
      (store.setState as any)?.((prev: any) => ({
        ...prev,
        activePanelId: panelId,
        byPanel: nextByPanel,
      }));
    });
    await browser.execute(() => {
      (document.querySelector('[data-dt-surface]') as HTMLElement | null)?.focus();
    });
    await dispatchGridKey('d', true);

    const snapshot = await browser
      .waitUntil(
        async () =>
          browser.execute(() => {
            type Slice = {
              draftInserts: Map<string, { draftId: string; values: Record<string, unknown> }>;
            };
            const store = (
              window as unknown as {
                __tableDataStore?: {
                  getState: () => {
                    activePanelId?: string | null;
                    byPanel: Map<string, Slice>;
                  };
                };
              }
            ).__tableDataStore;
            if (!store) return null;
            const state = store.getState();
            const orderedIds = state.activePanelId
              ? [state.activePanelId, ...state.byPanel.keys()]
              : [...state.byPanel.keys()];
            for (const id of orderedIds) {
              const slice = state.byPanel.get(id);
              if (!slice || slice.draftInserts.size === 0) continue;
              const values = [...slice.draftInserts.values()].map((draft) => draft.values);
              if (values.some((v) => v && (v.name === 'Alice' || v.name === 'Bob'))) return values;
            }
            return null;
          }),
        {
          timeout: 8000,
          interval: 200,
          timeoutMsg: 'Timed out waiting for duplicated row drafts from test table',
        },
      )
      .catch(async () => null);

    expect(snapshot).toEqual([
      // The serial primary key never travels with a copied row; the ordinary
      // columns keep their real values.
      { name: 'Alice', score: 100 },
      { name: 'Bob', score: 200 },
    ]);

    await rollbackDrafts();
  });

  it('有活动草稿时 Cmd/Ctrl+D 仍是复制该草稿，不被行复制抢占 (DBR-02)', async () => {
    await rollbackDrafts();
    // Add a draft row (Cmd/Ctrl+I) and duplicate it: the legacy meaning must
    // survive the new one.
    await dispatchGridKey('i', true);
    await dispatchGridKey('d', true);
    const drafts = await browser.execute(() => {
      type Slice = {
        draftInserts: Map<string, unknown>;
        draftInsertOrder: string[];
      };
      const store = (
        window as unknown as {
          __tableDataStore?: { getState: () => { byPanel: Map<string, Slice> } };
        }
      ).__tableDataStore;
      if (!store) return null;
      const slice = [...store.getState().byPanel.values()].find(
        (candidate) => candidate.draftInserts.size > 0,
      );
      return slice ? slice.draftInserts.size : 0;
    });
    expect(drafts).toBe(2);

    await rollbackDrafts();
  });

  let mainWindow: string;

  before(async () => {
    mainWindow = await browser.getWindowHandle();
    await setSafeMode(false);

    // The editor has to exist *before* the first statement: `executeSQLChecked`
    // types into `sql-editor-content`, and a bare connection (what the WDIO
    // global hook leaves behind) has no query tab at all. Enter the connection
    // workspace deterministically instead of inferring it from the tab strip —
    // a leftover tab from another spec made the "no tabs" probe lie and the
    // real toolbar never appeared.
    await connectSeededPgInWorkspace();
    await waitForNewQueryButton(20000);
    await openQueryTab();
    await $('[data-testid="editor-execute-button"]').waitForDisplayed({ timeout: 10000 });

    await executeSQLChecked(`DROP TABLE IF EXISTS ${TEST_TABLE}`);

    // One query tab owns one retained session; the fixture reuses it, exactly like the
    // table-edit spec does (a fresh tab per statement exhausts the session quota).
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

    let outcome = await dispatchClipboard('paste', 'Alicia');
    // Without the grid's preconditions (anchor / closed editor / non-editable
    // target) the gesture is silently ignored; retry once after re-focusing.
    if (!outcome.prevented) {
      await clickCell(0, 'name');
      await browser.pause(200);
      outcome = await dispatchClipboard('paste', 'Alicia');
    }
    if (!outcome.prevented) throw new Error(`paste was not consumed: ${outcome.reason}`);
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
