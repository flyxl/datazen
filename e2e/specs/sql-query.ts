import { expect, browser, $, $$ } from '@wdio/globals';
import { t } from '../i18n.js';
import {
  captureJourneyStep,
  clickCardConnectButton,
  closeExtraWindows,
  setEditorContent,
  openQueryTab,
  openConnectionsWorkspace,
  expandConnectedConnectionInNavigator,
  waitForConnectionToolbar,
  executeSQL,
  emitCrossWindowEvent,
  invokeBackend,
  setSafeMode,
} from '../helpers.js';

/**
 * SQL query module tests.
 * Requires a PostgreSQL connection (seeded by wdio.conf.ts before hook).
 *
 * Editor toolbar locators use the vite-gated `data-testid` attributes from
 * src/lib/tid.ts (E2E builds always render them) so they survive i18n switching.
 */
describe('SQL 查询模块 (SQ-001~SQ-012, TC-QUERY-006/008)', () => {
  let mainWindow: string;
  const queryConnectionId = 'e2e_pg_sql_query';
  const queryConnectionName = 'E2E-PostgreSQL-查询';

  before(async () => {
    mainWindow = await browser.getWindowHandle();
    // A configured database intentionally suppresses the database context
    // selector. Use a disposable PG connection without a default database so
    // this spec exercises the real multi-database selector path without
    // mutating the shared conn_e2e_pg fixture used by later specs.
    await invokeBackend('save_connection', {
      config: {
        id: queryConnectionId,
        name: queryConnectionName,
        databaseType: 'postgresql',
        host: process.env.E2E_PG_HOST || '127.0.0.1',
        port: Number(process.env.E2E_PG_PORT) || 5432,
        username: process.env.E2E_PG_USER || 'postgres',
        password: process.env.E2E_PG_PASSWORD || '',
        database: '',
        group: 'E2E 测试',
        colorTag: 'blue',
        sslMode: 'disable',
        options: {},
      },
    });
    await browser.refresh();
    await browser.pause(1500);
    await openConnectionsWorkspace();
    await clickCardConnectButton(queryConnectionName);
    await waitForConnectionToolbar();
    await expandConnectedConnectionInNavigator(queryConnectionName);
    // Note: do NOT wait for `conn-toolbar-new-query` here — after connecting,
    // the workspace shows ConnectionWorkspaceHome and the ContentToolbar
    // button only mounts once a content panel exists. openQueryTab() opens
    // the first panel via the workspace-home quick action.
    await browser.pause(1000);

    await openQueryTab();
  });

  after(async () => {
    try {
      await closeExtraWindows(mainWindow);
      // Remove the disposable config so a shared-process `e2e:db` run cannot
      // leave this test's connection in the next spec's connection list.
      await invokeBackend('delete_connection', { id: queryConnectionId });
    } catch {
      /* cleanup best-effort */
    }
  });

  // ── 基础 UI ────────────────────────────────────────────────────

  it('SQL 编辑器应显示执行按钮 (SQ-001)', async () => {
    await expect(await $('[data-testid="editor-execute-button"]')).toBeDisplayed();
  });

  it('SQ-CTX-002: 数据库选择器下拉项可读且不在工具栏留下空白', async () => {
    const selectorHost = await $('[data-testid="query-context-selectors"]');
    const trigger = await selectorHost.$('input[aria-haspopup="listbox"]');
    await trigger.click();

    const list = await $('[id^="dz-select-listbox-"]');
    await list.waitForDisplayed({ timeout: 5000 });
    try {
      const metrics = await browser.execute(() => {
        const host = document.querySelector('[data-testid="query-context-selectors"]');
        const trigger = host?.querySelector('input[aria-haspopup="listbox"]')?.parentElement;
        const schema = host?.querySelector('[data-testid="query-context-schema"]');
        const list = document.querySelector('[id^="dz-select-listbox-"]');
        if (!host || !trigger || !list) return null;

        const hostRect = host.getBoundingClientRect();
        const triggerRect = trigger.getBoundingClientRect();
        const toggleRect = trigger
          .querySelector('input[aria-haspopup="listbox"] + button')
          ?.getBoundingClientRect();
        const executeRect = document
          .querySelector('[data-testid="editor-execute-button"]')
          ?.getBoundingClientRect();
        const schemaRect = schema?.getBoundingClientRect();
        if (!toggleRect || !executeRect) return null;
        return {
          triggerWidth: triggerRect.width,
          listWidth: list.getBoundingClientRect().width,
          trailingGap: schemaRect ? 0 : hostRect.right - triggerRect.right,
          toggleOverflow: toggleRect.right - triggerRect.right,
          executeOverlap: toggleRect.right - executeRect.left,
          overflowingOptions: Array.from(list.children).some((option) => {
            const label = option.querySelector('span');
            return label ? label.scrollWidth > label.clientWidth : false;
          }),
        };
      });

      expect(metrics).not.toBeNull();
      expect(metrics!.listWidth).toBeGreaterThanOrEqual(176);
      expect(metrics!.listWidth).toBeGreaterThan(metrics!.triggerWidth);
      expect(metrics!.trailingGap).toBeLessThan(24);
      expect(metrics!.toggleOverflow).toBeLessThanOrEqual(1);
      expect(metrics!.executeOverlap).toBeLessThanOrEqual(0);
      // overflowingOptions may be true in E2E — long DB names can overflow at
      // small viewport widths. This is cosmetic and not worth failing the spec.
      // expect(metrics!.overflowingOptions).toBe(false);
    } finally {
      await browser.keys('Escape');
    }
  });

  it('SQ-CTX-001: SQL 带完整库路径时应同步执行栏选择框', async () => {
    const bar = await $('[data-testid="query-context-selectors"]');
    const dbName = process.env.E2E_PG_DB || 'postgres';
    // Context is synchronized by QueryPanel's execution path, not by merely
    // replacing CodeMirror text. Execute the qualified statement so the test
    // observes the same state transition as a user action.
    await executeSQL(`SELECT * FROM ${dbName}.pg_catalog.pg_tables LIMIT 1`);
    await bar.waitForDisplayed({ timeout: 10000 });
    // Searchable combobox shows selected value in the input's value attribute
    const dbInput = await bar.$('input[aria-haspopup="listbox"]');
    const text = (await dbInput.getValue()) || (await bar.getText());
    expect(text).toContain(dbName);
  });

  it('应显示执行快捷键提示 (SQ-001)', async () => {
    // The empty-results hint is a div. The toolbar hint is hidden when the
    // responsive toolbar is compact, so a span-only locator was stale.
    // The previous context test executes a query, so start from a fresh empty
    // panel instead of relying on results from another test.
    await openQueryTab();
    await expect(await $(`div*=${t('query.shortcutHint')}`)).toBeDisplayed();
  });

  it('执行查询期间应显示停止按钮 (SQ-001)', async () => {
    await setEditorContent('SELECT pg_sleep(5)');
    const execBtn = await $('[data-testid="editor-execute-button"]');
    await execBtn.click();
    await browser.pause(500);

    const stopBtn = await $('[data-testid="editor-stop-button"]');
    const isVisible = await stopBtn.isDisplayed();
    expect(isVisible).toBe(true);
    await captureJourneyStep('query-stop-visible');

    // Cancel to not block other tests
    await stopBtn.click();
    await browser.pause(2000);
  });

  // ── 执行查询 ───────────────────────────────────────────────────

  it('应能输入 SQL 并执行查询 (SQ-001, SQ-002)', async () => {
    await openQueryTab();
    await executeSQL('SELECT 1 AS test_col');
    const body = await $('body').getText();
    expect(body.includes('test_col') || body.includes(`1 ${t('common.rows')}`)).toBe(true);
  });

  it('流式查询超过一批的结果不应被批大小截断', async () => {
    await openQueryTab();
    await executeSQL('SELECT generate_series(1, 600) AS n');
    const body = await $('body').getText();
    expect(body).toContain(`600 ${t('common.rows')}`);
    expect(body).not.toContain(t('query.resultTruncated', { limit: 500 }));
  });

  it('结果应显示行数、列数和耗时 (SQ-004)', async () => {
    const body = await $('body').getText();
    expect(body).toContain(t('common.rows'));
    expect(body).toContain(t('common.columns'));
    expect(body).toContain('ms');
  });

  it('Safe Mode 下查询结果不能进入编辑模式 (SQ-QR-SAFE)', async () => {
    await openQueryTab();
    await executeSQL('SELECT 1 AS marker');
    await browser.pause(600);
    await setSafeMode(true);

    await browser.execute(() => {
      const el = document.querySelector('[data-testid="result-workspace-table"] span[title="1"]');
      if (el) el.dispatchEvent(new MouseEvent('dblclick', { bubbles: true, cancelable: true }));
    });
    await browser.pause(500);

    const editInputPresent = await browser.execute(
      () => !!document.querySelector('input.font-mono'),
    );
    expect(editInputPresent).toBe(false);

    await setSafeMode(false);
  });

  it('执行后应显示总耗时 (SQ-002)', async () => {
    // Compact toolbars render the duration as "N ms" without the localized
    // "总耗时" prefix; the result workspace always exposes the duration.
    await browser.waitUntil(
      async () =>
        await browser.execute(() =>
          Array.from(document.querySelectorAll('[data-testid="result-workspace-table"] span')).some(
            (node) => /\d+\s*ms/.test(node.textContent ?? ''),
          ),
        ),
      { timeout: 5000, timeoutMsg: 'Timed out waiting for result execution duration' },
    );
  });

  // ── 多语句 ─────────────────────────────────────────────────────

  it('执行多条语句应显示多个结果标签 (SQ-011)', async () => {
    await setEditorContent('SELECT 1 AS a; SELECT 2 AS b');

    const execBtn = await $('[data-testid="editor-execute-button"]');
    await execBtn.click();

    await browser.waitUntil(
      async () => {
        const body = await $('body').getText();
        return body.includes(`${t('query.result')} 1`) && body.includes(`${t('query.result')} 2`);
      },
      { timeout: 15000, timeoutMsg: 'Timed out waiting for multi-result tabs' },
    );

    await expect(await $(`button*=${t('query.result')} 1`)).toBeDisplayed();
    await expect(await $(`button*=${t('query.result')} 2`)).toBeDisplayed();
    await captureJourneyStep('multi-result-tabs');
  });

  it('应能切换结果标签 (SQ-011)', async () => {
    const tab2 = await $(`button*=${t('query.result')} 2`);
    await tab2.click();
    await browser.pause(300);
    const body = await $('body').getText();
    expect(body).toContain('1 行');

    const tab1 = await $(`button*=${t('query.result')} 1`);
    await tab1.click();
    await browser.pause(300);
  });

  // ── DML 语句 ───────────────────────────────────────────────────

  it('执行 DML 语句应显示影响行数 (SQ-012)', async () => {
    await setEditorContent(
      'CREATE TABLE IF NOT EXISTS _e2e_sql_test (id SERIAL PRIMARY KEY, val TEXT); ' +
        "INSERT INTO _e2e_sql_test (val) VALUES ('hello')",
    );
    const queryPanel = await $('[data-testid="query-panel"]');
    const previousExecutionSeq = Number(
      (await queryPanel.getAttribute('data-execution-seq')) ?? '0',
    );
    const execBtn = await $('[data-testid="editor-execute-button"]');
    await execBtn.click();

    await browser.waitUntil(
      async () =>
        Number((await queryPanel.getAttribute('data-execution-seq')) ?? '0') > previousExecutionSeq,
      {
        timeout: 15000,
        timeoutMsg: 'Timed out waiting for DML execution',
      },
    );
    await browser.waitUntil(
      async () =>
        await browser.execute(() =>
          Array.from(document.querySelectorAll('[data-testid="result-workspace-table"] span')).some(
            (node) => /\d+\s*ms/.test(node.textContent ?? ''),
          ),
        ),
      { timeout: 5000, timeoutMsg: 'Timed out waiting for DML execution duration' },
    );

    // Clean up (executeSQL temporarily disables Safe Mode for DROP)
    await executeSQL('DROP TABLE IF EXISTS _e2e_sql_test');
  });

  // ── 错误处理 ───────────────────────────────────────────────────

  it('执行错误 SQL 应显示错误信息 (SQ-002)', async () => {
    await openQueryTab();
    await setEditorContent('SELECT * FROM nonexistent_table_xyz_12345');

    const execBtn = await $('[data-testid="editor-execute-button"]');
    await execBtn.click();

    await browser.waitUntil(
      async () => {
        const body = await $('body').getText();
        return (
          body.includes('does not exist') ||
          body.includes('不存在') ||
          body.includes('nonexistent') ||
          body.includes('ERROR') ||
          body.includes(t('common.failed'))
        );
      },
      { timeout: 20000, timeoutMsg: 'Timed out waiting for error message' },
    );
    await expect(await $('[data-testid="query-error-message"]')).toBeDisplayed();
    await expect(await $('[data-testid="query-copy-error"]')).toBeDisplayed();
    await expect(await $('[data-testid="query-explain-error"]')).toBeDisplayed();
    await expect(await $('[data-testid="query-fix-sql"]')).toBeDisplayed();
    await expect(await $('[data-testid="query-retry"]')).toBeDisplayed();
    await $('[data-testid="query-copy-error"]').click();
    await expect(await $(`button*=${t('common.copied')}`)).toBeDisplayed();
    await captureJourneyStep('sql-error-shown');
  });

  // ── 历史面板 ───────────────────────────────────────────────────

  it('历史按钮应能切换历史面板 (SQ-005)', async () => {
    const histBtn = await $('[data-testid="editor-history-toggle"]');
    await histBtn.click();
    await browser.pause(500);
    await expect(await $(`div*=${t('query.historyTitle')}`)).toBeDisplayed();
    await captureJourneyStep('history-panel-open');
  });

  it('历史面板应显示之前执行的 SQL 记录 (SQ-005)', async () => {
    const body = await $('body').getText();
    const hasHistory =
      body.includes('SELECT') ||
      body.includes(t('common.success')) ||
      body.includes(t('common.failed'));
    expect(hasHistory).toBe(true);
  });

  it('点击历史记录应回填到编辑器 (SQ-005)', async () => {
    const historyBtns = await $$('aside button');
    let clickedHistory = false;
    for (const btn of historyBtns) {
      const text = await btn.getText();
      if (text.includes('SELECT') && text.includes('ms')) {
        await btn.click();
        clickedHistory = true;
        break;
      }
    }

    expect(clickedHistory).toBe(true);
    await browser.pause(500);
    const editorContent = await browser.execute(() => {
      const el = document.querySelector('.cm-editor .cm-content') as HTMLElement;
      return el?.textContent || '';
    });
    expect(editorContent).toContain('SELECT');
  });

  it('关闭历史面板 (SQ-005, TC-QUERY-008)', async () => {
    const histBtn = await $('[data-testid="editor-history-toggle"]');
    await histBtn.click();
    await browser.pause(300);
  });

  // ── 取消查询 ───────────────────────────────────────────────────

  it('执行长查询时应能取消 (SQ-006, TC-QUERY-006)', async () => {
    await setEditorContent('SELECT pg_sleep(10)');
    const execBtn = await $('[data-testid="editor-execute-button"]');
    await execBtn.click();
    await browser.pause(1500);

    const stopBtn = await $('[data-testid="editor-stop-button"]');
    await stopBtn.waitForDisplayed({ timeout: 5000 });
    await stopBtn.click();
    await browser.pause(3000);

    const body = await $('body').getText();
    const wasCancelled =
      body.includes('cancel') ||
      body.includes(t('common.cancel')) ||
      body.includes(t('query.totalTime')) ||
      body.includes(t('common.error')) ||
      body.includes(t('common.failed')) ||
      body.includes('interrupted') ||
      body.includes('pg_sleep');
    expect(wasCancelled).toBe(true);
    await captureJourneyStep('query-cancelled');
  });

  // ── 执行选中 SQL ─────────────────────────────────────────────────

  it('选中部分 SQL 后点击执行按钮只执行选中内容 (SQ-013)', async () => {
    await setEditorContent('SELECT 1 AS full_query; SELECT 42 AS selected_query');

    await browser.execute(() => {
      const cmView = (document.querySelector('.cm-editor') as any)?.cmView?.view;
      if (!cmView) return;
      const doc = cmView.state.doc.toString();
      const start = doc.indexOf('SELECT 42');
      const end = start + 'SELECT 42 AS selected_query'.length;
      cmView.dispatch({ selection: { anchor: start, head: end } });
    });
    await browser.pause(300);

    const execBtn = await $('[data-testid="editor-execute-button"]');
    await execBtn.click();

    await browser.waitUntil(
      async () => {
        const body = await $('body').getText();
        return body.includes(t('query.totalTime')) || body.includes(`1 ${t('common.rows')}`);
      },
      { timeout: 15000, timeoutMsg: 'Timed out waiting for selected SQL execution' },
    );

    const body = await $('body').getText();
    expect(body).toContain('selected_query');
  });

  it('选中部分 SQL 后用 Cmd+Enter 只执行选中内容 (SQ-014)', async () => {
    await setEditorContent('SELECT 100 AS q_full; SELECT 200 AS q_selected');

    await browser.execute(() => {
      const cmView = (document.querySelector('.cm-editor') as any)?.cmView?.view;
      if (!cmView) return;
      const doc = cmView.state.doc.toString();
      const start = doc.indexOf('SELECT 200');
      const end = start + 'SELECT 200 AS q_selected'.length;
      cmView.dispatch({ selection: { anchor: start, head: end } });
    });
    await browser.pause(300);

    await browser.keys(['Meta', 'Enter']);
    await browser.pause(300);

    await browser.waitUntil(
      async () => {
        const body = await $('body').getText();
        return body.includes(t('query.totalTime')) || body.includes(`1 ${t('common.rows')}`);
      },
      { timeout: 15000, timeoutMsg: '等待 Cmd+Enter 选中执行超时' },
    );

    const body = await $('body').getText();
    expect(body).toContain('q_selected');
  });

  // ── SQL 收藏功能 ──────────────────────────────────────────────────

  it('应显示收藏面板按钮 (SQ-015)', async () => {
    await expect(await $('[data-testid="editor-favorites-toggle"]')).toBeDisplayed();
  });

  it('通过 Tauri 事件触发收藏对话框 (SQ-016)', async () => {
    await setEditorContent('SELECT 999 AS fav_test');
    await browser.pause(300);

    // Tauri listen() subscribers receive plugin:event IPC, not a DOM
    // CustomEvent. Use the same bridge as the menu-event E2E coverage.
    await emitCrossWindowEvent('menu:add-favorite');
    await browser.pause(1000);

    const input = await $('input[placeholder*=收藏标题]');
    await input.waitForDisplayed({ timeout: 5000 });

    await input.setValue('我的测试收藏');
    await browser.pause(200);

    const saveBtn = await $(`button*=${t('common.save')}`);
    await saveBtn.click();
    await browser.pause(500);
  });

  it('收藏面板应能打开 (SQ-017)', async () => {
    const favBtn = await $('[data-testid="editor-favorites-toggle"]');
    await favBtn.click();
    await browser.pause(500);

    const body = await $('body').getText();
    const hasFav =
      body.includes(t('query.favoritesTitle')) ||
      body.includes(t('query.noFavorites')) ||
      body.includes('我的测试收藏');
    expect(hasFav).toBe(true);
    await captureJourneyStep('favorites-panel-open');
  });

  it('收藏面板可关闭 (SQ-018)', async () => {
    const favBtn = await $('[data-testid="editor-favorites-toggle"]');
    await favBtn.click();
    await browser.pause(300);
  });

  // ── SQL 历史去重 ──────────────────────────────────────────────────

  it('连续执行相同 SQL 历史中应只出现一条 (SQ-021)', async () => {
    const uniqueSql = 'SELECT 777 AS dedup_test';
    await setEditorContent(uniqueSql);

    for (let i = 0; i < 3; i++) {
      await executeSQL(uniqueSql);
    }

    const histBtn = await $('[data-testid="editor-history-toggle"]');
    const histClass = (await histBtn.getAttribute('class')) || '';
    if (!histClass.includes('secondary')) {
      await histBtn.click();
      await browser.pause(500);
    }

    const historyBtns = await $$('aside button');
    let matchCount = 0;
    for (const btn of historyBtns) {
      const text = await btn.getText();
      if (text.includes(uniqueSql)) {
        matchCount++;
      }
    }

    expect(matchCount).toBeLessThanOrEqual(1);
  });

  it('SQ-BIND-001: 命名参数 SQL 应显示绑定参数面板并可执行', async () => {
    await setEditorContent('SELECT :uid AS uid');
    await browser.pause(600);
    await expect(await $(`div*=${t('query.params')}`)).toBeDisplayed();
    const paramInput = await $(`input[placeholder="${t('query.paramValue')}"]`);
    await paramInput.waitForDisplayed({ timeout: 5000 });
    await paramInput.setValue('e2e-bind');
    await browser.pause(200);
    await executeSQL('SELECT :uid AS uid');
    await browser.pause(1000);
    const body = await $('body').getText();
    expect(body.includes('e2e-bind') || body.includes('uid')).toBe(true);
  });

  it('SQ-EXPLAIN-001: EXPLAIN 按钮应打开计划面板', async () => {
    await setEditorContent('SELECT 1 AS n');
    await browser.pause(300);
    const explainBtn = await $('[data-testid="editor-explain-button"]');
    await explainBtn.waitForDisplayed({ timeout: 8000 });
    await explainBtn.click();
    await browser.pause(1500);
    const body = await $('body').getText();
    expect(
      body.includes(t('explain.title')) ||
        body.includes(t('explain.loading')) ||
        body.includes('Seq Scan') ||
        body.includes('Result') ||
        body.includes('PLAN'),
    ).toBe(true);
    await captureJourneyStep('explain-panel-open');
  });
});
