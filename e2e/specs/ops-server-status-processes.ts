/**
 * E2E: ops 5.4 Server Status + Process List (TS-OPS-E01 / TS-OPS-E02)
 * 右键连接 -> 服务器状态面板渲染指标卡片; 进程列表表头齐全;
 * Kill 仅验证确认框出现后取消(不做破坏性终止)。依赖 seed 的 本地 PostgreSQL。
 */
import { expect, browser, $, $$ } from '@wdio/globals';
import { t } from '../i18n.js';
import { closeExtraWindows, openSeededPgConnectionWindow } from '../helpers.js';

async function rightClickConn(connName: string) {
  await browser.execute((name: string) => {
    const items = Array.from(document.querySelectorAll('[data-conn-item]'));
    const item = items.find((el) => {
      const attr = el.getAttribute('data-conn-name');
      if (attr) return attr === name;
      return el.querySelector('span.truncate')?.textContent?.trim() === name;
    });
    if (!item) return false;
    const rect = (item as HTMLElement).getBoundingClientRect();
    item.dispatchEvent(
      new MouseEvent('contextmenu', {
        bubbles: true,
        cancelable: true,
        clientX: rect.left + rect.width / 2,
        clientY: rect.top + rect.height / 2,
      }),
    );
    return true;
  }, connName);
  await browser.pause(400);
}

async function clickMenuItem(label: string) {
  await browser.execute((lbl: string) => {
    const menuItems = document.querySelectorAll('[data-testid="web-context-menu"] button');
    for (const item of menuItems) {
      if (item.textContent?.includes(lbl)) {
        (item as HTMLElement).click();
        return;
      }
    }
  }, label);
  await browser.pause(600);
}

describe('ops 5.4 server status and process list (OPS-SS/PL)', () => {
  let mainWindow: string;
  const CONN = '本地 PostgreSQL';

  before(async () => {
    mainWindow = await browser.getWindowHandle();
    await openSeededPgConnectionWindow(mainWindow);
  });

  after(async () => {
    await closeExtraWindows(mainWindow);
  });

  it('OPS-SS-001: status panel renders readonly metrics', async () => {
    await rightClickConn(CONN);
    await clickMenuItem(t('main.ctx.serverStatus'));
    await browser.waitUntil(
      async () => (await $('body').getText()).includes(t('serverStatus.version')),
      { timeout: 10000, timeoutMsg: 'Server status panel did not render' },
    );
    const body = await $('body').getText();
    expect(body).toContain(t('serverStatus.version'));
    expect(body).toContain(t('serverStatus.uptime'));
    expect(body).toContain(t('serverStatus.connections'));
  });

  it('OPS-SS-002: refresh keeps panel healthy', async () => {
    const refresh = await $(`button*=${t('serverStatus.refresh')}`);
    await refresh.click();
    await browser.pause(800);
    const body = await $('body').getText();
    expect(body).toContain(t('serverStatus.version'));
  });

  it('OPS-PL-001: process list table headers render', async () => {
    await rightClickConn(CONN);
    await clickMenuItem(t('main.ctx.processList'));
    await browser.waitUntil(
      async () => {
        const count = await browser.execute(
          () => document.querySelectorAll('table th, [role="columnheader"]').length,
        );
        return count > 0;
      },
      { timeout: 10000, timeoutMsg: 'Process list table did not render' },
    );
    const body = await $('body').getText();
    expect(body).toContain(t('processList.colPid'));
    expect(body).toContain(t('processList.colUser'));
    expect(body).toContain(t('processList.colState'));
  });

  it('OPS-PL-002: kill shows confirm then cancel', async () => {
    const killBtn = await $(`button*=${t('processList.kill')}`);
    await expect(killBtn).toBeDisplayed();
    await killBtn.click();
    await browser.pause(400);
    const body = await $('body').getText();
    expect(body).toContain(t('processList.killTitle'));
    const cancel = await $('button*=取消');
    if (await cancel.isExisting()) {
      await cancel.click();
      await browser.pause(300);
    }
  });
});
