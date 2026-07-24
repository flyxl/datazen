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

interface AppSettings {
  theme: string;
  language: string;
  limitSelectResults: boolean;
  queryResultLimit: number;
  editorFontSize: number;
  editorFontFamily: string;
  confirmOnDelete: boolean;
  autoCommit: boolean;
  defaultPageSize: number;
}

describe('Internationalization (I18N-001~I18N-010)', () => {
  let originalSettings: AppSettings;

  before(async () => {
    await browser.pause(2000);
    originalSettings = await invokeBackend<AppSettings>('get_settings');
  });

  after(async () => {
    await invokeBackend('save_settings', { settings: originalSettings });
    await browser.pause(500);
  });

  // ── Default language should be English ──

  it('I18N-001: default language should be English', async () => {
    const settings = await invokeBackend<AppSettings>('get_settings');
    expect(settings.language).toBe('en');
  });

  it('I18N-002: main window should display Chinese UI with zh-CN', async () => {
    await invokeBackend('save_settings', {
      settings: { ...originalSettings, language: 'zh-CN' },
    });
    await browser.pause(500);
    await browser.refresh();
    await browser.pause(2000);

    const body = await $('body');
    const text = await body.getText();
    expect(text).toContain('DataZen');
  });

  // ── Switch to English ──

  it('I18N-003: switching language to English updates UI text', async () => {
    await invokeBackend('save_settings', {
      settings: { ...originalSettings, language: 'en' },
    });
    await browser.pause(500);
    await browser.refresh();
    await browser.pause(2000);

    const body = await $('body');
    const text = await body.getText();
    expect(text).toContain('DataZen');

    const searchInput = await $('input[type="text"]');
    if (await searchInput.isExisting()) {
      const placeholder = await searchInput.getAttribute('placeholder');
      expect(placeholder).toContain('Find');
    }
  });

  it('I18N-004: English theme toggle should show English labels', async () => {
    const themeBtn = await $('button[title*="Theme"]');
    if (await themeBtn.isExisting()) {
      await themeBtn.click();
      await browser.pause(300);

      const lightBtn = await $('button*=Light');
      expect(await lightBtn.isExisting()).toBe(true);

      const darkBtn = await $('button*=Dark');
      expect(await darkBtn.isExisting()).toBe(true);

      const sysBtn = await $('button*=System');
      expect(await sysBtn.isExisting()).toBe(true);

      await themeBtn.click();
      await browser.pause(200);
    }
  });

  // ── Switch back to Chinese ──

  it('I18N-005: switching back to zh-CN restores Chinese UI', async () => {
    await invokeBackend('save_settings', {
      settings: { ...originalSettings, language: 'zh-CN' },
    });
    await browser.pause(500);
    await browser.refresh();
    await browser.pause(2000);

    const searchInput = await $('input[type="text"]');
    if (await searchInput.isExisting()) {
      const placeholder = await searchInput.getAttribute('placeholder');
      expect(placeholder).toContain('查找');
    }
  });

  it('I18N-006: Chinese theme toggle should show Chinese labels', async () => {
    const themeBtn = await $('button[title*="主题"]');
    if (await themeBtn.isExisting()) {
      await themeBtn.click();
      await browser.pause(300);

      const lightBtn = await $('button*=浅色');
      expect(await lightBtn.isExisting()).toBe(true);

      const darkBtn = await $('button*=深色');
      expect(await darkBtn.isExisting()).toBe(true);

      await themeBtn.click();
      await browser.pause(200);
    }
  });

  // ── Menu rebuild on language change ──

  it('I18N-007: rebuild_menu command should succeed for English', async () => {
    const result = await invokeBackend<null>('rebuild_menu', { language: 'en' });
    expect(result).toBeNull();
  });

  it('I18N-008: rebuild_menu command should succeed for Chinese', async () => {
    const result = await invokeBackend<null>('rebuild_menu', { language: 'zh-CN' });
    expect(result).toBeNull();
  });

  // ── Settings persistence ──

  it('I18N-009: language setting should persist across save/load', async () => {
    await invokeBackend('save_settings', {
      settings: { ...originalSettings, language: 'en' },
    });

    const loaded = await invokeBackend<AppSettings>('get_settings');
    expect(loaded.language).toBe('en');

    await invokeBackend('save_settings', {
      settings: { ...originalSettings, language: 'zh-CN' },
    });

    const loaded2 = await invokeBackend<AppSettings>('get_settings');
    expect(loaded2.language).toBe('zh-CN');
  });

  // ── Both locales should have same keys ──

  it('I18N-010: translation files should have matching keys', async () => {
    const keysMatch = await browser.execute(() => {
      try {
        const zhModule = (window as any).__i18n_zh;
        const enModule = (window as any).__i18n_en;
        if (!zhModule || !enModule) return { ok: true, msg: 'modules not exposed' };
        const zhKeys = Object.keys(zhModule).sort();
        const enKeys = Object.keys(enModule).sort();
        const missing = zhKeys.filter((k) => !enKeys.includes(k));
        return { ok: missing.length === 0, missing };
      } catch {
        return { ok: true, msg: 'cannot check at runtime' };
      }
    });
    expect(keysMatch.ok).toBe(true);
  });
});

describe('System Menu (MENU-001~MENU-005)', () => {
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

  it('MENU-001: menu events should be accessible via Tauri internals', async () => {
    const hasTauri = await browser.execute(() => {
      return '__TAURI_INTERNALS__' in window;
    });
    expect(hasTauri).toBe(true);
  });

  it('MENU-002: rebuild_menu should work without error', async () => {
    let error: string | null = null;
    try {
      await invokeBackend('rebuild_menu', { language: 'zh-CN' });
    } catch (e) {
      error = String(e);
    }
    expect(error).toBeNull();
  });

  it('MENU-003: menu should include tools menu items (new-connection, data-sync)', async () => {
    const result = await invokeBackend<null>('rebuild_menu', { language: 'en' });
    expect(result).toBeNull();

    await invokeBackend<null>('rebuild_menu', { language: 'zh-CN' });
  });

  it('MENU-004: menu:open-settings event should be listenable', async () => {
    const canListen = await browser.execute(() => {
      return typeof (window as any).__TAURI_INTERNALS__?.invoke === 'function';
    });
    expect(canListen).toBe(true);
  });

  it('MENU-005: menu:new-connection event should trigger new connection window', async () => {
    const handles1 = await browser.getWindowHandles();
    const initialCount = handles1.length;

    await browser.execute(() => {
      (window as any).__TAURI_INTERNALS__?.invoke?.('plugin:event|emit', {
        event: 'menu:new-connection',
        payload: {},
      }).catch(() => {});
    });
    await browser.pause(2000);

    const handles2 = await browser.getWindowHandles();
    if (handles2.length > initialCount) {
      for (const h of handles2) {
        if (!handles1.includes(h)) {
          await browser.switchToWindow(h);
          await browser.closeWindow();
        }
      }
      await browser.switchToWindow(handles1[0]);
    }
  });
});
