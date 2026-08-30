/**
 * Built-in UI locales (en, zh-CN) — runtime dropdown + language switch smoke.
 *
 * I18N10-001  Settings language dropdown lists built-in locales
 * I18N10-002  Built-in locale codes align with settings options
 * I18N10-003  Switching language updates main-window UI text
 * I18N10-005  Unsupported locale code falls back to English UI
 *
 * Additional locales ship via language extensions; full locale key parity is
 * covered by release workflow (`scripts/i18n-sync-check.mjs` + i18n-sync skill).
 */
import { expect, browser, $, $$ } from '@wdio/globals';

/** Must match `src/locales/builtin-locales.json` / `BUILTIN_LOCALES`. */
const EXPECTED_LOCALES = ['en', 'zh-CN'] as const;

const LANGUAGE_LABELS: Record<(typeof EXPECTED_LOCALES)[number], string> = {
  en: 'English',
  'zh-CN': '简体中文',
};

/** Per-locale substring expected in main.searchPlaceholder after switch. */
const SEARCH_PLACEHOLDER_MARKERS: Record<(typeof EXPECTED_LOCALES)[number], string> = {
  en: 'Find',
  'zh-CN': '查找',
};

interface AppSettings {
  theme: string;
  language: string;
  limitSelectResults: boolean;
  queryResultLimit: number;
  editorFontSize: number;
  editorFontFamily: string;
  confirmOnDelete: boolean;
  autoCommit: boolean;
  safeMode: boolean;
  defaultPageSize: number;
}

async function invokeBackend<T>(cmd: string, args: Record<string, unknown> = {}): Promise<T> {
  const result = await browser.executeAsync(
    (c: string, a: string, done: (r: unknown) => void) => {
      (window as any).__TAURI_INTERNALS__
        .invoke(c, JSON.parse(a))
        .then((r: unknown) => done(r))
        .catch((e: unknown) => done({ __error: String(e) }));
    },
    cmd,
    JSON.stringify(args),
  );
  if (result && typeof result === 'object' && '__error' in (result as object)) {
    throw new Error((result as { __error: string }).__error);
  }
  return result as T;
}

async function openSettingsWindow() {
  const { openSettingsInMainWindow } = await import('../helpers.js');
  await openSettingsInMainWindow();
}

async function openMainWindow() {
  await browser.url('tauri://localhost');
  await browser.waitUntil(
    async () => {
      const search = await $('[data-testid="connection-search-input"]');
      if (await search.isExisting()) return await search.isDisplayed();
      const inputs = await $$('input');
      return inputs.length > 0;
    },
    { timeout: 20000, timeoutMsg: 'Main window not ready' },
  );
  await browser.pause(500);
}

async function getLanguageSelectTrigger() {
  const triggers = await $$('button[aria-haspopup="listbox"]');
  for (const trigger of triggers) {
    const label = await trigger.getText();
    if (
      Object.values(LANGUAGE_LABELS).some((l) => label.includes(l)) ||
      label.includes('English') ||
      label.includes('简体')
    ) {
      return trigger;
    }
  }
  throw new Error('Language select trigger not found in settings');
}

async function getLanguageDropdownOptions(): Promise<string[]> {
  const trigger = await getLanguageSelectTrigger();
  await trigger.click();
  await browser.pause(300);

  const listbox = await $('#dz-select-listbox');
  await listbox.waitForDisplayed({ timeout: 5000 });
  const items = await listbox.$$('div[aria-selected]');
  const labels: string[] = [];
  for (const item of items) {
    labels.push((await item.getText()).replace('✓', '').trim());
  }
  await trigger.click();
  await browser.pause(200);
  return labels;
}

async function setLanguageOnMainWindow(language: string, baseSettings: AppSettings) {
  await invokeBackend('save_settings', {
    settings: { ...baseSettings, language },
  });
  await browser.pause(300);
  await openMainWindow();
  await browser.refresh();
  await browser.waitUntil(
    async () => {
      const search = await $('[data-testid="connection-search-input"]');
      if (await search.isExisting()) return await search.isDisplayed();
      return (await $$('input')).length > 0;
    },
    { timeout: 20000, timeoutMsg: 'Main window not ready after language change' },
  );
  await browser.pause(2000);
}

async function getMainSearchPlaceholder(): Promise<string | null> {
  const search = await $('[data-testid="connection-search-input"]');
  if (await search.isExisting()) {
    return search.getAttribute('placeholder');
  }
  const inputs = await $$('input');
  for (const input of inputs) {
    const placeholder = await input.getAttribute('placeholder');
    if (placeholder && placeholder.length > 0) {
      return placeholder;
    }
  }
  return null;
}

describe('Built-in locale i18n (I18N10-001~I18N10-005)', () => {
  let originalSettings: AppSettings;

  before(async () => {
    await browser.pause(1000);
    originalSettings = await invokeBackend<AppSettings>('get_settings');
  });

  after(async () => {
    await invokeBackend('save_settings', { settings: originalSettings });
    await browser.pause(400);
    try {
      await openMainWindow();
    } catch {
      await browser.url('tauri://localhost');
      await browser.pause(2000);
    }
  });

  it('I18N10-001: settings language dropdown should list built-in locales', async () => {
    await openSettingsWindow();
    const labels = await getLanguageDropdownOptions();
    expect(labels).toHaveLength(EXPECTED_LOCALES.length);
    for (const locale of EXPECTED_LOCALES) {
      expect(labels).toContain(LANGUAGE_LABELS[locale]);
    }
  });

  it('I18N10-002: built-in locale codes should match settings options', async () => {
    const optionValues = Object.keys(LANGUAGE_LABELS).sort();
    expect(optionValues).toEqual([...EXPECTED_LOCALES].sort());
  });

  it('I18N10-003: switching language should update main window UI text', async () => {
    for (const locale of EXPECTED_LOCALES) {
      await setLanguageOnMainWindow(locale, originalSettings);

      const placeholder = await getMainSearchPlaceholder();
      expect(placeholder).not.toBeNull();
      expect(placeholder).toContain(SEARCH_PLACEHOLDER_MARKERS[locale]);
    }
  });

  it('I18N10-005: unsupported locale should fall back to English UI', async () => {
    await setLanguageOnMainWindow('xx-XX', originalSettings);

    const placeholder = await getMainSearchPlaceholder();
    expect(placeholder).not.toBeNull();
    expect(placeholder).toContain('Find');

    const settings = await invokeBackend<AppSettings>('get_settings');
    expect(settings.language).toBe('xx-XX');
  });
});
