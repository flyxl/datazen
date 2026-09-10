/**
 * AI panel graceful degradation — verify AI features degrade cleanly when no
 * API key is configured.
 *
 * Covers: TC-AI-007 ~ TC-AI-009
 */
import { expect, browser, $ } from '@wdio/globals';
import { t } from '../i18n.js';
import {
  closeExtraWindows,
  captureJourneyStep,
  connectSeededPgInWorkspace,
  invokeBackend,
  openQueryTab,
  openSettingsInMainWindow,
  waitForConnectionToolbar,
} from '../helpers.js';

describe('AI 面板无 Key 降级 (TC-AI-007~009)', () => {
  let mainWindow: string;
  let hadConfig = false;

  before(async () => {
    mainWindow = await browser.getWindowHandle();
    try {
      const cfg = await invokeBackend<unknown | null>('ai_get_config');
      hadConfig = cfg != null;
    } catch {
      hadConfig = false;
    }
    await invokeBackend('ai_delete_config');
  });

  after(async () => {
    if (hadConfig) {
      // Best-effort restore is omitted — E2E sandbox; ai-features re-seeds when key present.
    }
    await closeExtraWindows(mainWindow);
  });

  it('TC-AI-007: 无 API Key 时 AI 面板应仍可打开并显示配置引导', async () => {
    await browser.switchToWindow(mainWindow);
    await connectSeededPgInWorkspace();
    await waitForConnectionToolbar();
    await openQueryTab();
    await browser.pause(500);

    const aiBtn = await $('[data-testid="conn-toolbar-ai"]');
    await aiBtn.waitForClickable({ timeout: 10000 });
    await aiBtn.click();
    await browser.pause(1000);

    const notConfigured = await $('[data-testid="ai-not-configured"]');
    await notConfigured.waitForDisplayed({ timeout: 10000 });
    await captureJourneyStep('ai-panel-no-key', 0, true);
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

  it('TC-AI-009: AI 设置分区应显示模型配置入口（空态 + 添加模型）', async () => {
    await browser.switchToWindow(mainWindow);
    await openSettingsInMainWindow('ai');
    await browser.pause(1500);
    // Profile-based AI settings: with no key there are no profiles — the
    // empty state plus the add-model entry must be visible. Provider options
    // (OpenAI/Anthropic/DeepSeek/…) live inside the add-model dialog.
    const emptyState = await $('[data-testid="ai-models-empty"]');
    await emptyState.waitForDisplayed({ timeout: 10000 });
    const body = await $('body').getText();
    expect(
      body.includes(t('settings.ai.modelsTitle')) ||
        body.includes(t('settings.ai.addModel')) ||
        body.includes(t('settings.ai.noModels')) ||
        body.includes('OpenAI') ||
        body.includes('Anthropic') ||
        body.includes('DeepSeek') ||
        body.includes('Provider'),
    ).toBe(true);
  });
});
