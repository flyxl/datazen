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
    await invokeBackend('save_settings', {
      settings: {
        theme: { mode: 'dark', packId: null },
        language: 'en',
        limitSelectResults: true,
        queryResultLimit: 1000,
        editorFontSize: 13,
        editorFontFamily: 'Menlo',
        confirmOnDelete: true,
        autoCommit: true,
        safeMode: true,
        defaultPageSize: 50,
        connectionPoolSize: 10,
      },
    });
    await browser.pause(500);

    // Query a small subset — well within the 1000-row limit
    await openQueryTab();
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
    await invokeBackend('save_settings', {
      settings: {
        theme: { mode: 'dark', packId: null },
        language: 'en',
        limitSelectResults: true,
        queryResultLimit: 500,
        editorFontSize: 13,
        editorFontFamily: 'Menlo',
        confirmOnDelete: true,
        autoCommit: true,
        safeMode: true,
        defaultPageSize: 50,
        connectionPoolSize: 10,
      },
    });
    await browser.pause(500);

    await openQueryTab();
    await executeSQL(`SELECT * FROM ${TEST_TABLE}`);
    await browser.pause(2000);

    const body = await $('body').getText();
    // Should show truncation warning with the configured limit
    expect(body).toContain(t('query.resultTruncated', { limit: 500 }));

    await captureJourneyStep('qlimit-004-exceeded-limit');
  });

  // ── QLIMIT-005: Toggle limit off ──

  it('QLIMIT-005: 关闭限制后查询不应截断', async () => {
    // Disable limit entirely
    await invokeBackend('save_settings', {
      settings: {
        theme: { mode: 'dark', packId: null },
        language: 'en',
        limitSelectResults: false,
        queryResultLimit: 500,
        editorFontSize: 13,
        editorFontFamily: 'Menlo',
        confirmOnDelete: true,
        autoCommit: true,
        safeMode: true,
        defaultPageSize: 50,
        connectionPoolSize: 10,
      },
    });
    await browser.pause(500);

    await openQueryTab();
    // Query all 2000 rows — limit is off so no truncation
    await executeSQL(`SELECT * FROM ${TEST_TABLE}`);
    await browser.pause(2000);

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
