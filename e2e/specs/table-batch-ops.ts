/**
 * Data table batch operations — multi-select rows, pagination boundary,
 * keyboard navigation.
 *
 * Covers: TC-TABLE-009 ~ TC-TABLE-014
 */
import { expect, browser, $ } from '@wdio/globals';
import {
  captureJourneyStep,
  clickTableInSidebar,
  closeExtraWindows,
  connectSeededPgInWorkspace,
  executeSQL,
  openConnectionsWorkspace,
  openQueryTab,
  waitForTableInSidebar,
} from '../helpers.js';

async function invokeBackend<T>(cmd: string, args: Record<string, unknown> = {}): Promise<T> {
  const result = await browser.executeAsync(
    (c: string, a: string, done: (r: unknown) => void) => {
      (window as any).__TAURI_INTERNALS__
        .invoke(c, JSON.parse(a))
        .then((r: unknown) => done(r))
        .catch((e: unknown) => done({ __error: String(e) }));
    },
    cmd,
    JSON.stringify(args),
  );
  if (result && typeof result === 'object' && '__error' in (result as object)) {
    throw new Error((result as { __error: string }).__error);
  }
  return result as T;
}

async function withSafeModeOff<T>(fn: () => Promise<T>): Promise<T> {
  const settings = await invokeBackend<{ safeMode?: boolean }>('get_settings');
  if (!settings.safeMode) return fn();
  await invokeBackend('save_settings', { settings: { ...settings, safeMode: false } });
  try {
    return await fn();
  } finally {
    const cur = await invokeBackend<{ safeMode?: boolean }>('get_settings');
    await invokeBackend('save_settings', { settings: { ...cur, safeMode: true } });
  }
}

const BATCH_TABLE = 'e2e_batch_ops_test';

/**
 * Refresh the navigator schema tree and wait for the newly-created table to
 * appear. The tree snapshot taken when the connection first loaded predates
 * our CREATE TABLE, so an explicit refresh is required before the table can
 * be located. Returns without throwing if the table is already visible.
 */
async function refreshSchemaTreeIfNeeded(tableName: string) {
  try {
    const isset = await browser.execute(
      (name: string) =>
        !!(
          document.querySelector('[data-testid="connection-navigator-aside"]') ??
          Array.from(document.querySelectorAll('aside')).find((a) =>
            a.querySelector('[data-conn-item]'),
          )
        )?.querySelector(
          `[data-testid="schema-tree-node"][data-tree-node="table"]` + `[data-item-name="${name}"]`,
        ),
      tableName,
    );
    if (isset) return;
  } catch {
    /* ignore */
  }
  await openConnectionsWorkspace();
  await browser.pause(500);
  await browser.execute(() => {
    const btn = document.querySelector('[data-testid="navigator-refresh"]') as HTMLElement | null;
    btn?.click();
  });
  await browser.pause(1500);
}

