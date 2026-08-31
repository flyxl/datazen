import { expect, browser, $ } from '@wdio/globals';
import { t } from '../i18n.js';
import {
  closeExtraWindows,
  executeSQL,
  openQueryTab,
  clickTableInSidebar,
  switchSubTab,
  connectSeededPgInWorkspace,
  setSafeMode,
  withSafeModeOff,
  waitForTableInSidebar,
} from '../helpers.js';

/**
 * Indexes tab: create dialog (editable default name), create, delete, editInStructure.
 */

const TEST_TABLE = '_e2e_idx_create';
const INDEX_NAME = 'idx_e2e_idx_create_name';

describe('表索引创建与删除 (IDX-001~IDX-006)', () => {
  let mainWindow: string;

  before(async () => {
    mainWindow = await browser.getWindowHandle();
    await connectSeededPgInWorkspace();

    await openQueryTab();
    await executeSQL(`DROP TABLE IF EXISTS ${TEST_TABLE}`);
    await executeSQL(`
      CREATE TABLE ${TEST_TABLE} (
        id SERIAL PRIMARY KEY,
        name VARCHAR(100) NOT NULL,
        score INT NOT NULL
      )
    `);
    // Let the query panel finish its result-state update before refreshing the
    // navigator; the schema refresh is independent of SQL completion.
    await browser.pause(1200);

    const refreshBtn = await $(`button[title="${t('connWin.refresh')} (⌘R)"]`);
    await refreshBtn.click();
    await waitForTableInSidebar(TEST_TABLE);

    await clickTableInSidebar(TEST_TABLE);
    await browser.pause(1000);
    await switchSubTab('indexes');
    await browser.pause(1000);
  });

  after(async () => {
    try {
      await browser.switchToWindow(mainWindow);
      await openQueryTab();
      await executeSQL(`DROP TABLE IF EXISTS ${TEST_TABLE}`);
    } catch {
      /* cleanup */
    }
    await setSafeMode(true);
    await closeExtraWindows(mainWindow);
  });

  it('新建索引对话框应预填可编辑名称 (IDX-001/002)', async () => {
    const newBtn = await $("[data-testid='idx-new-index']");
    await newBtn.waitForDisplayed({ timeout: 10000 });
    await newBtn.click();
    await browser.pause(500);

    const nameInput = await $('#idx-name');
    await nameInput.waitForDisplayed({ timeout: 5000 });
    const defaultName = await nameInput.getValue();
    expect(defaultName.length).toBeGreaterThan(0);
    expect(defaultName.toLowerCase()).toContain('idx');

    await nameInput.clearValue();
    await nameInput.setValue(INDEX_NAME);
    expect(await nameInput.getValue()).toBe(INDEX_NAME);
  });

  it('勾选列后应显示 SQL 预览 (IDX-003)', async () => {
    // Check name column
    await browser.execute(() => {
      const labels = Array.from(document.querySelectorAll('#idx-cols label'));
      const nameLabel = labels.find((l) => (l.textContent || '').includes('name'));
      const cb = nameLabel?.querySelector('input[type="checkbox"]') as HTMLInputElement | null;
      if (cb && !cb.checked) cb.click();
    });
    await browser.pause(300);
    await expect($("[data-testid='idx-sql-preview']")).toExist();
    const body = await $('body').getText();
    expect(body.toUpperCase()).toContain('CREATE');
    expect(body).toContain(INDEX_NAME);
  });

  it('提交后索引应出现在列表 (IDX-004)', async () => {
    const createBtn = await $(`button*=${t('indexes.createIndex')}`);
    await createBtn.waitForEnabled({ timeout: 5000 });
    await createBtn.click();
    await browser.waitUntil(async () => (await $('body').getText()).includes(INDEX_NAME), {
      timeout: 15000,
      timeoutMsg: '等待新建索引出现在列表',
    });
  });

  it('删除索引应弹出确认并移除 (IDX-005)', async () => {
    await withSafeModeOff(async () => {
      await switchSubTab('indexes');
      await browser.pause(800);
      const indexRow = await $(`[data-index-name="${INDEX_NAME}"]`);
      await indexRow.moveTo();
      const deleteBtn = await $(
        `[data-index-name="${INDEX_NAME}"] [data-testid='idx-delete-index']`,
      );
      // The action is intentionally opacity-0 until the row is hovered. WebKit
      // can keep the CSS hover state stale for virtualized table rows, so wait
      // for the real button and invoke the same React handler from the DOM.
      await deleteBtn.waitForExist({ timeout: 8000 });
      await browser.execute((name: string) => {
        const button = document.querySelector<HTMLButtonElement>(
          `[data-index-name="${name}"] [data-testid='idx-delete-index']`,
        );
        button?.click();
      }, INDEX_NAME);
      await browser.pause(400);
      await expect(await $(`div*=${t('indexes.confirmDeleteTitle')}`)).toBeDisplayed();
      await browser.execute(() => {
        const dialog = document.querySelector('[role="dialog"]');
        const buttons = Array.from(dialog?.querySelectorAll('button') ?? []);
        const confirm = buttons[buttons.length - 1] as HTMLButtonElement | undefined;
        confirm?.click();
      });
      await browser.waitUntil(async () => !(await $('body').getText()).includes(INDEX_NAME), {
        timeout: 15000,
        timeoutMsg: '等待索引从列表移除',
      });
    });
  });

  it('在表结构中编辑应切到结构子标签 (IDX-006)', async () => {
    // Recreate a disposable index so structure navigation still has a target list
    await switchSubTab('indexes');
    await browser.pause(600);
    const newBtn = await $("[data-testid='idx-new-index']");
    await newBtn.click();
    await browser.pause(400);
    const nameInput = await $('#idx-name');
    await nameInput.clearValue();
    await nameInput.setValue('idx_e2e_struct_nav');
    await browser.execute(() => {
      const labels = Array.from(document.querySelectorAll('#idx-cols label'));
      const nameLabel = labels.find((l) => (l.textContent || '').includes('name'));
      const cb = nameLabel?.querySelector('input[type="checkbox"]') as HTMLInputElement | null;
      if (cb && !cb.checked) cb.click();
    });
    await $(`button*=${t('indexes.createIndex')}`).click();
    await browser.waitUntil(
      async () => (await $('body').getText()).includes('idx_e2e_struct_nav'),
      { timeout: 15000 },
    );

    const editBtn = await $("[data-testid='idx-edit-in-structure']");
    await editBtn.waitForDisplayed({ timeout: 8000 });
    await editBtn.click();
    await browser.pause(800);
    await expect($("[data-testid='struct-edit-structure']")).toExist();
    await expect($("[data-testid='sub-tab-structure']")).toExist();
  });
});
