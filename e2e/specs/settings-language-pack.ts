/**
 * Host E2E: external language pack install journey (Settings → General →
 * Language packs).
 *
 * Fixture: `e2e/fixtures/language-packs/ja.zip` (small ja pack — manifest +
 * settings.json + core.json).
 *
 * Journey:
 *   LP-001 install via webdriver path override → the pack appears in the
 *        language selector (the feature's core acceptance)
 *   LP-002 switching to 日本語 renders fixture translations in Settings
 *   LP-003 removing the *active* pack falls back to English (builtin), the
 *        dropdown entry disappears, and the setting persists as 'en'
 */
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { expect, browser, $ } from '@wdio/globals';
import {
  backFromSettingsInMainWindow,
  captureJourneyStep,
  closeExtraWindows,
  openSettingsInMainWindow,
} from '../helpers.js';

const THIS_DIR = path.dirname(fileURLToPath(import.meta.url));
/** Absolute path handed to the webdriver-only install override. */
const FIXTURE_ZIP = path.resolve(THIS_DIR, '..', 'fixtures', 'language-packs', 'ja.zip');

async function invokeBackend<T>(cmd: string, args: Record<string, unknown> = {}): Promise<T> {
  const result = await browser.executeAsync(
    (c: string, a: string, done: (r: unknown) => void) => {
      (
        window as unknown as {
          __TAURI_INTERNALS__: { invoke: (c: string, a: unknown) => Promise<unknown> };
        }
      ).__TAURI_INTERNALS__
        .invoke(c, JSON.parse(a))
        .then((r: unknown) => done(r))
        .catch((e: unknown) => done({ __error: String(e) }));
    },
    cmd,
    JSON.stringify(args),
  );
  if (result && typeof result === 'object' && '__error' in (result as Record<string, unknown>)) {
    throw new Error((result as { __error: string }).__error);
  }
  return result as T;
}

/** Pick an option in the language selector by its trigger test id. */
async function pickLanguageOption(optionLabel: string) {
  await browser.execute(() => {
    const btn = document.querySelector(
      '[data-testid="settings-language-select"]',
    ) as HTMLElement | null;
    if (!btn) throw new Error('language select trigger not found');
    btn.click();
  });
  await browser.waitUntil(
    async () =>
      browser.execute(() => Boolean(document.querySelector('[data-testid="select-listbox"]'))),
    { timeout: 5000, timeoutMsg: 'language select listbox did not open' },
  );
  await browser.execute((option: string) => {
    const list = document.querySelector('[data-testid="select-listbox"]');
    if (!list) throw new Error('select listbox not open');
    const item = Array.from(list.children).find((el) => (el.textContent || '').includes(option));
    if (!item) throw new Error(`language option not found: ${option}`);
    item.dispatchEvent(new MouseEvent('mousedown', { bubbles: true, cancelable: true }));
  }, optionLabel);
  await browser.pause(300);
}

/** The visible text of the settings section title (locale probe). */
async function settingsTitleText(): Promise<string> {
  const el = await $('[data-testid="settings-section-general"] h2');
  return el.getText();
}

async function removePackIfInstalled(): Promise<void> {
  try {
    const packs = await invokeBackend<{ code: string }[]>('list_language_packs');
    for (const pack of packs ?? []) {
      if (pack.code === 'ja') {
        await invokeBackend('remove_language_pack', { code: 'ja' });
      }
    }
  } catch {
    // Not installed — nothing to clean up.
  }
}

describe('Settings language packs (LP-001~LP-003)', () => {
  before(async () => {
    const mainWindow = await browser.getWindowHandle();
    await closeExtraWindows(mainWindow);
    await browser.switchToWindow(mainWindow);
    await removePackIfInstalled();
    // Start from a deterministic builtin language.
    await invokeBackend('save_host_settings', {
      settings: { language: 'zh-CN' },
    });
    await openSettingsInMainWindow('general');
  });

  after(async () => {
    try {
      await removePackIfInstalled();
      await invokeBackend('save_host_settings', { settings: { language: 'zh-CN' } });
      await backFromSettingsInMainWindow();
    } catch {
      // best-effort cleanup
    }
  });

  it('LP-001: installs the fixture pack and the language selector offers it', async () => {
    const summary = await invokeBackend<{ code: string; label: string; messageCount: number }>(
      'install_language_pack',
      { pickToken: null, overridePath: FIXTURE_ZIP },
    );
    expect(summary.code).toBe('ja');
    expect(summary.messageCount).toBe(5);

    await captureJourneyStep('lp-001-installed');

    // The broadcast refreshes the store; the card lists the pack …
    const row = await $('[data-testid="language-pack-row-ja"]');
    await row.waitForDisplayed({ timeout: 8000 });
    // … and the language selector offers 日本語.
    await pickLanguageOption('日本語');
    await captureJourneyStep('lp-001-selector-offers-ja');
  });

  it('LP-002: switching to 日本語 renders the fixture translations', async () => {
    // The pack is now the active language (selected in LP-001).
    await browser.pause(600);
    await expect(await settingsTitleText()).toBe('全般');
    const inUse = await $('[data-testid="language-pack-active-ja"]');
    await inUse.waitForDisplayed({ timeout: 5000 });
    await captureJourneyStep('lp-002-ja-rendered');
  });

  it('LP-003: removing the active pack falls back to English and de-lists it', async () => {
    await captureJourneyStep('lp-003-before-remove');
    const removeBtn = await $('[data-testid="language-pack-remove-ja"]');
    await removeBtn.waitForDisplayed({ timeout: 5000 });
    await removeBtn.click();

    const confirm = await $('[data-testid="language-pack-remove-confirm"]');
    await confirm.waitForDisplayed({ timeout: 5000 });
    await confirm.click();

    // The interface falls back to English …
    await browser.waitUntil(async () => (await settingsTitleText()) === 'General', {
      timeout: 8000,
      timeoutMsg: 'interface did not fall back to English after pack removal',
    });
    await captureJourneyStep('lp-003-english-fallback');

    // … the persisted language is en …
    const settings = await invokeBackend<{ language?: string }>('get_host_settings');
    expect(settings.language).toBe('en');

    // … the card row is gone …
    const row = await $('[data-testid="language-pack-row-ja"]');
    await expect(await row.isExisting()).toBe(false);

    // … and the selector no longer offers 日本語.
    await browser.execute(() => {
      const btn = document.querySelector(
        '[data-testid="settings-language-select"]',
      ) as HTMLElement | null;
      if (!btn) throw new Error('language select trigger not found');
      btn.click();
    });
    await browser.waitUntil(
      async () =>
        browser.execute(() => Boolean(document.querySelector('[data-testid="select-listbox"]'))),
      { timeout: 5000, timeoutMsg: 'language select listbox did not open' },
    );
    const hasJa = await browser.execute(() => {
      const list = document.querySelector('[data-testid="select-listbox"]');
      return Array.from(list?.children ?? []).some((el) =>
        (el.textContent || '').includes('日本語'),
      );
    });
    expect(hasJa).toBe(false);
    await captureJourneyStep('lp-003-removed');
  });
});
