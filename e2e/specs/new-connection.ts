import { expect, browser, $, $$ } from '@wdio/globals';
import {
  closeNewConnectionDialogFromUi,
  openNewConnectionDialogFromUi,
  closeExtraWindows,
  captureJourneyStep,
  expandAllGroups,
  selectNewConnectionDriver,
  clickNewConnectionSave,
} from '../helpers.js';

describe('新建连接 (CM-002, CM-005)', () => {
  let mainWindow: string;

  before(async () => {
    mainWindow = await browser.getWindowHandle();
  });

  afterEach(async () => {
    try {
      if (await $('[data-testid="new-connection-dialog"]').isExisting()) {
        await closeNewConnectionDialogFromUi();
      }
    } catch {
      /* dialog already closed */
    }
    await closeExtraWindows(mainWindow);
  });

  after(async () => {
    const conns: any[] = await browser.executeAsync((done: (r: any) => void) => {
      (window as any).__TAURI_INTERNALS__
        .invoke('get_connections')
        .then((r: any) => done(r))
        .catch(() => done([]));
    });
    for (const c of conns) {
      if (c.name === 'E2E-自动测试' || c.name === 'E2E-测试连接') {
        await browser.executeAsync((id: string, done: (r: any) => void) => {
          (window as any).__TAURI_INTERNALS__
            .invoke('delete_connection', { id })
            .then(() => done(null))
            .catch(() => done(null));
        }, c.id);
      }
    }
    await browser.pause(300);
  });

  it('点击新建连接按钮应打开弹窗 (CM-002)', async () => {
    await openNewConnectionDialogFromUi();
    await expect(await $('[data-testid="new-connection-dialog"]')).toBeDisplayed();
    await captureJourneyStep('dialog-open', 0, true);
  });

  it('新建连接弹窗应显示完整表单 (CM-002)', async () => {
    await openNewConnectionDialogFromUi();
    await expect(await $('div*=选择数据库类型')).toBeDisplayed();
    await expect(await $('div*=连接配置')).toBeDisplayed();
    await expect(await $('input[placeholder="例如：主数据库"]')).toBeDisplayed();
    await expect(await $('[data-testid="new-conn-test-connection"]')).toBeDisplayed();
    await expect(await $('[data-testid="new-conn-cancel"]')).toBeDisplayed();
    await expect(await $('[data-testid="new-conn-save"]')).toBeDisplayed();
  });

  it('应默认选中 PostgreSQL 并显示对应字段 (CM-002)', async () => {
    await openNewConnectionDialogFromUi();
    const hostInput = await $('input[placeholder="prod-db.example.com"]');
    await expect(hostInput).toBeDisplayed();
    expect(await hostInput.getValue()).toBe('127.0.0.1');
    const allInputs = await $$('input');
    let portFound = false;
    for (const inp of allInputs) {
      if ((await inp.getValue()) === '5432') {
        portFound = true;
        break;
      }
    }
    expect(portFound).toBe(true);
    await expect(await $('input[placeholder="myapp_production"]')).toBeDisplayed();
    await expect(await $('input[placeholder="postgres"]')).toBeDisplayed();
    await captureJourneyStep('pg-default-fields', 0, true);
  });

  it('切换数据库类型为 SQLite 应显示文件路径输入框 (CM-002)', async () => {
    await openNewConnectionDialogFromUi();
    await selectNewConnectionDriver('sqlite');
    const fileInput = await $('input[placeholder="/path/to/db.sqlite"]');
    await expect(fileInput).toBeDisplayed();
    const hostInput = await $('input[placeholder="prod-db.example.com"]');
    await expect(hostInput).not.toBeExisting();
    await captureJourneyStep('sqlite-fields', 0, true);
  });

  it('切换数据库类型为 MySQL 应更新默认端口 (CM-002)', async () => {
    await openNewConnectionDialogFromUi();
    await selectNewConnectionDriver('mysql');
    await browser.pause(200);
    const allInputs = await $$('input');
    let port3306Found = false;
    for (const inp of allInputs) {
      if ((await inp.getValue()) === '3306') {
        port3306Found = true;
        break;
      }
    }
    expect(port3306Found).toBe(true);
    await captureJourneyStep('mysql-port-3306', 0, true);
  });

  it('切换数据库类型为 MariaDB 应更新默认端口 (CM-002)', async () => {
    await openNewConnectionDialogFromUi();
    const mariaBtn = await $('[data-testid="new-conn-driver-mariadb"]');
    if (await mariaBtn.isExisting()) {
      await mariaBtn.click();
      await browser.pause(200);
      const allInputs = await $$('input');
      let port3306Found = false;
      for (const inp of allInputs) {
        if ((await inp.getValue()) === '3306') {
          port3306Found = true;
          break;
        }
      }
      expect(port3306Found).toBe(true);
      await captureJourneyStep('mariadb-port-3306', 0, true);
    }
  });

  it('应能填写连接表单 (CM-002)', async () => {
    await openNewConnectionDialogFromUi();
    const nameInput = await $('input[placeholder="例如：主数据库"]');
    await nameInput.setValue('E2E-测试连接');
    expect(await nameInput.getValue()).toBe('E2E-测试连接');
    const dbInput = await $('input[placeholder="myapp_production"]');
    await dbInput.setValue('testdb');
    expect(await dbInput.getValue()).toBe('testdb');
    await captureJourneyStep('form-filled', 0, true);
  });

  it('展开高级设置应显示 SSL、分组和 SSH 选项 (CM-002)', async () => {
    await openNewConnectionDialogFromUi();
    const advBtn = await $('button*=高级设置');
    await advBtn.click();
    await browser.pause(400);
    const sslEl = await $('div*=SSL 模式');
    await sslEl.waitForDisplayed({ timeout: 3000 });
    await expect(await $('div*=颜色标签')).toBeDisplayed();
    await expect(await $('div*=分组')).toBeDisplayed();
    await expect(await $('[data-testid="new-conn-ssh-tunnel"]')).toBeDisplayed();
    await captureJourneyStep('advanced-settings-expanded', 0, true);
  });

  it('点击取消按钮应关闭弹窗 (CM-005)', async () => {
    await openNewConnectionDialogFromUi();
    await closeNewConnectionDialogFromUi();
    await expect(await $('[data-testid="new-connection-dialog"]')).not.toBeExisting();
    await captureJourneyStep('dialog-closed', 0, true);
  });

  it('保存连接后弹窗应关闭且主窗口显示新连接 (CM-002)', async () => {
    await browser.switchToWindow(mainWindow);
    await openNewConnectionDialogFromUi();
    const nameInput = await $('input[placeholder="例如：主数据库"]');
    await nameInput.setValue('E2E-自动测试');
    await clickNewConnectionSave();
    await browser.waitUntil(
      async () => !(await $('[data-testid="new-connection-dialog"]').isExisting()),
      { timeout: 10000 },
    );
    await browser.switchToWindow(mainWindow);
    await expandAllGroups();
    const card = await $('div*=E2E-自动测试');
    await card.waitForDisplayed({ timeout: 5000 });
    await expect(card).toBeDisplayed();
    await captureJourneyStep('connection-saved-in-list', 0, true);
  });
});
