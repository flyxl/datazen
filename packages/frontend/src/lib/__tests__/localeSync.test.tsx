/**
 * i18n-core acceptance: settingsStore.language change → startLocaleSync
 * subscription → @datazen/ui setLocale → useI18n consumers re-render with
 * the new locale, resolved through the single shared lookup engine.
 *
 * Language packs: the persisted language may be provided by an installed
 * pack, so `startLocaleSync` ensures the pack's messages BEFORE switching
 * (async microtask). These tests pin that ordering plus the
 * last-writer-wins guard for fast consecutive switches.
 */
import { describe, expect, it, afterEach, beforeEach, vi } from 'vitest';
import { render, screen, act, cleanup, waitFor } from '@testing-library/react';
import { getLocale, registerTranslations, setLocale, useI18n } from '@datazen/ui';
import { startLocaleSync } from '../localeSync';
import { useSettingsStore } from '../../stores/settingsStore';
import {
  __resetLanguagePackRuntimeForTests,
  ensureLanguagePackMessages,
} from '../languagePackRuntime';
import { unregisterLocale } from '../../locales';

const { readLanguagePackMock } = vi.hoisted(() => ({
  readLanguagePackMock: vi.fn(),
}));

vi.mock('../../commands/languagePacks', () => ({
  LANGUAGE_PACKS_CHANGED_EVENT: 'language-packs:changed',
  languagePackCommands: {
    readLanguagePack: (...args: unknown[]) => readLanguagePackMock(...args),
  },
}));

registerTranslations({
  en: { 'localeSync.probe': 'Language: en' },
  'zh-CN': { 'localeSync.probe': '语言：中文' },
});

function Probe() {
  const { t } = useI18n();
  return <span data-testid="probe">{t('localeSync.probe')}</span>;
}

function setLanguage(language: string): void {
  useSettingsStore.setState({
    settings: { ...useSettingsStore.getState().settings, language },
  });
}

/** Flush the async ensure → setLocale chain (microtask + waitFor). */
async function flushLocale(): Promise<void> {
  await act(async () => {
    await Promise.resolve();
  });
}

describe('localeSync (host → shared engine wiring)', () => {
  afterEach(() => {
    cleanup();
    setLocale('en');
    unregisterLocale('ja');
    __resetLanguagePackRuntimeForTests();
    readLanguagePackMock.mockReset();
  });

  it('seeds the engine from the persisted language on start', async () => {
    setLanguage('zh-CN');
    const stop = startLocaleSync();
    try {
      await waitFor(() => expect(getLocale()).toBe('zh-CN'));
    } finally {
      stop();
    }
  });

  it('re-renders useI18n consumers when settingsStore.language changes', async () => {
    setLanguage('en');
    const stop = startLocaleSync();
    try {
      render(<Probe />);
      await waitFor(() => expect(screen.getByTestId('probe').textContent).toBe('Language: en'));
      await act(async () => {
        setLanguage('zh-CN');
        await Promise.resolve();
      });
      await waitFor(() => expect(getLocale()).toBe('zh-CN'));
      expect(screen.getByTestId('probe').textContent).toBe('语言：中文');
      await act(async () => {
        setLanguage('en');
        await Promise.resolve();
      });
      await waitFor(() => expect(screen.getByTestId('probe').textContent).toBe('Language: en'));
    } finally {
      stop();
    }
  });

  it('registers pack messages before switching to a pack-backed locale', async () => {
    readLanguagePackMock.mockResolvedValue({
      code: 'ja',
      label: '日本語',
      messages: { 'localeSync.probe': '言語：ja' },
    });
    setLanguage('en');
    const stop = startLocaleSync();
    try {
      render(<Probe />);
      await act(async () => {
        setLanguage('ja');
        await Promise.resolve();
      });
      // The switch only happens after the pack read resolves.
      await waitFor(() => expect(getLocale()).toBe('ja'));
      expect(screen.getByTestId('probe').textContent).toBe('言語：ja');
      expect(readLanguagePackMock).toHaveBeenCalledWith('ja');
    } finally {
      stop();
    }
  });

  it('a slow pack read cannot override a later language switch (last writer wins)', async () => {
    let resolveRead: (value: unknown) => void = () => {};
    readLanguagePackMock.mockImplementation(
      () =>
        new Promise((resolve) => {
          resolveRead = resolve;
        }),
    );
    setLanguage('en');
    const stop = startLocaleSync();
    try {
      await act(async () => {
        setLanguage('ja');
        await Promise.resolve();
      });
      // Switch again while the ja read is still in flight.
      await act(async () => {
        setLanguage('en');
        await Promise.resolve();
      });
      resolveRead({
        code: 'ja',
        label: '日本語',
        messages: { 'localeSync.probe': '言語：ja' },
      });
      await flushLocale();
      // The stale ja result must NOT flip the locale back.
      expect(getLocale()).toBe('en');
    } finally {
      stop();
    }
  });
});

describe('ensureLanguagePackMessages', () => {
  beforeEach(() => {
    __resetLanguagePackRuntimeForTests();
    unregisterLocale('ja');
    readLanguagePackMock.mockReset();
  });

  it('registers pack messages through the extension-locale channel', async () => {
    readLanguagePackMock.mockResolvedValue({
      code: 'ja',
      label: '日本語',
      messages: { 'runtime.probe': '了解' },
    });
    await ensureLanguagePackMessages('ja');
    expect(readLanguagePackMock).toHaveBeenCalledWith('ja');
    // Registered → a second call makes no further IPC.
    readLanguagePackMock.mockClear();
    await ensureLanguagePackMessages('ja');
    expect(readLanguagePackMock).not.toHaveBeenCalled();
  });

  it('is a no-op for builtin locales and uninstalled codes', async () => {
    await ensureLanguagePackMessages('en');
    await ensureLanguagePackMessages('zh-CN');
    readLanguagePackMock.mockRejectedValue(new Error('language pack not found: de'));
    await ensureLanguagePackMessages('de');
    expect(readLanguagePackMock).toHaveBeenCalledTimes(1);
  });
});
