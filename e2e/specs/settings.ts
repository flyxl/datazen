import { expect, browser, $ } from '@wdio/globals';
import {
  backFromSettingsInMainWindow,
  captureJourneyStep,
  closeExtraWindows,
  openSettingsInMainWindow,
} from '../helpers.js';
import { t } from '../i18n.js';

async function invokeBackend<T>(cmd: string, args: Record<string, unknown> = {}): Promise<T> {
  const result = await browser.executeAsync(
    (c: string, a: string, done: (r: any) => void) => {
      (window as any).__TAURI_INTERNALS__
        .invoke(c, JSON.parse(a))
        .then((r: any) => done(r))
        .catch((e: any) => done({ __error: String(e) }));
    },
    cmd,
    JSON.stringify(args),
  );
  if (result && typeof result === 'object' && '__error' in (result as any)) {
    throw new Error((result as any).__error);
  }
  return result as T;
}

describe('Settings (SS-001~SS-006)', () => {
  before(async () => {
    await invokeBackend('save_settings', {
      settings: {
        theme: { mode: 'dark', packId: null },
        language: 'zh-CN',
        limitSelectResults: true,
        queryResultLimit: 5000,
        editorFontSize: 13,
        editorFontFamily: 'Menlo, Monaco, Consolas, "Courier New", monospace',
        confirmOnDelete: true,
        autoCommit: true,
        safeMode: true,
        defaultPageSize: 50,
      },
    });
    await browser.pause(500);
    await browser.refresh();
    await browser.pause(2000);
  });

  // ── Theme toggle (SS-001) ──

  it('SS-001: should display theme toggle button', async () => {
    const themeBtn = await $('button[title*="主题"]');
    await expect(themeBtn).toBeDisplayed();
  });

  it('SS-001: clicking theme button should show options', async () => {
    const themeBtn = await $('button[title*="主题"]');
    await themeBtn.click();
    await browser.pause(300);

    await expect(await $('button*=浅色')).toBeDisplayed();
    await expect(await $('button*=深色')).toBeDisplayed();
    await expect(await $('button*=跟随系统')).toBeDisplayed();
    await captureJourneyStep('theme-menu-open');
  });

  it('SS-001: light theme should remove dark class', async () => {
    const lightBtn = await $('button*=浅色');
    await lightBtn.click();
    await browser.pause(500);

    const html = await $('html');
    const cls = await html.getAttribute('class');
    expect(cls).not.toContain('dark');
  });

  it('SS-001: dark theme should add dark class', async () => {
    const themeBtn = await $('button[title*="主题"]');
    await themeBtn.click();
    await browser.pause(300);

    const darkBtn = await $('button*=深色');
    await darkBtn.click();
    await browser.pause(500);

    const html = await $('html');
    const cls = await html.getAttribute('class');
    expect(cls).toContain('dark');
  });

  it('SS-002: system theme should work', async () => {
    const themeBtn = await $('button[title*="主题"]');
    await themeBtn.click();
    await browser.pause(300);

    const systemBtn = await $('button*=跟随系统');
    await expect(systemBtn).toBeDisplayed();
    await systemBtn.click();
    await browser.pause(300);
  });

  // ── Settings persistence (SS-003~SS-006) ──

  it('SS-003: settings should persist theme changes', async () => {
    await invokeBackend('save_settings', {
      settings: {
        theme: { mode: 'light', packId: null },
        language: 'zh-CN',
        limitSelectResults: true,
        queryResultLimit: 5000,
        editorFontSize: 13,
        editorFontFamily: 'Menlo',
        confirmOnDelete: true,
        autoCommit: true,
        safeMode: true,
        defaultPageSize: 50,
      },
    });

    const loaded = await invokeBackend<any>('get_settings');
    expect(loaded.theme.mode).toBe('light');
  });

  it('SS-004: settings should persist page size', async () => {
    await invokeBackend('save_settings', {
      settings: {
        theme: { mode: 'dark', packId: null },
        language: 'zh-CN',
        limitSelectResults: true,
        queryResultLimit: 5000,
        editorFontSize: 13,
        editorFontFamily: 'Menlo',
        confirmOnDelete: true,
        autoCommit: true,
        safeMode: true,
        defaultPageSize: 100,
      },
    });

    const loaded = await invokeBackend<any>('get_settings');
    expect(loaded.defaultPageSize).toBe(100);
  });

  it('SS-005: settings should persist query result limit', async () => {
    await invokeBackend('save_settings', {
      settings: {
        theme: { mode: 'dark', packId: null },
        language: 'zh-CN',
        limitSelectResults: true,
        queryResultLimit: 10000,
        editorFontSize: 13,
        editorFontFamily: 'Menlo',
        confirmOnDelete: true,
        autoCommit: true,
        safeMode: true,
        defaultPageSize: 50,
      },
    });

    const loaded = await invokeBackend<any>('get_settings');
    expect(loaded.queryResultLimit).toBe(10000);
  });

  it('SS-006: settings should persist editor preferences', async () => {
    await invokeBackend('save_settings', {
      settings: {
        theme: { mode: 'dark', packId: null },
        language: 'zh-CN',
        limitSelectResults: true,
        queryResultLimit: 5000,
        editorFontSize: 16,
        editorFontFamily: 'Fira Code',
        confirmOnDelete: false,
        autoCommit: false,
        safeMode: true,
        defaultPageSize: 50,
      },
    });

    const loaded = await invokeBackend<any>('get_settings');
    expect(loaded.editorFontSize).toBe(16);
    expect(loaded.editorFontFamily).toBe('Fira Code');
    expect(loaded.confirmOnDelete).toBe(false);
    expect(loaded.autoCommit).toBe(false);
  });

  it('TC-SET-003: 设置窗口编辑器分区应显示字体相关控件', async () => {
    await openSettingsInMainWindow();
    const editorNav = await $('button*=编辑器');
    if (await editorNav.isExisting()) {
      await editorNav.click();
      await browser.pause(400);
    }
    const body = await $('body').getText();
    expect(
      body.includes('字体') ||
        body.includes('Font') ||
        body.includes('字号') ||
        body.includes('字'),
    ).toBe(true);
    await captureJourneyStep('settings-editor-section');
  });

  it('TC-SET-004: 设置窗口数据浏览分区应显示分页/限制相关项', async () => {
    await openSettingsInMainWindow();
    const dataNav = await $('button*=数据浏览');
    if (await dataNav.isExisting()) {
      await dataNav.click();
      await browser.pause(400);
    }
    const body = await $('body').getText();
    expect(
      body.includes('页') ||
        body.includes('page') ||
        body.includes('限制') ||
        body.includes('limit') ||
        body.includes('行'),
    ).toBe(true);
    await captureJourneyStep('settings-data-section');
  });

  it('TC-SET-007: 设置窗口应有 Prompt 自定义入口', async () => {
    await openSettingsInMainWindow();
    const promptNav = await $('button*=Prompt 管理');
    const promptNavAlt = await $('button*=Prompt');
    if (await promptNav.isExisting()) {
      await promptNav.click();
    } else if (await promptNavAlt.isExisting()) {
      await promptNavAlt.click();
    }
    await browser.pause(500);
    const body = await $('body').getText();
    expect(
      body.includes('Prompt') ||
        body.includes('提示') ||
        body.includes('场景') ||
        body.includes('驱动'),
    ).toBe(true);
  });

  it('SS-NAV-001: 设置侧栏应能进入行为 / 日志 / AI / MCP / 扩展分区', async () => {
    await openSettingsInMainWindow();

    const sections: Array<{ label: string; expectText: string[] }> = [
      {
        label: t('settings.behavior'),
        expectText: [t('settings.dataCleanup.title'), '确认', 'Safe', '安全'],
      },
      { label: t('settings.logging'), expectText: ['日志', 'Log', '路径'] },
      {
        label: t('settings.ai'),
        expectText: [t('settings.ai.provider'), t('settings.ai.apiKey'), 'API'],
      },
      { label: t('mcp.title'), expectText: ['MCP', t('mcp.title')] },
      {
        label: t('mcpClient.title'),
        expectText: [t('mcpClient.title'), t('mcpClient.savedConfigs'), 'MCP'],
      },
      {
        label: t('settings.extensions.title'),
        expectText: [t('settings.extensions.title'), t('settings.extensions.empty'), '扩展'],
      },
    ];

    for (const sec of sections) {
      const nav = await $(`button*=${sec.label}`);
      await nav.waitForDisplayed({ timeout: 8000 });
      await nav.click();
      await browser.pause(400);
      const body = await $('body').getText();
      expect(sec.expectText.some((frag) => body.includes(frag))).toBe(true);
    }
  });

  it('SS-CLN-001: 行为分区应能看到数据清理入口', async () => {
    await openSettingsInMainWindow();
    await $(`button*=${t('settings.behavior')}`).click();
    await browser.pause(400);
    const body = await $('body').getText();
    expect(body).toContain(t('settings.dataCleanup.title'));
    expect(body).toContain(t('settings.dataCleanup.run'));
  });

  // ── F1: SettingsPage in main window ──

  describe('F1 SettingsPage navigation (F1-E2E)', () => {
    it('F1-E2E-001: menu:open-settings opens SettingsPage in main window', async () => {
      await openSettingsInMainWindow();
      await expect(await $('[data-testid="settings-page"]')).toBeDisplayed();
      await expect(await $('[data-testid="settings-back"]')).toBeDisplayed();
      await expect(await $('[data-testid="workspace-nav-connections"]')).not.toBeDisplayed();
    });

    it('F1-E2E-002: back button returns to main workspace shell', async () => {
      await openSettingsInMainWindow();
      await backFromSettingsInMainWindow();
      await expect(await $('[data-testid="workspace-nav-connections"]')).toBeDisplayed();
      await expect(await $('[data-testid="workspace-nav-workflow"]')).toBeDisplayed();
      await expect(await $('[data-testid="settings-page"]')).not.toBeExisting();
    });

    it('F1-E2E-003: menu:open-settings with section opens target nav', async () => {
      await openSettingsInMainWindow('ai');
      const aiNav = await $(`button*=${t('settings.ai')}`);
      await aiNav.waitForDisplayed({ timeout: 8000 });
      const body = await $('body').getText();
      expect(
        body.includes(t('settings.ai.provider')) ||
          body.includes('API') ||
          body.includes('Provider'),
      ).toBe(true);
    });
  });

  it('SS-MCP-CLIENT-001: MCP Client 分区应支持保存带 env 的配置', async () => {
    await invokeBackend('save_settings', {
      settings: {
        theme: { mode: 'dark', packId: null },
        language: 'zh-CN',
        limitSelectResults: true,
        queryResultLimit: 5000,
        editorFontSize: 13,
        editorFontFamily: 'Menlo',
        confirmOnDelete: true,
        autoCommit: true,
        safeMode: true,
        defaultPageSize: 50,
        mcpClientServers: [],
      },
    });

    await openSettingsInMainWindow();
    const mcpClientNav = await $(`button*=${t('mcpClient.title')}`);
    await mcpClientNav.waitForDisplayed({ timeout: 8000 });
    await mcpClientNav.click();
    await browser.pause(400);

    const savedConfigsLabel = await $(`*=${t('mcpClient.savedConfigs')}`);
    await savedConfigsLabel.waitForDisplayed({ timeout: 5000 });

    const addBtn = await $(`button*=${t('mcpClient.addServer')}`);
    await addBtn.waitForDisplayed({ timeout: 5000 });
    await addBtn.click();
    await browser.pause(300);

    const idInput = await $('input[placeholder="my-mcp-server"]');
    await idInput.waitForDisplayed({ timeout: 5000 });
    await idInput.setValue('e2e-mcp-test');
    const nameInput = await $('input[placeholder="My Server"]');
    await nameInput.setValue('E2E MCP');
    const cmdInput = await $('input[placeholder*="/usr/local/bin"]');
    await cmdInput.waitForDisplayed({ timeout: 5000 });
    await cmdInput.setValue('/usr/bin/true');

    const addEnvBtn = await $(`button*=${t('mcpClient.addEnv')}`);
    await addEnvBtn.waitForDisplayed({ timeout: 5000 });
    await addEnvBtn.click();
    await browser.pause(200);

    const envKeyInputs = await $$(
      'input[placeholder*="变量名"], input[placeholder*="Variable name"]',
    );
    expect(envKeyInputs.length).toBeGreaterThan(0);
    await envKeyInputs[0].setValue('TEST_ENV');
    const envValueInputs = await $$('input[placeholder="值"], input[placeholder="Value"]');
    expect(envValueInputs.length).toBeGreaterThan(0);
    await envValueInputs[0].setValue('e2e-value');

    const saveBtn = await $(`button*=${t('mcpClient.save')}`);
    await saveBtn.click();
    await browser.pause(500);

    const bodyAfter = await $('body').getText();
    expect(bodyAfter).toContain('e2e-mcp-test');

    const loaded = await invokeBackend<any>('get_settings');
    expect(Array.isArray(loaded.mcpClientServers)).toBe(true);
    const saved = loaded.mcpClientServers.find((c: { id: string }) => c.id === 'e2e-mcp-test');
    expect(saved).toBeDefined();
    expect(saved.command).toBe('/usr/bin/true');
    expect(saved.env?.TEST_ENV).toBe('e2e-value');
  });

  // ── Persistence: language / font-size / confirm-delete (from settings-persistence.ts) ──

  it('TC-SET-008: 语言切换应更新 UI 文本', async () => {
    const settings = await invokeBackend<Record<string, unknown>>('get_settings');
    const originalLang = settings.language;

    const newLang = originalLang === 'zh-CN' ? 'en' : 'zh-CN';
    await invokeBackend('save_settings', { settings: { ...settings, language: newLang } });
    await browser.refresh();
    await browser.pause(2000);

    const body = await $('body').getText();
    if (newLang === 'en') {
      expect(
        body.includes('Settings') ||
          body.includes('Connection') ||
          body.includes('New Connection') ||
          body.includes('Query'),
      ).toBe(true);
    } else {
      expect(
        body.includes('设置') ||
          body.includes('连接') ||
          body.includes('新建连接') ||
          body.includes('查询'),
      ).toBe(true);
    }

    // Restore
    await invokeBackend('save_settings', { settings: { ...settings, language: originalLang } });
    await browser.refresh();
    await browser.pause(1500);
  });

  it('TC-SET-009: 编辑器字体大小设置应持久化', async () => {
    const settings = await invokeBackend<Record<string, unknown>>('get_settings');
    const originalFontSize = settings.editorFontSize;

    const newFontSize = originalFontSize === 14 ? 16 : 14;
    await invokeBackend('save_settings', {
      settings: { ...settings, editorFontSize: newFontSize },
    });
    await browser.refresh();
    await browser.pause(1500);

    const persisted = await invokeBackend<Record<string, unknown>>('get_settings');
    expect(persisted.editorFontSize).toBe(newFontSize);

    // Restore
    await invokeBackend('save_settings', {
      settings: { ...settings, editorFontSize: originalFontSize },
    });
  });

  it('TC-SET-010: 确认删除开关应持久化', async () => {
    const settings = await invokeBackend<Record<string, unknown>>('get_settings');
    const originalConfirm = settings.confirmOnDelete;

    await invokeBackend('save_settings', {
      settings: { ...settings, confirmOnDelete: !originalConfirm },
    });
    await browser.refresh();
    await browser.pause(1500);

    const persisted = await invokeBackend<Record<string, unknown>>('get_settings');
    expect(persisted.confirmOnDelete).toBe(!originalConfirm);

    // Restore
    await invokeBackend('save_settings', {
      settings: { ...settings, confirmOnDelete: originalConfirm },
    });
  });

  // ── Restore defaults ──

  after(async () => {
    await invokeBackend('save_settings', {
      settings: {
        theme: { mode: 'dark', packId: null },
        language: 'zh-CN',
        limitSelectResults: true,
        queryResultLimit: 5000,
        editorFontSize: 13,
        editorFontFamily: 'Menlo, Monaco, Consolas, "Courier New", monospace',
        confirmOnDelete: true,
        autoCommit: true,
        safeMode: true,
        defaultPageSize: 50,
      },
    });
    await browser.pause(300);
    await browser.url('tauri://localhost');
    await browser.pause(1000);
  });
});
