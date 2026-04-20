import { expect, browser, $, $$ } from '@wdio/globals';
import { switchToNewWindow, closeExtraWindows } from '../helpers.js';

describe('新建连接 (CM-002, CM-005)', () => {
  let mainWindow: string;

  before(async () => {
    mainWindow = await browser.getWindowHandle();
  });

  afterEach(async () => {
    await closeExtraWindows(mainWindow);
  });

  it('点击新建连接按钮应打开新窗口 (CM-002)', async () => {
    const btn = await $('button*=新建连接');
    await btn.click();
    const newHandle = await switchToNewWindow(mainWindow);
    expect(newHandle).not.toBe(mainWindow);
  });

  it('新建连接窗口应显示完整表单 (CM-002)', async () => {
    const btn = await $('button*=新建连接');
    await btn.click();
    await switchToNewWindow(mainWindow);

    await expect(await $('div*=选择数据库类型')).toBeDisplayed();
    await expect(await $('div*=连接配置')).toBeDisplayed();
    await expect(await $('input[placeholder="例如：主数据库"]')).toBeDisplayed();
    await expect(await $('button*=测试连接')).toBeDisplayed();
    await expect(await $('button*=取消')).toBeDisplayed();
    await expect(await $('button*=保存')).toBeDisplayed();
  });

  it('应默认选中 PostgreSQL 并显示对应字段 (CM-002)', async () => {
    const btn = await $('button*=新建连接');
    await btn.click();
    await switchToNewWindow(mainWindow);

    const hostInput = await $('input[placeholder="prod-db.example.com"]');
    await expect(hostInput).toBeDisplayed();
    expect(await hostInput.getValue()).toBe('127.0.0.1');

    const allInputs = await $$('input');
    let portFound = false;
    for (const inp of allInputs) {
      if ((await inp.getValue()) === '5432') { portFound = true; break; }
    }
    expect(portFound).toBe(true);

    await expect(await $('input[placeholder="myapp_production"]')).toBeDisplayed();
    await expect(await $('input[placeholder="postgres"]')).toBeDisplayed();
  });

  it('切换数据库类型为 SQLite 应显示文件路径输入框 (CM-002)', async () => {
    const btn = await $('button*=新建连接');
    await btn.click();
    await switchToNewWindow(mainWindow);

    const sqliteBtn = await $('button*=SQLite');
    await sqliteBtn.click();
    const fileInput = await $('input[placeholder="/path/to/db.sqlite"]');
    await expect(fileInput).toBeDisplayed();
    const hostInput = await $('input[placeholder="prod-db.example.com"]');
    await expect(hostInput).not.toBeExisting();
  });

  it('切换数据库类型为 MySQL 应更新默认端口 (CM-002)', async () => {
    const btn = await $('button*=新建连接');
    await btn.click();
    await switchToNewWindow(mainWindow);

    const mysqlBtn = await $('button*=MySQL');
    await mysqlBtn.click();
    await browser.pause(200);
    const allInputs = await $$('input');
    let port3306Found = false;
    for (const inp of allInputs) {
      if ((await inp.getValue()) === '3306') { port3306Found = true; break; }
    }
    expect(port3306Found).toBe(true);
  });

  it('切换数据库类型为 MariaDB 应更新默认端口 (CM-002)', async () => {
    const btn = await $('button*=新建连接');
    await btn.click();
    await switchToNewWindow(mainWindow);

    const mariaBtn = await $('button*=MariaDB');
    if (await mariaBtn.isExisting()) {
      await mariaBtn.click();
      await browser.pause(200);
      const allInputs = await $$('input');
      let port3306Found = false;
      for (const inp of allInputs) {
        if ((await inp.getValue()) === '3306') { port3306Found = true; break; }
      }
      expect(port3306Found).toBe(true);
    }
  });

  it('应能填写连接表单 (CM-002)', async () => {
    const btn = await $('button*=新建连接');
    await btn.click();
    await switchToNewWindow(mainWindow);

    const nameInput = await $('input[placeholder="例如：主数据库"]');
    await nameInput.setValue('E2E-测试连接');
    expect(await nameInput.getValue()).toBe('E2E-测试连接');

    const dbInput = await $('input[placeholder="myapp_production"]');
    await dbInput.setValue('testdb');
    expect(await dbInput.getValue()).toBe('testdb');
  });

  it('展开高级设置应显示 SSL、分组和 SSH 选项 (CM-002)', async () => {
    const btn = await $('button*=新建连接');
    await btn.click();
    await switchToNewWindow(mainWindow);

    const advBtn = await $('button*=高级设置');
    await advBtn.click();
    const sslEl = await $('div*=SSL 模式');
    await sslEl.waitForDisplayed({ timeout: 3000 });
    await expect(await $('div*=颜色标签')).toBeDisplayed();
    await expect(await $('div*=分组')).toBeDisplayed();
    await expect(await $('label*=通过 SSH 隧道连接')).toBeDisplayed();
  });

  it('点击取消按钮应关闭窗口 (CM-005)', async () => {
    const btn = await $('button*=新建连接');
    await btn.click();
    await switchToNewWindow(mainWindow);

    const cancelBtn = await $('button*=取消');
    await cancelBtn.click();
    await browser.waitUntil(
      async () => (await browser.getWindowHandles()).length === 1,
      { timeout: 10000, timeoutMsg: '等待取消后窗口关闭超时' },
    );
    const handles = await browser.getWindowHandles();
    expect(handles.length).toBe(1);
  });

  it('保存连接后窗口应关闭且主窗口显示新连接 (CM-002)', async () => {
    await browser.switchToWindow(mainWindow);
    const btn = await $('button*=新建连接');
    await btn.click();
    await switchToNewWindow(mainWindow);

    const nameInput = await $('input[placeholder="例如：主数据库"]');
    await nameInput.setValue('E2E-自动测试');
    const saveBtn = await $('button*=保存');
    await saveBtn.click();

    await browser.waitUntil(
      async () => (await browser.getWindowHandles()).length === 1,
      { timeout: 10000 },
    );
    await browser.switchToWindow(mainWindow);

    const card = await $('div*=E2E-自动测试');
    await card.waitForDisplayed({ timeout: 5000 });
    await expect(card).toBeDisplayed();
  });
});
