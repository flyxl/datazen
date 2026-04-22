import { expect, browser, $ } from '@wdio/globals';

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
        theme: 'dark',
        language: 'zh-CN',
        limitSelectResults: true,
        queryResultLimit: 5000,
        editorFontSize: 13,
        editorFontFamily: 'Menlo, Monaco, Consolas, "Courier New", monospace',
        confirmOnDelete: true,
        autoCommit: true,
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
        theme: 'light',
        language: 'zh-CN',
        limitSelectResults: true,
        queryResultLimit: 5000,
        editorFontSize: 13,
        editorFontFamily: 'Menlo',
        confirmOnDelete: true,
        autoCommit: true,
        defaultPageSize: 50,
      },
    });

    const loaded = await invokeBackend<any>('get_settings');
    expect(loaded.theme).toBe('light');
  });

  it('SS-004: settings should persist page size', async () => {
    await invokeBackend('save_settings', {
      settings: {
        theme: 'dark',
        language: 'zh-CN',
        limitSelectResults: true,
        queryResultLimit: 5000,
        editorFontSize: 13,
        editorFontFamily: 'Menlo',
        confirmOnDelete: true,
        autoCommit: true,
        defaultPageSize: 100,
      },
    });

    const loaded = await invokeBackend<any>('get_settings');
    expect(loaded.defaultPageSize).toBe(100);
  });

  it('SS-005: settings should persist query result limit', async () => {
    await invokeBackend('save_settings', {
      settings: {
        theme: 'dark',
        language: 'zh-CN',
        limitSelectResults: true,
        queryResultLimit: 10000,
        editorFontSize: 13,
        editorFontFamily: 'Menlo',
        confirmOnDelete: true,
        autoCommit: true,
        defaultPageSize: 50,
      },
    });

    const loaded = await invokeBackend<any>('get_settings');
    expect(loaded.queryResultLimit).toBe(10000);
  });

  it('SS-006: settings should persist editor preferences', async () => {
    await invokeBackend('save_settings', {
      settings: {
        theme: 'dark',
        language: 'zh-CN',
        limitSelectResults: true,
        queryResultLimit: 5000,
        editorFontSize: 16,
        editorFontFamily: 'Fira Code',
        confirmOnDelete: false,
        autoCommit: false,
        defaultPageSize: 50,
      },
    });

    const loaded = await invokeBackend<any>('get_settings');
    expect(loaded.editorFontSize).toBe(16);
    expect(loaded.editorFontFamily).toBe('Fira Code');
    expect(loaded.confirmOnDelete).toBe(false);
    expect(loaded.autoCommit).toBe(false);
  });

  // ── Restore defaults ──

  after(async () => {
    await invokeBackend('save_settings', {
      settings: {
        theme: 'dark',
        language: 'zh-CN',
        limitSelectResults: true,
        queryResultLimit: 5000,
        editorFontSize: 13,
        editorFontFamily: 'Menlo, Monaco, Consolas, "Courier New", monospace',
        confirmOnDelete: true,
        autoCommit: true,
        defaultPageSize: 50,
      },
    });
    await browser.pause(300);
    await browser.refresh();
    await browser.pause(1000);
  });
});
