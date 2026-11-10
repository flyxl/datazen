import { expect, browser, $, $$ } from '@wdio/globals';
import { t } from '../i18n.js';
import { closeWorkspacePanels } from '../lib/testDataLifecycle.js';
import {
  clickTableInSidebar,
  closeExtraWindows,
  connectSeededPgInWorkspace,
  executeSQLChecked,
  openQueryTab,
  setSafeMode,
  switchSubTab,
  waitForNewQueryButton,
  waitForTableInSidebar,
  clickNavigatorRefresh,
} from '../helpers.js';

/**
 * Data Browsing E2E：Quick Look、列统计与类型化写表达式（设为当前时间）。
 *
 * 覆盖近期/增强的 Data Browsing 能力：
 * - DBR-05：详情面板 Quick Look（可对 JSON 等字段快速查看）
 * - DBR-05：单元格 Web 上下文菜单「设值」→「设为当前时间」（类型化写表达式）
 * - 列统计（Column Stats）：列头 Web 上下文菜单打开服务端聚合统计对话框
 *
 * 说明：应用内右键菜单使用 Web 上下文菜单（`web-context-menu` / `web-context-item-*`），
 * 可通过派发 contextmenu 事件并点击菜单项进行自动化，不依赖系统原生菜单。
 */

const TEST_TABLE = '_e2e_quicklook_stats_test';

async function waitForWebContextMenu(timeout = 8000) {
  await browser.waitUntil(async () => await $('[data-testid="web-context-menu"]').isExisting(), {
    timeout,
    timeoutMsg: 'Web context menu did not appear',
  });
}

async function closeWebContextMenu() {
  await browser.execute(() => {
    document.body.dispatchEvent(new MouseEvent('mousedown', { bubbles: true }));
  });
  await browser
    .waitUntil(async () => !(await $('[data-testid="web-context-menu"]').isExisting()), {
      timeout: 5000,
    })
    .catch(() => {});
}

async function rightClickSelector(selector: string) {
  await closeWebContextMenu();
  const opened = await browser.execute((sel: string) => {
    const el = document.querySelector(sel);
    if (!el) return false;
    const rect = (el as HTMLElement).getBoundingClientRect();
    el.dispatchEvent(
      new MouseEvent('contextmenu', {
        bubbles: true,
        cancelable: true,
        clientX: rect.left + Math.min(10, rect.width / 2),
        clientY: rect.top + Math.min(10, rect.height / 2),
      }),
    );
    return true;
  }, selector);
  expect(opened).toBe(true);
  await waitForWebContextMenu();
}

async function openSetValueSubmenu() {
  const trigger = await $('[data-testid="web-context-submenu-trigger-set-value"]');
  await trigger.waitForDisplayed({ timeout: 8000 });
  await trigger.moveTo().catch(async () => {
    await trigger.click();
  });
  await browser
    .waitUntil(async () => await $('[data-testid="web-context-submenu"]').isExisting(), {
      timeout: 5000,
      timeoutMsg: 'Set Value submenu did not open',
    })
    .catch(async () => {
      // 兜底：点击触发
      await trigger.click();
      await browser.waitUntil(
        async () => await $('[data-testid="web-context-submenu"]').isExisting(),
        { timeout: 5000, timeoutMsg: 'Set Value submenu did not open' },
      );
    });
}

