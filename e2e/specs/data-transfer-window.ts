import { expect, browser, $ } from '@wdio/globals';
import { t } from '../i18n.js';
import { closeExtraWindows } from '../helpers.js';

/**
 * Data Transfer window smoke (DTW-001~DTW-003).
 * Full cross-dialect execute paths are not covered here — see data-transfer-guide.md V1 limits.
 */

describe('数据传输窗口 (DTW-001~DTW-003)', () => {
  let mainWindow: string;

  before(async () => {
    mainWindow = await browser.getWindowHandle();
    await $(`button*=${t('action.newConnection')}`).waitForDisplayed({ timeout: 10000 });
  });

  after(async () => {
    await closeExtraWindows(mainWindow);
    await browser.switchToWindow(mainWindow);
  });

  it('DTW-001: 应能通过 URL 打开数据传输窗口', async () => {
    await browser.url('tauri://localhost/window.html?window=data-transfer');
    await browser.pause(1500);
    const root = await $('[data-testid="data-transfer-window"]');
    await expect(root).toBeDisplayed();
    const body = await $('body').getText();
    expect(body).toContain(t('transfer.title'));
    expect(body).toContain(t('transfer.source'));
    expect(body).toContain(t('transfer.target'));
  });

  it('DTW-002: 应显示向导步骤与模式选项', async () => {
    await expect(await $('[data-testid="data-transfer-step-endpoints"]')).toBeDisplayed();
    await expect(await $('[data-testid="data-transfer-mode-structure"]')).toBeDisplayed();
    await expect(await $('[data-testid="data-transfer-mode-data"]')).toBeDisplayed();
    await expect(await $('[data-testid="data-transfer-mode-both"]')).toBeDisplayed();
  });

  it('DTW-003: 未选两端点 Next 应提示错误', async () => {
    const next = await $('[data-testid="data-transfer-next"]');
    await next.waitForDisplayed({ timeout: 8000 });
    await next.click();
    await browser.pause(500);
    const err = await $('[data-testid="data-transfer-error"]');
    await expect(err).toBeDisplayed();
    expect(await err.getText()).toContain(t('transfer.selectBoth'));
  });
});
