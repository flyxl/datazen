/**
 * Connection validation / reverse-path E2E (TC-CONN-005/006/007, TC-EDGE-007).
 */
import { expect, browser, $, $$ } from '@wdio/globals';
import {
  waitForNewConnectionDialog,
  closeExtraWindows,
  closeNewConnectionDialogFromUi,
  expandNewConnectionAdvanced,
  openNewConnectionDialogFromUi,
  selectNewConnectionDriver,
  clickNewConnectionTest,
  selectDzOptionInWrap,
} from '../helpers.js';
import { t } from '../i18n.js';

const PG_HOST = process.env.E2E_PG_HOST || '127.0.0.1';
const PG_USER = process.env.E2E_PG_USER || 'postgres';
const PG_DB = process.env.E2E_PG_DB || 'postgres';
const MYSQL_HOST = process.env.E2E_MYSQL_HOST || '127.0.0.1';
const MYSQL_USER = process.env.E2E_MYSQL_USER || 'root';
const MYSQL_DB = process.env.E2E_MYSQL_DB || 'datazen_test';

async function openNewConnectionForm(_mainWindow: string) {
  await openNewConnectionDialogFromUi();
  await $(`[data-testid="new-conn-test-connection"]`).waitForDisplayed({ timeout: 10000 });
}

async function setInputByPlaceholder(placeholder: string, value: string) {
  const input = await $(`input[placeholder="${placeholder}"]`);
  await input.waitForDisplayed({ timeout: 5000 });
  await input.clearValue();
  if (value !== '') await input.setValue(value);
}

async function setPort(value: string) {
  const inputs = await $$('input');
  for (const inp of inputs) {
    const v = await inp.getValue();
    if (v === '5432' || v === '3306') {
      await inp.clearValue();
      await inp.setValue(value);
      return;
    }
  }
}

async function setPassword(value: string) {
  const pw = await $('input[type="password"]');
  if (await pw.isExisting()) {
    await pw.clearValue();
    if (value !== '') await pw.setValue(value);
  }
}

async function clickTestConnection() {
  await clickNewConnectionTest();
}

async function expandAdvancedSettings() {
  await expandNewConnectionAdvanced();
}

async function setSslModeDisable() {
  await expandAdvancedSettings();
  await selectDzOptionInWrap('new-conn-ssl-mode', 'Disable');
}

describe('连接校验反向用例 (TC-CONN-005/006/007, TC-EDGE-007)', () => {
  let mainWindow: string;

  before(async () => {
    mainWindow = await browser.getWindowHandle();
  });

  afterEach(async () => {
    await closeNewConnectionDialogFromUi();
    await closeExtraWindows(mainWindow);
  });

  it('TC-CONN-005: 必填 Host 清空后测试连接应提示失败或校验，且不白屏', async () => {
    await openNewConnectionForm(mainWindow);
    await setInputByPlaceholder('prod-db.example.com', '');
    await clickTestConnection();
    await browser.pause(2000);

    const body = await $('body').getText();
    const failed =
      body.includes(t('newConn.testFailed')) ||
      body.includes('必填') ||
      body.includes('required') ||
      body.includes('Host') ||
      body.includes('失败');
    expect(failed).toBe(true);
    await expect(await $('[data-testid="new-conn-test-connection"]')).toBeDisplayed();
  });

  it('TC-CONN-006: 无效 Host 测试连接应失败并显示错误', async () => {
    await openNewConnectionForm(mainWindow);
    await setInputByPlaceholder('例如：主数据库', 'E2E-无效Host');
    await setInputByPlaceholder('prod-db.example.com', 'invalid-host-12345.nonexistent');
    await setInputByPlaceholder('myapp_production', PG_DB);
    await setInputByPlaceholder('postgres', PG_USER);
    await clickTestConnection();

    await browser.waitUntil(
      async () => {
        const body = await $('body').getText();
        return (
          body.includes(t('newConn.testFailed')) ||
          body.includes('失败') ||
          body.includes('error') ||
          body.includes('Error')
        );
      },
      { timeout: 30000, timeoutMsg: '等待无效 Host 测试失败超时' },
    );

    const body = await $('body').getText();
    expect(body.toLowerCase()).not.toContain('wrong_password_123');
  });

  it('TC-CONN-007: MySQL 错误密码应认证失败且错误信息不含明文密码', async () => {
    await openNewConnectionForm(mainWindow);
    await selectNewConnectionDriver('mysql');
    await browser.pause(300);

    await setInputByPlaceholder('例如：主数据库', 'E2E-错误密码');
    await setInputByPlaceholder('prod-db.example.com', MYSQL_HOST);
    await setPort('3306');
    await setInputByPlaceholder('myapp_production', MYSQL_DB);
    // MySQL username placeholder may differ; fill visible text inputs carefully
    const inputs = await $$('input');
    for (const inp of inputs) {
      const ph = (await inp.getAttribute('placeholder')) || '';
      const type = (await inp.getAttribute('type')) || 'text';
      if (type === 'password') continue;
      if (ph.includes('root') || ph === 'root' || ph.includes('用户') || ph.includes('user')) {
        await inp.clearValue();
        await inp.setValue(MYSQL_USER);
      }
    }
    // Fallback: last non-password text-ish field near form often username
    const userCandidates = await $$('input[placeholder="root"], input[placeholder="postgres"]');
    for (const inp of userCandidates) {
      if (await inp.isExisting()) {
        await inp.clearValue();
        await inp.setValue(MYSQL_USER);
      }
    }
    await setPassword('wrong_password_123');
    await clickTestConnection();

    await browser.waitUntil(
      async () => {
        const body = await $('body').getText();
        return (
          body.includes(t('newConn.testFailed')) ||
          body.includes('Access denied') ||
          body.includes('认证') ||
          body.includes('失败') ||
          body.includes('1045')
        );
      },
      { timeout: 30000, timeoutMsg: '等待错误密码测试失败超时' },
    );

    const body = await $('body').getText();
    expect(body).not.toContain('wrong_password_123');
  });

  it('TC-EDGE-007: 空密码连接应成功或给出认证错误，不崩溃', async () => {
    await openNewConnectionForm(mainWindow);
    await setInputByPlaceholder('例如：主数据库', 'E2E-空密码');
    await setInputByPlaceholder('prod-db.example.com', PG_HOST);
    await setInputByPlaceholder('myapp_production', PG_DB);
    await setInputByPlaceholder('postgres', PG_USER);
    await setPassword('');
    await setSslModeDisable();
    await clickTestConnection();

    await browser.waitUntil(
      async () => {
        const body = await $('body').getText();
        return (
          body.includes(t('newConn.testSuccess')) ||
          body.includes('连接成功') ||
          body.includes(t('newConn.testFailed')) ||
          body.includes('失败')
        );
      },
      { timeout: 30000, timeoutMsg: '等待空密码测试结果超时' },
    );

    await expect(await $('[data-testid="new-conn-test-connection"]')).toBeDisplayed();
  });
});
