/**
 * AI panel graceful degradation — verify AI features degrade cleanly when no
 * API key is configured.
 *
 * Covers: TC-AI-007 ~ TC-AI-009
 */
import { expect, browser, $ } from '@wdio/globals';
import {
  closeExtraWindows,
  connectSeededPgInWorkspace,
  invokeBackend,
  openSettingsInMainWindow,
  waitForConnectionToolbar,
} from '../helpers.js';

describe('AI 面板无 Key 降级 (TC-AI-007~009)', () => {
  let mainWindow: string;
  let originalAiConfig: unknown;

  before(async () => {
    mainWindow = await browser.getWindowHandle();
    try {
      originalAiConfig = await invokeBackend('ai_get_config');
    } catch {
      originalAiConfig = null;
    }
    await invokeBackend('ai_save_config', {
      config: { providerType: 'open_ai', endpoint: '', apiKey: '', model: '' },
    });
  });

  after(async () => {
    if (originalAiConfig) {
      try {
        await invokeBackend('ai_save_config', { config: originalAiConfig });
      } catch {
        /* */
      }
    }
    await closeExtraWindows(mainWindow);
  });

  it('TC-AI-007: 无 API Key 时 AI 面板应仍可打开并显示配置引导', async () => {
    await browser.switchToWindow(mainWindow);
    await connectSeededPgInWorkspace();
    await waitForConnectionToolbar();

    const aiBtn = await $('[data-testid="conn-toolbar-ai"]');
    await aiBtn.waitForClickable({ timeout: 10000 });
    await aiBtn.click();
    await browser.pause(1000);

    const notConfigured = await $('[data-testid="ai-not-configured"]');
    await notConfigured.waitForDisplayed({ timeout: 10000 });
    const body = await $('body').getText();
    expect(body.includes('Uncaught') || body.includes('Fatal')).toBe(false);
  });

  it('TC-AI-008: 未配置引导应提供跳转设置入口', async () => {
    await browser.switchToWindow(mainWindow);
    const configureBtn = await $('[data-testid="ai-not-configured"] button');
    await expect(configureBtn).toBeDisplayed();
    const label = await configureBtn.getText();
    expect(label.length).toBeGreaterThan(0);
  });

  it('TC-AI-009: AI 设置分区应列出 Provider 选项', async () => {
    await browser.switchToWindow(mainWindow);
    await openSettingsInMainWindow('ai');
    await browser.pause(1500);
    const body = await $('body').getText();
    expect(
      body.includes('OpenAI') ||
        body.includes('Anthropic') ||
        body.includes('DeepSeek') ||
        body.includes('Provider'),
    ).toBe(true);
  });
});
