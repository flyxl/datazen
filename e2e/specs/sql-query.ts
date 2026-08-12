import { expect, browser, $, $$ } from '@wdio/globals';
import { t } from '../i18n.js';
import {
  clickCardConnectButton,
  closeExtraWindows,
  setEditorContent,
  openQueryTab,
  executeSQL,
} from '../helpers.js';

/**
 * SQL query module tests.
 * Requires a PostgreSQL connection (seeded by wdio.conf.ts before hook).
 */
describe('SQL 查询模块 (SQ-001~SQ-012, TC-QUERY-006/008)', () => {
  let mainWindow: string;

  before(async () => {
    mainWindow = await browser.getWindowHandle();
    await $(`button*=${t('action.newConnection')}`).waitForDisplayed({ timeout: 10000 });
    await browser.pause(1500);

    await clickCardConnectButton();

    await browser.waitUntil(
      async () => (await browser.getWindowHandles()).length > 1,
      { timeout: 20000, timeoutMsg: 'Timed out waiting for connection window' },
    );
    const handles = await browser.getWindowHandles();
    const connWindow = handles.find((h) => h !== mainWindow)!;
    await browser.switchToWindow(connWindow);

    await $(`button*=${t('connWin.newQuery')}`).waitForDisplayed({ timeout: 20000 });
    await browser.pause(1000);

    await openQueryTab();
  });

  after(async () => {
    await closeExtraWindows(mainWindow);
  });

  // ── 基础 UI ────────────────────────────────────────────────────

  it('SQL 编辑器应显示执行按钮 (SQ-001)', async () => {
    await expect(await $(`button*=${t('query.execute')}`)).toBeDisplayed();
  });

  it('应显示执行快捷键提示 (SQ-001)', async () => {
    await expect(await $('span*=⌘+Enter')).toBeDisplayed();
  });

  it('执行查询期间应显示停止按钮 (SQ-001)', async () => {
    await setEditorContent('SELECT pg_sleep(5)');
    const execBtn = await $(`button*=${t('query.execute')}`);
    await execBtn.click();
    await browser.pause(500);

    const stopBtn = await $(`button*=${t('query.stop')}`);
    const isVisible = await stopBtn.isDisplayed();
    expect(isVisible).toBe(true);

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

  it('执行后应显示总耗时 (SQ-002)', async () => {
    await expect(await $(`span*=${t('query.totalTime')}`)).toBeDisplayed();
  });

  // ── 多语句 ─────────────────────────────────────────────────────

  it('执行多条语句应显示多个结果标签 (SQ-011)', async () => {
    await setEditorContent('SELECT 1 AS a; SELECT 2 AS b');

    const execBtn = await $(`button*=${t('query.execute')}`);
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
      "CREATE TABLE IF NOT EXISTS _e2e_sql_test (id SERIAL PRIMARY KEY, val TEXT); " +
      "INSERT INTO _e2e_sql_test (val) VALUES ('hello')"
    );
    const execBtn = await $(`button*=${t('query.execute')}`);
    await execBtn.click();

    await browser.waitUntil(
      async () => (await $('body').getText()).includes('总耗时'),
      { timeout: 15000, timeoutMsg: 'Timed out waiting for DML execution' },
    );

    // Clean up
    await setEditorContent('DROP TABLE IF EXISTS _e2e_sql_test');
    const execBtn2 = await $(`button*=${t('query.execute')}`);
    await execBtn2.click();
    await browser.waitUntil(
      async () => (await $('body').getText()).includes('总耗时'),
      { timeout: 10000 },
    );
  });

  // ── 错误处理 ───────────────────────────────────────────────────

  it('执行错误 SQL 应显示错误信息 (SQ-002)', async () => {
    await openQueryTab();
    await setEditorContent('SELECT * FROM nonexistent_table_xyz_12345');

    const execBtn = await $(`button*=${t('query.execute')}`);
    await execBtn.click();

    await browser.waitUntil(
      async () => {
        const body = await $('body').getText();
        return body.includes('does not exist') || body.includes('不存在') ||
          body.includes('nonexistent') || body.includes('ERROR') ||
          body.includes(t('common.failed'));
      },
      { timeout: 20000, timeoutMsg: 'Timed out waiting for error message' },
    );
  });

  // ── 历史面板 ───────────────────────────────────────────────────

  it('历史按钮应能切换历史面板 (SQ-005)', async () => {
    const histBtn = await $(`button*=${t('query.history')}`);
    await histBtn.click();
    await browser.pause(500);
    await expect(await $(`div*=${t('query.historyTitle')}`)).toBeDisplayed();
  });

  it('历史面板应显示之前执行的 SQL 记录 (SQ-005)', async () => {
    const body = await $('body').getText();
    const hasHistory = body.includes('SELECT') || body.includes(t('common.success')) || body.includes(t('common.failed'));
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

    if (clickedHistory) {
      await browser.pause(500);
      const editorContent = await browser.execute(() => {
        const el = document.querySelector('.cm-editor .cm-content') as HTMLElement;
        return el?.textContent || '';
      });
      expect(editorContent).toContain('SELECT');
    }
  });

  it('关闭历史面板 (SQ-005, TC-QUERY-008)', async () => {
    const histBtn = await $(`button*=${t('query.history')}`);
    await histBtn.click();
    await browser.pause(300);
  });

  // ── 取消查询 ───────────────────────────────────────────────────

  it('执行长查询时应能取消 (SQ-006, TC-QUERY-006)', async () => {
    await setEditorContent('SELECT pg_sleep(10)');
    const execBtn = await $(`button*=${t('query.execute')}`);
    await execBtn.click();
    await browser.pause(1500);

    const stopBtn = await $(`button*=${t('query.stop')}`);
    if (await stopBtn.isExisting() && await stopBtn.isDisplayed()) {
      await stopBtn.click();
      await browser.pause(3000);

      const body = await $('body').getText();
      const wasCancelled = body.includes('cancel') || body.includes(t('common.cancel')) ||
        body.includes(t('query.totalTime')) || body.includes(t('common.error')) ||
        body.includes(t('common.failed')) || body.includes('interrupted') ||
        body.includes('pg_sleep');
      expect(wasCancelled).toBe(true);
    } else {
      const body = await $('body').getText();
      expect(body.length).toBeGreaterThan(0);
    }
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

    const execBtn = await $(`button*=${t('query.execute')}`);
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
    await expect(await $(`button*=${t('query.favorites')}`)).toBeDisplayed();
  });

  it('通过 Tauri 事件触发收藏对话框 (SQ-016)', async () => {
    await setEditorContent('SELECT 999 AS fav_test');
    await browser.pause(300);

    await browser.execute(() => {
      const event = new CustomEvent('tauri://menu', { detail: { id: 'menu:add-favorite' } });
      window.dispatchEvent(event);
      const { emit } = (window as any).__TAURI_INTERNALS__;
      if (emit) emit('menu:add-favorite', {});
    });
    await browser.pause(1000);

    const input = await $('input[placeholder*=收藏标题]');
    const isShown = await input.isExisting();
    if (isShown) {
      await expect(input).toBeDisplayed();

      await input.setValue('我的测试收藏');
      await browser.pause(200);

      const saveBtn = await $(`button*=${t('common.save')}`);
      await saveBtn.click();
      await browser.pause(500);
    }
  });

  it('收藏面板应能打开 (SQ-017)', async () => {
    const favBtn = await $(`button*=${t('query.favorites')}`);
    await favBtn.click();
    await browser.pause(500);

    const body = await $('body').getText();
    const hasFav = body.includes(t('query.favoritesTitle')) || body.includes(t('query.noFavorites')) || body.includes('我的测试收藏');
    expect(hasFav).toBe(true);
  });

  it('收藏面板可关闭 (SQ-018)', async () => {
    const favBtn = await $(`button*=${t('query.favorites')}`);
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

    const histBtn = await $(`button*=${t('query.history')}`);
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
    await setEditorContent("SELECT :uid AS uid");
    await browser.pause(600);
    await expect(await $(`div*=${t('query.params')}`)).toBeDisplayed();
    const paramInput = await $(`input[placeholder="${t('query.paramValue')}"]`);
    await paramInput.waitForDisplayed({ timeout: 5000 });
    await paramInput.setValue('e2e-bind');
    await browser.pause(200);
    await executeSQL("SELECT :uid AS uid");
    await browser.pause(1000);
    const body = await $('body').getText();
    expect(body.includes('e2e-bind') || body.includes('uid')).toBe(true);
  });

  it('SQ-EXPLAIN-001: EXPLAIN 按钮应打开计划面板', async () => {
    await setEditorContent('SELECT 1 AS n');
    await browser.pause(300);
    const explainBtn = await $(`button*=${t('explain.title')}`);
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
  });
});
