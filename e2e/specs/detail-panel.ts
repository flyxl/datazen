import { expect, browser, $ } from '@wdio/globals';
import { t } from '../i18n.js';
import {
  clickCardConnectButton,
  closeExtraWindows,
  executeSQL,
  openQueryTab,
  clickTableInSidebar,
  switchSubTab,
} from '../helpers.js';

/**
 * Detail panel E2E tests.
 *
 * DP-001: Detail panel renders at window level (not inside DataTable border)
 * DP-002: Clicking a row shows its fields in the detail panel
 * DP-003: Editing a field in the detail panel updates the database
 * DP-004: Updated value is immediately visible in the data grid
 */

const TEST_TABLE = '_e2e_detail_panel';

describe('详情面板 (DP-001~DP-004)', () => {
  let mainWindow: string;

  before(async () => {
    mainWindow = await browser.getWindowHandle();
    await $(`button*=${t('action.newConnection')}`).waitForDisplayed({ timeout: 10000 });
    await browser.pause(1500);

    let handles = await browser.getWindowHandles();
    if (handles.length === 1) {
      await clickCardConnectButton();
      await browser.waitUntil(
        async () => (await browser.getWindowHandles()).length > 1,
        { timeout: 30000, timeoutMsg: 'Timed out waiting for connection window' },
      );
      handles = await browser.getWindowHandles();
    }
    const connWindow = handles.find((h) => h !== mainWindow)!;
    await browser.switchToWindow(connWindow);
    await $(`button*=${t('connWin.newQuery')}`).waitForDisplayed({ timeout: 20000 });
    await browser.pause(2000);

    await openQueryTab();
    await executeSQL(`DROP TABLE IF EXISTS ${TEST_TABLE}`);
    await executeSQL(`
      CREATE TABLE ${TEST_TABLE} (
        id SERIAL PRIMARY KEY,
        name VARCHAR(100) NOT NULL,
        score INT NOT NULL DEFAULT 0,
        metadata JSONB DEFAULT '{}'::jsonb
      )
    `);
    await executeSQL(`
      INSERT INTO ${TEST_TABLE} (name, score, metadata) VALUES
        ('Alice', 100, '{"role":"admin"}'),
        ('Bob', 200, '{"role":"user"}')
    `);

    const refreshBtn = await $(`button[title="${t('connWin.refresh')} (⌘R)"]`);
    await refreshBtn.click();
    await browser.pause(2000);
  });

  after(async () => {
    try {
      const handles = await browser.getWindowHandles();
      if (handles.length > 1) {
        const connWindow = handles.find((h) => h !== mainWindow);
        if (connWindow) {
          await browser.switchToWindow(connWindow);
          await openQueryTab();
          await executeSQL(`DROP TABLE IF EXISTS ${TEST_TABLE}`);
        }
      }
    } catch {
      // best-effort cleanup
    }
    await closeExtraWindows(mainWindow);
  });

  it('打开表数据后应看到详情面板的折叠按钮 (DP-001)', async () => {
    await clickTableInSidebar(TEST_TABLE);
    await browser.pause(2000);
    await switchSubTab(t('connWin.data'));

    await browser.waitUntil(
      async () => (await $('body').getText()).includes('Alice'),
      { timeout: 15000, timeoutMsg: 'Timed out waiting for table data to load' },
    );

    // The detail panel toggle button should exist at the window level
    const toggleBtn = await $(`button[title="${t('detail.show')}"]`);
    await expect(toggleBtn).toBeExisting();
  });

  it('详情面板应在窗口右侧而不是表格内部 (DP-001)', async () => {
    // The toggle button should be a sibling of the main content area,
    // NOT inside the DataTable's rounded border container.
    const isWindowLevel = await browser.execute((showTitle) => {
      const btn = document.querySelector(`button[title="${showTitle}"]`);
      if (!btn) return false;
      // Walk up the DOM — the detail panel should NOT be inside
      // a container with rounded border (DataTable's wrapper).
      let parent = btn.parentElement;
      while (parent) {
        const cl = parent.className || '';
        if (cl.includes('rounded-md') && cl.includes('border-edge')) {
          return false;
        }
        parent = parent.parentElement;
      }
      return true;
    }, t('detail.show'));
    expect(isWindowLevel).toBe(true);
  });

  it('点击行后展开详情面板应显示该行各字段 (DP-002)', async () => {
    // Click a row to select it
    await browser.execute(() => {
      const span = document.querySelector('span[title="Alice"]');
      if (!span) return;
      const row = span.closest('[tabindex="0"]');
      if (row) row.dispatchEvent(new MouseEvent('click', { bubbles: true }));
    });
    await browser.pause(300);

    // Open the detail panel
    const toggleBtn = await $(`button[title="${t('detail.show')}"]`);
    await toggleBtn.click();
    await browser.pause(500);

    // The detail panel should now show the "详情" header
    const body = await $('body').getText();
    expect(body).toContain(t('detail.title'));

    // Should show field names from the table
    expect(body).toContain('name');
    expect(body).toContain('score');
  });

  it('详情面板中的字段应可编辑 (DP-003)', async () => {
    // Find the input/textarea for the "name" field in the detail panel
    // and change Alice to AliceEdited.
    const edited = await browser.execute(() => {
      // Detail panel is the right-hand aside (w-72), not the schema tree.
      const asides = Array.from(document.querySelectorAll('aside'));
      const panel = asides.find((el) => el.className.includes('w-72')) ?? asides[asides.length - 1];
      if (!panel) return false;
      const inputs = panel.querySelectorAll('input, textarea');
      for (const input of inputs) {
        if ((input as HTMLInputElement).value === 'Alice') {
          const el = input as HTMLInputElement;
          el.focus();
          const setter = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value')?.set;
          setter?.call(el, 'AliceEdited');
          el.dispatchEvent(new Event('input', { bubbles: true }));
          el.dispatchEvent(new Event('change', { bubbles: true }));
          el.dispatchEvent(new Event('blur', { bubbles: true }));
          return true;
        }
      }
      return false;
    });
    expect(edited).toBe(true);
    await browser.pause(2000);
  });

  it('编辑后的值应持久化到数据库 (DP-004)', async () => {
    await openQueryTab();
    await executeSQL(`SELECT name FROM ${TEST_TABLE} WHERE id = 1`);
    await browser.pause(1000);

    const body = await $('body').getText();
    expect(body).toContain('AliceEdited');
  });
});
