import { expect, browser, $ } from '@wdio/globals';
import { t } from '../i18n.js';
import {
  closeExtraWindows,
  openSeededPgConnectionWindow,
  waitForNewConnectionButton,
} from '../helpers.js';

/**
 * Objects browser + Privileges panel Host UI paths (OBJ / PRV).
 */

describe('对象浏览器与权限 (OBJ/PRV)', () => {
  let mainWindow: string;

  before(async () => {
    mainWindow = await browser.getWindowHandle();
    await waitForNewConnectionButton(10000);
    await openSeededPgConnectionWindow(mainWindow);
  });

  after(async () => {
    await closeExtraWindows(mainWindow);
  });

  it('应能打开对象浏览器并切换函数子标签 (OBJ-001)', async () => {
    const objBtn = await $(`button*=${t('objects.title')}`);
    await objBtn.waitForDisplayed({ timeout: 15000 });
    await objBtn.click();
    await browser.pause(800);
    const fnBtn = await $(`button*=${t('objects.function')}`);
    await expect(fnBtn).toBeDisplayed();
    await fnBtn.click();
    await browser.pause(800);
    const body = await $('body').getText();
    // Empty list or function names — panel must load without crash
    expect(body).toContain(t('objects.function'));
  });

  it('例程列表右键应打开 Web 菜单 (OBJ-003)', async () => {
    const item = await $('[data-testid="object-browser-item"]');
    if (!(await item.isExisting())) {
      return;
    }
    await item.click({ button: 'right' });
    const menu = await $('[data-testid="web-context-menu"]');
    await menu.waitForDisplayed({ timeout: 5000 });
    const text = await menu.getText();
    expect(text).toContain(t('objects.open'));
    expect(await $("[data-testid='web-context-item-copy-name']").isExisting()).toBe(true);
    await browser.keys('Escape');
  });

  it('应能切换到过程子标签 (OBJ-002)', async () => {
    const procBtn = await $(`button*=${t('objects.procedure')}`);
    if (await procBtn.isExisting()) {
      await procBtn.click();
      await browser.pause(600);
      expect(await $('body').getText()).toContain(t('objects.procedure'));
    }
  });

  it('应能打开权限面板并看到刷新/执行相关控件 (PRV-001)', async () => {
    const privBtn = await $(`button*=${t('privileges.title')}`);
    await privBtn.waitForDisplayed({ timeout: 10000 });
    await privBtn.click();
    await browser.pause(300);
    const body = await $('body').getText();
    expect(body).toContain(t('privileges.title'));
    expect(
      body.includes(t('privileges.sqlHint')) ||
        body.includes(t('privileges.empty')) ||
        body.includes(t('privileges.grantee')) ||
        body.includes('GRANT'),
    ).toBe(true);
  });
});
