/**
 * E2E journey tests for the query row limit feature.
 *
 * Verifies:
 * 1. Default limit is 1000 rows
 * 2. Settings UI for changing the limit
 * 3. Truncation warning appears when limit is exceeded
 * 4. Limit can be toggled on/off
 * 5. Version display in Settings → General
 *
 * Requires a PostgreSQL connection (seeded by wdio.conf.ts before hook).
 * Creates a disposable connection and a test table with >1000 rows to exercise
 * the row-limit path.
 */
import { expect, browser, $ } from '@wdio/globals';
import {
  captureJourneyStep,
  clickCardConnectButton,
  closeExtraWindows,
  executeSQL,
  openConnectionsWorkspace,
  openQueryTab,
  openSettingsInMainWindow,
  backFromSettingsInMainWindow,
  waitForConnectionToolbar,
  invokeBackend,
} from '../../helpers.js';
import { t } from '../../i18n.js';

const CONNECTION_ID = 'e2e_pg_row_limit';
const CONNECTION_NAME = 'E2E-PG-行数限制';
const TEST_TABLE = 'e2e_row_limit_test';

/**
 * Open a query tab after a Settings round-trip.
 *
 * openSettingsInMainWindow() navigates with browser.url() — a full page
 * reload that drops the frontend session, so afterwards the app sits on the
 * connections list where neither `conn-toolbar-new-query` (connection
 * workspace toolbar) nor `home-quick-new-query` (connected-home quick action)
 * exists. openQueryTab() then polls for 30s and dies at its final
 * waitForDisplayed. Re-run the same connect path the before() hook uses
 * (fast path when the workspace survived).
 */
async function openQueryTabWithConnection(): Promise<void> {
  try {
    const toolbarBtn = await $('[data-testid="conn-toolbar-new-query"]');
    const quickBtn = await $('[data-testid="home-quick-new-query"]');
    const connected =
      (await toolbarBtn.isExisting().catch(() => false)) ||
      (await quickBtn.isExisting().catch(() => false));
    if (connected) {
      await openQueryTab();
      return;
    }
  } catch {
    /* fall through to reconnect */
  }
  await openConnectionsWorkspace();
  await clickCardConnectButton(CONNECTION_NAME);
  await waitForConnectionToolbar();
  await openQueryTab();
}

/**
 * Re-open the fixture editor instead of adding another query tab.
 *
 * Every query tab is its own retained editor session and the connected-editor
 * budget is 5, so the later cases used to open a tab that had no usable session
 * — the execute click then did nothing and the assertion only saw an empty pane.
 */
async function reuseQueryTab(): Promise<void> {
  const tabs = await $$('[data-testid="panel-tab-select"]');
  if (tabs.length > 0) {
    await tabs[0].click();
    await browser.pause(400);
    await $('[data-testid="editor-execute-button"]').waitForDisplayed({ timeout: 10000 });
    return;
  }
  await openQueryTabWithConnection();
}

/**
 * Persist the row-limit settings so the very next query uses them.
 *
 * `save_settings` over raw IPC writes the backend, but the renderer keeps the
 * settings store it booted with — the UI's own save path is what refreshes it.
 * Re-entering Settings re-hydrates that store, otherwise the next query still
 * applies the previous limit and the truncation banner reports the old number.
 */
async function applyRowLimitSettings(limitSelectResults: boolean, queryResultLimit: number) {
  await invokeBackend('save_settings', {
    settings: {
      theme: { mode: 'dark', packId: null },
      // Keep the UI in the language this journey's `t()` helper resolves
      // (e2e/i18n.ts is zh-CN). Once the Settings round-trip below actually
      // applies the saved settings, a different language flips every label the
      // assertions look for.
      language: 'zh-CN',
      limitSelectResults,
      queryResultLimit,
      editorFontSize: 13,
      editorFontFamily: 'Menlo',
      confirmOnDelete: true,
      autoCommit: true,
      safeMode: false,
      defaultPageSize: 50,
      connectionPoolSize: 10,
    },
  });
  await browser.pause(400);
  await openSettingsInMainWindow('dataBrowsing');
  await browser.pause(400);
  await backFromSettingsInMainWindow();
  await browser.pause(400);
}

/** Wait until the body text contains `needle` (large result sets arrive in batches). */
async function waitForBodyText(needle: string, timeout: number): Promise<void> {
  await browser.waitUntil(async () => (await $('body').getText()).includes(needle), {
    timeout,
    interval: 500,
    timeoutMsg: `等待页面出现「${needle}」超时`,
  });
}