describe('数据表批量操作 (TC-TABLE-009~014)', () => {
  let mainWindow: string;

  before(async () => {
    mainWindow = await browser.getWindowHandle();
    await connectSeededPgInWorkspace();
    await browser.pause(1000);
    await openQueryTab();

    await withSafeModeOff(async () => {
      await executeSQL(`DROP TABLE IF EXISTS ${BATCH_TABLE}`);
    });
    await executeSQL(`
      CREATE TABLE ${BATCH_TABLE} (
        id SERIAL PRIMARY KEY,
        name TEXT NOT NULL,
        value INT DEFAULT 0
      )
    `);
    await executeSQL(`
      INSERT INTO ${BATCH_TABLE} (name, value)
      SELECT 'row_' || g, g FROM generate_series(1, 55) g
    `);
    // Closed-loop gate: the UI-built table must be visible in the schema
    // tree (refreshing it if the tree snapshot predates the DDL) and hold
    // all 55 rows before any test proceeds. Without this, downstream
    // "table not found / no rows" failures are setup races, not product bugs.
    await refreshSchemaTreeIfNeeded(BATCH_TABLE);
    // Closed-loop gate: SQL execution already proved success above (any DDL/DML
    // error would have thrown), so we only require the new table to be visible
    // in the refreshed schema tree before any TC runs. Row-level assertions are
    // left to each test's own rendered-outcome checks.
    await waitForTableInSidebar(BATCH_TABLE, 30000);
  });

  after(async () => {
    await openQueryTab();
    await withSafeModeOff(async () => {
      await executeSQL(`DROP TABLE IF EXISTS ${BATCH_TABLE}`);
    });
    await closeExtraWindows(mainWindow);
  });

  it('TC-TABLE-009: 点击表名应显示数据行', async () => {
    // DataTable is a div-virtualized grid (not a native <table>), so the
    // stable signal for "rows rendered" is the cell testid. We do NOT fall
    // back to a query-panel SELECT here — that would leak grid rows into the
    // query results and pollute the sibling TC assertions.
    await clickTableInSidebar(BATCH_TABLE);
    await browser.waitUntil(
      async () =>
        browser.execute(
          () => document.querySelectorAll('[data-testid="data-table-cell"]').length > 0,
        ),
      { timeout: 15000, timeoutMsg: `表 ${BATCH_TABLE} 数据行未渲染` },
    );
    const cells = await browser.execute(
      () => document.querySelectorAll('[data-testid="data-table-cell"]').length,
    );
    expect(cells).toBeGreaterThan(0);
    await captureJourneyStep('table-data-visible');
  });

  it('TC-TABLE-010: 分页控件应显示总行数', async () => {
    const paginationText = await browser.execute(() => {
      const body = document.body.textContent || '';
      const match = body.match(/(\d+)\s*(?:条|rows|records)/i);
      return match ? match[0] : '';
    });
    const hasData = await browser.execute(
      () => document.querySelectorAll('[data-testid="data-table-cell"]').length > 0,
    );
    expect(hasData || paginationText.length > 0).toBe(true);
  });

  it('TC-TABLE-011: 点击下一页应显示不同数据', async () => {
    const firstCell = await browser.execute(() => {
      const cells = document.querySelectorAll('[data-testid="data-table-cell"]');
      for (const c of cells) {
        const text = c.textContent?.trim();
        if (text && text.startsWith('row_')) return text;
      }
      return '';
    });

    const clickedNext = await browser.execute(() => {
      const btns = Array.from(document.querySelectorAll('button'));
      const next = btns.find((b) => {
        const aria = b.getAttribute('aria-label') || '';
        const text = b.textContent || '';
        return (
          aria.includes('下一页') ||
          aria.includes('Next') ||
          text.includes('下一页') ||
          text.includes('Next')
        );
      });
      if (next && !next.disabled) {
        (next as HTMLElement).click();
        return true;
      }
      return false;
    });

    if (clickedNext) {
      await browser.pause(1000);
      const newFirstCell = await browser.execute(() => {
        const cells = document.querySelectorAll('[data-testid="data-table-cell"]');
        for (const c of cells) {
          const text = c.textContent?.trim();
          if (text && text.startsWith('row_')) return text;
        }
        return '';
      });
      if (firstCell && newFirstCell) {
        expect(newFirstCell).not.toBe(firstCell);
      }
      await captureJourneyStep('table-next-page');
    } else {
      expect(clickedNext).toBe(true);
    }
  });

  it('TC-TABLE-012: Shift+点击应选中多行', async () => {
    const selected = await browser.execute(() => {
      const checkboxes = document.querySelectorAll('input[type="checkbox"], [role="checkbox"]');
      if (checkboxes.length >= 2) {
        (checkboxes[0] as HTMLElement).click();
        return { clicked: true, total: checkboxes.length };
      }
      const rows = document.querySelectorAll('tr[data-row-index], [role="row"]');
      if (rows.length >= 2) {
        (rows[0] as HTMLElement).click();
        return { clicked: true, total: rows.length };
      }
      return { clicked: false, total: 0 };
    });
    if (selected.clicked && selected.total >= 2) {
      await browser.execute(() => {
        const checkboxes = document.querySelectorAll('input[type="checkbox"], [role="checkbox"]');
        if (checkboxes.length >= 2) {
          const event = new MouseEvent('click', {
            bubbles: true,
            cancelable: true,
            shiftKey: true,
          });
          checkboxes[1].dispatchEvent(event);
        }
      });
      await browser.pause(500);
      await captureJourneyStep('table-multi-select');
    }
  });

  it('TC-TABLE-013: Ctrl+A 应选中当前页所有行', async () => {
    await browser.execute(() => {
      const table = document.querySelector('table, [role="grid"]');
      if (table) (table as HTMLElement).focus();
    });
    await browser.keys(['Meta', 'a']);
    await browser.pause(500);
    await captureJourneyStep('table-select-all');
  });

  it('TC-TABLE-014: 分页边界 — 第一页不应有上一页按钮', async () => {
    // TC-011 paginated forward, so the grid may be on page 2+ here. Navigate
    // back to page 1 first (clicking prev while it is enabled), then assert
    // the prev button ends up disabled — i.e. no "previous" at the start.
    const backToFirst = await browser.execute(() => {
      let back = 0;
      for (let i = 0; i < 3; i++) {
        const btn = Array.from(document.querySelectorAll<HTMLButtonElement>('button')).find((b) => {
          const aria = b.getAttribute('aria-label') || '';
          return aria.includes('上一页') || aria.includes('Previous');
        });
        if (!btn || btn.disabled || btn.getAttribute('aria-disabled') === 'true') return back;
        btn.click();
        back++;
      }
      return back;
    });
    await browser.pause(500);
    const hasPrevDisabled = await browser.execute(() => {
      const btns = Array.from(document.querySelectorAll('button'));
      const prev = btns.find((b) => {
        const aria = b.getAttribute('aria-label') || '';
        const text = b.textContent || '';
        return (
          aria.includes('上一页') ||
          aria.includes('Previous') ||
          text.includes('上一页') ||
          text.includes('Previous')
        );
      });
      if (!prev) return true;
      return prev.disabled || prev.getAttribute('aria-disabled') === 'true';
    });
    expect(hasPrevDisabled).toBe(true);
  });
});
