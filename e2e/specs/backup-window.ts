import { expect, browser, $ } from '@wdio/globals';
import { t } from '../i18n.js';
import { closeExtraWindows, switchToNewWindow } from '../helpers.js';

/**
 * Backup window UI paths (BKU-001~BKU-004).
 * Full dump execution remains covered by backup-database.ts (IPC).
 */

describe('备份窗口 UI (BKU-001~BKU-004)', () => {
  let mainWindow: string;

  before(async () => {
    mainWindow = await browser.getWindowHandle();
    await $(`button*=${t('action.newConnection')}`).waitForDisplayed({ timeout: 10000 });
  });

  afterEach(async () => {
    await closeExtraWindows(mainWindow);
    await browser.switchToWindow(mainWindow);
    await browser.pause(300);
  });

  it('主页备份按钮应打开备份窗口 (BKU-001)', async () => {
    const btn = await $(`button*=${t('action.backup')}`);
    await btn.waitForDisplayed({ timeout: 10000 });
    await btn.click();
    await switchToNewWindow(mainWindow);
    await browser.pause(1000);
    const body = await $('body').getText();
    expect(body).toContain(t('backup.title'));
  });

  it('备份窗口应显示连接搜索与文件名控件 (BKU-002)', async () => {
    await $(`button*=${t('action.backup')}`).click();
    await switchToNewWindow(mainWindow);
    await browser.pause(1000);
    await expect(await $(`input[placeholder="${t('backup.searchConnection')}"]`)).toBeDisplayed();
    const body = await $('body').getText();
    expect(body).toContain(t('backup.fileName'));
  });

  it('未选连接时应提示先选择连接 (BKU-003)', async () => {
    await $(`button*=${t('action.backup')}`).click();
    await switchToNewWindow(mainWindow);
    await browser.pause(1000);
    const body = await $('body').getText();
    expect(body).toContain(t('backup.selectConnectionFirst'));
  });

  it('应能搜索并选中已有连接 (BKU-004)', async () => {
    await $(`button*=${t('action.backup')}`).click();
    await switchToNewWindow(mainWindow);
    await browser.pause(1000);
    const search = await $(`input[placeholder="${t('backup.searchConnection')}"]`);
    await search.setValue('PostgreSQL');
    await browser.pause(500);
    // Click first matching connection row/button in the list
    await browser.execute(() => {
      const candidates = Array.from(document.querySelectorAll('button, [role="option"], li, div'));
      const hit = candidates.find((el) => {
        const text = el.textContent || '';
        return text.includes('PostgreSQL') || text.includes('Postgres') || text.includes('本地');
      });
      (hit as HTMLElement | undefined)?.click();
    });
    await browser.pause(800);
    const body = await $('body').getText();
    // After selection, either DB search or start backup becomes available
    expect(
      body.includes(t('backup.searchDatabase')) ||
        body.includes(t('backup.startBackup')) ||
        body.includes(t('backup.compressGzip')),
    ).toBe(true);
  });
});