describe('数据查询行数限制 Journey（QLIMIT-001~QLIMIT-006）', () => {
  let mainWindow: string;

  before(async () => {
    mainWindow = await browser.getWindowHandle();

    // Create a disposable PG connection
    await invokeBackend('save_connection', {
      config: {
        id: CONNECTION_ID,
        name: CONNECTION_NAME,
        databaseType: 'postgresql',
        host: process.env.E2E_PG_HOST || '127.0.0.1',
        port: Number(process.env.E2E_PG_PORT) || 5432,
        username: process.env.E2E_PG_USER || 'postgres',
        password: process.env.E2E_PG_PASSWORD || '',
        database: process.env.E2E_PG_DB || 'postgres',
        schema: process.env.E2E_WORKER_SCHEMA || undefined,
        group: 'E2E 测试',
        colorTag: 'green',
        sslMode: 'disable',
        options: {},
      },
    });
    await browser.refresh();
    await browser.pause(1500);

    // Connect to the database
    await openConnectionsWorkspace();
    await clickCardConnectButton(CONNECTION_NAME);
    await waitForConnectionToolbar();
    await browser.pause(1000);

    // Open a query tab and prepare test data
    await openQueryTab();

    // Create a test table with 2000 rows to trigger row-limit scenarios
    await executeSQL(`DROP TABLE IF EXISTS ${TEST_TABLE}`);
    await executeSQL(`
      CREATE TABLE ${TEST_TABLE} AS
      SELECT generate_series(1, 2000) AS id,
             'row_' || generate_series(1, 2000) AS label
    `);
    await browser.pause(500);
  });

  after(async () => {
    try {
      // Clean up test table
      await executeSQL(`DROP TABLE IF EXISTS ${TEST_TABLE}`);
    } catch {
      /* cleanup best-effort */
    }
    // Restore default settings
    await invokeBackend('save_settings', {
      settings: {
        theme: { mode: 'dark', packId: null },
        language: 'en',
        limitSelectResults: true,
        queryResultLimit: 1000,
        editorFontSize: 13,
        editorFontFamily: 'Menlo, Monaco, Consolas, "Courier New", monospace',
        confirmOnDelete: true,
        autoCommit: true,
        safeMode: true,
        defaultPageSize: 50,
        connectionPoolSize: 10,
      },
    });
    await closeExtraWindows(mainWindow);
    // Remove disposable connection
    try {
      await invokeBackend('delete_connection', { id: CONNECTION_ID });
    } catch {
      /* cleanup best-effort */
    }
  });

  // ── QLIMIT-001: Default limit is 1000 ──

  it('QLIMIT-001: 默认 queryResultLimit 应为 1000', async () => {
    const settings = await invokeBackend<any>('get_settings');
    expect(settings.queryResultLimit).toBe(1000);
    expect(settings.limitSelectResults).toBe(true);
    await captureJourneyStep('qlimit-001-default');
  });

  // ── QLIMIT-002: Settings UI shows limit controls ──

  it('QLIMIT-002: 设置页面数据浏览区应显示限制 SELECT 结果行数开关和最大返回行数选项', async () => {
    await openSettingsInMainWindow('dataBrowsing');
    await browser.pause(500);

    const body = await $('body').getText();
    // Should show the limit toggle
    expect(body.includes('Limit') || body.includes('限制') || body.includes('SELECT')).toBe(true);

    // Should show max rows option
    expect(
      body.includes('Max') ||
        body.includes('最大') ||
        body.includes('1,000') ||
        body.includes('1000'),
    ).toBe(true);

    await captureJourneyStep('qlimit-002-settings-ui');
    await backFromSettingsInMainWindow();
  });

  // ── QLIMIT-003: Query within limit shows no truncation ──

  it('QLIMIT-003: 查询结果未超过限制时不应显示截断警告', async () => {
    // Set limit to 1000
    await applyRowLimitSettings(true, 1000);

    // Query a small subset — well within the 1000-row limit
    await openQueryTabWithConnection();
    await executeSQL(`SELECT * FROM ${TEST_TABLE} WHERE id <= 100`);
    await browser.pause(1000);

    const body = await $('body').getText();
    // Should show 100 rows
    expect(body).toContain(`100 ${t('common.rows')}`);
    // Should NOT show truncation warning
    expect(body).not.toContain(t('query.resultTruncated', { limit: 1000 }));

    await captureJourneyStep('qlimit-003-within-limit');
  });

  // ── QLIMIT-004: Query exceeding limit shows truncation ──

  it('QLIMIT-004: 查询结果超过限制时应显示截断警告', async () => {
    // Lower the limit to 500 so the 2000-row table triggers truncation
    await applyRowLimitSettings(true, 500);

    // Reuse the fixture editor: a new tab is a new retained session, and the
    // connected-editor budget is smaller than this journey's tab count.
    await reuseQueryTab();
    // An unbounded SELECT is refused by Safe Mode, so the guard is off above:
    // this case is about the row limit, not about blocking.
    await executeSQL(`SELECT * FROM ${TEST_TABLE}`);
    await waitForBodyText(t('query.resultTruncated', { limit: 500 }), 30000);

    const body = await $('body').getText();
    // Should show truncation warning with the configured limit
    expect(body).toContain(t('query.resultTruncated', { limit: 500 }));

    await captureJourneyStep('qlimit-004-exceeded-limit');
  });

  // ── QLIMIT-005: Toggle limit off ──

  it('QLIMIT-005: 关闭限制后查询不应截断', async () => {
    await applyRowLimitSettings(false, 500);

    await reuseQueryTab();
    // Query all 2000 rows — limit is off so no truncation
    await executeSQL(`SELECT * FROM ${TEST_TABLE}`);
    await waitForBodyText(`2000 ${t('common.rows')}`, 30000);

    const body = await $('body').getText();
    // Should show all 2000 rows
    expect(body).toContain(`2000 ${t('common.rows')}`);
    // Should NOT show truncation warning
    expect(body).not.toContain(t('query.resultTruncated', { limit: 500 }));

    await captureJourneyStep('qlimit-005-limit-disabled');
  });

  // ── QLIMIT-006: Version display in Settings → General ──

  it('QLIMIT-006: 设置页面通用区应显示 DataZen 版本号', async () => {
    await openSettingsInMainWindow('general');
    await browser.pause(500);

    const body = await $('body').getText();
    // Should show version with "v" prefix
    expect(body).toMatch(/DataZen v\d+\.\d+\.\d+/);

    await captureJourneyStep('qlimit-006-version-display');
    await backFromSettingsInMainWindow();
  });
});