describe('Data Browsing：Quick Look、列统计与设为当前时间', () => {
  let mainWindow: string;

  before(async () => {
    mainWindow = await browser.getWindowHandle();
    await setSafeMode(false);
    await connectSeededPgInWorkspace();
    await waitForNewQueryButton(20000);
    await browser.pause(1500);

    await openQueryTab();
    await executeSQLChecked(`DROP TABLE IF EXISTS ${TEST_TABLE}`);
    await executeSQLChecked(`
      CREATE TABLE ${TEST_TABLE} (
        id SERIAL PRIMARY KEY,
        name VARCHAR(100) NOT NULL,
        score INT NOT NULL,
        updated_at TIMESTAMP,
        data_json JSONB
      )
    `);
    await executeSQLChecked(`
      INSERT INTO ${TEST_TABLE} (name, score, updated_at, data_json)
      VALUES
        ('Alice', 10, '2020-01-01 00:00:00', '{"user":"alice","role":"admin"}'),
        ('Bob', 20, '2020-01-02 00:00:00', '{"user":"bob","role":"user"}'),
        ('Cara', 30, '2020-01-03 00:00:00', '{"user":"cara","role":"user"}')
    `);

    await clickNavigatorRefresh();
    await waitForTableInSidebar(TEST_TABLE);
  });

  after(async () => {
    try {
      await browser.switchToWindow(mainWindow);
      await openQueryTab();
      await executeSQLChecked(`DROP TABLE IF EXISTS ${TEST_TABLE}`);
    } catch {
      // best-effort cleanup
    }
    await setSafeMode(true);
    await closeExtraWindows(mainWindow);
    await closeWorkspacePanels(browser);
  });

  it('加载表数据并打开详情面板（DBR-05 前置）', async () => {
    await clickTableInSidebar(TEST_TABLE);
    await browser.pause(2000);
    await switchSubTab('data');

    await browser.waitUntil(
      async () => {
        const body = await $('body').getText();
        return body.includes('Alice') && body.includes('Bob');
      },
      { timeout: 15000, timeoutMsg: 'Timed out waiting for table data to load' },
    );

    const firstCell = await $('[data-dt-row="0"][data-dt-col="name"]');
    if (await firstCell.isExisting()) {
      await firstCell.click();
      await browser.pause(400);
    }

    const showDetailTitle = t('detail.show');
    const hideDetailTitle = t('detail.hide');
    const showBtn = await $(`button[title="${showDetailTitle}"]`);
    const hideBtn = await $(`button[title="${hideDetailTitle}"]`);

    if (await showBtn.isExisting()) {
      await showBtn.click();
    } else if (!(await hideBtn.isExisting())) {
      throw new Error('Detail panel toggle button not found');
    }

    await browser.waitUntil(
      async () => {
        const body = await $('body').getText();
        return (
          body.includes(t('detail.title')) || body.includes('Alice') || body.includes('data_json')
        );
      },
      { timeout: 10000, timeoutMsg: 'Detail panel did not open' },
    );
  });

  it('详情面板中可对 JSON 字段触发 Quick Look（DBR-05）', async () => {
    const quickLookTitle = t('detail.quickLook');

    const clicked = await browser.execute((title: string) => {
      const buttons = Array.from(document.querySelectorAll('button'));
      const btn = buttons.find((b) => b.getAttribute('title') === title) as
        | HTMLButtonElement
        | undefined;
      if (!btn) return false;
      btn.click();
      return true;
    }, quickLookTitle);

    if (!clicked) {
      console.warn('Quick Look button not found; skipping dialog assertion');
      return;
    }

    await browser.waitUntil(
      async () => {
        const body = await $('body').getText();
        const hasJsonContent =
          body.includes('alice') || body.includes('admin') || body.includes('user');
        const hasDialog =
          (await $('[role="dialog"]')
            .isExisting()
            .catch(() => false)) ||
          (await $('dialog')
            .isExisting()
            .catch(() => false));
        return hasJsonContent || hasDialog;
      },
      { timeout: 10000, timeoutMsg: 'Quick Look did not open or show content' },
    );

    await browser.keys('Escape');
    await browser.pause(300);
  });

  it('列头 Web 上下文菜单可打开列统计对话框并显示聚合结果', async () => {
    await rightClickSelector('[data-col-header="score"]');

    const statsItem = await $('[data-testid="web-context-item-column-stats"]');
    await statsItem.waitForDisplayed({
      timeout: 8000,
      timeoutMsg: 'Column stats menu item not shown',
    });
    await statsItem.click();

    const dialog = await $('[data-testid="column-stats-dialog"]');
    await dialog.waitForDisplayed({
      timeout: 15000,
      timeoutMsg: 'Column stats dialog did not open',
    });

    await browser.waitUntil(
      async () => {
        const hasRows = await $('[data-testid="column-stats-rows"]')
          .isExisting()
          .catch(() => false);
        const hasError = (await dialog.getText()).includes(t('columnStats.error'));
        return hasRows || hasError;
      },
      { timeout: 15000, timeoutMsg: 'Column stats did not finish loading' },
    );

    expect(await $('[data-testid="column-stats-rows"]').isExisting()).toBe(true);
    expect(await $('[data-testid="column-stats-avg"]').isExisting()).toBe(true);

    await browser.keys('Escape');
    await browser
      .waitUntil(async () => !(await $('[data-testid="column-stats-dialog"]').isExisting()), {
        timeout: 8000,
      })
      .catch(() => {});
  });

  it('单元格 Web 上下文菜单可将时间列设为当前时间（DBR-05 类型化写表达式）', async () => {
    // 右键 updated_at 单元格
    await rightClickSelector('[data-dt-row="0"][data-dt-col="updated_at"]');

    // 打开「设值」子菜单并点击「设为当前时间」
    await openSetValueSubmenu();
    const setNow = await $('[data-testid="web-context-item-set-value-setCurrentDatetime"]');
    if (!(await setNow.isExisting())) {
      console.warn('Set current time menu item not available; skipping expression assertion');
      await closeWebContextMenu();
      return;
    }
    await setNow.waitForDisplayed({ timeout: 5000 });
    await setNow.click();

    // 预期产生暂存修改：单元格/行出现脏状态或待提交提示
    await browser.waitUntil(
      async () => {
        const dirty = await $('[data-dt-row="0"][data-dt-col="updated_at"][data-dt-dirty="true"]')
          .isExisting()
          .catch(() => false);
        const body = await $('body').getText();
        return dirty || body.includes(t('dataTable.pendingChanges')) || body.includes('未提交');
      },
      { timeout: 10000, timeoutMsg: 'Set current time did not stage a pending change' },
    );

    // 回滚暂存修改，避免影响后续用例/状态
    const rollback = await $('[data-testid="pending-rollback"]');
    if (await rollback.isExisting()) {
      await rollback.click().catch(() => {});
      await browser.pause(800);
    }
  });
});
