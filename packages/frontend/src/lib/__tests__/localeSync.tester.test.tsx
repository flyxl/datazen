/**
 * [tester] Coverage-driven branch tests for src/lib/localeSync.ts (track
 * i18n-core). Locks the defensive fallbacks and the no-change guard that the
 * coder suite left uncovered:
 *   - missing persisted language seeds the engine with 'en'
 *   - a settings change that does NOT touch language must not setLocale
 *   - stopping the sync handle unsubscribes
 *   - clearing language at runtime falls back to 'en' through the subscription
 */
import { describe, expect, it, afterEach } from 'vitest';
import { getLocale, setLocale } from '@datazen/ui';
import { startLocaleSync } from '../localeSync';
import { useSettingsStore } from '../../stores/settingsStore';
import type { AppSettings } from '../../types';

function setSettings(patch: Partial<AppSettings>): void {
  useSettingsStore.setState({
    settings: { ...useSettingsStore.getState().settings, ...patch },
  });
}

function setLanguage(language: unknown): void {
  setSettings({ language } as unknown as Partial<AppSettings>);
}

describe('[tester] localeSync defensive branches', () => {
  let stop: (() => void) | undefined;

  afterEach(() => {
    stop?.();
    stop = undefined;
    setLocale('en');
    setLanguage('en');
  });

  it('test_tester_seeds_en_when_persisted_language_is_missing', () => {
    setLanguage(undefined);
    stop = startLocaleSync();
    expect(getLocale()).toBe('en');
  });

  it('test_tester_unrelated_settings_change_does_not_touch_locale', () => {
    setLanguage('en');
    stop = startLocaleSync();
    expect(getLocale()).toBe('en');
    // Simulate an out-of-band engine change; the guard must NOT reset it back
    // when a settings mutation leaves `language` untouched.
    setLocale('fr');
    setSettings({
      connectionPoolSize: useSettingsStore.getState().settings.connectionPoolSize + 1,
    });
    expect(getLocale()).toBe('fr');
  });

  it('test_tester_stop_unsubscribes_language_wiring', () => {
    setLanguage('en');
    const handle = startLocaleSync();
    handle();
    setLanguage('zh-CN');
    expect(getLocale()).toBe('en');
  });

  it('test_tester_runtime_language_reset_falls_back_to_en', () => {
    setLanguage('zh-CN');
    stop = startLocaleSync();
    expect(getLocale()).toBe('zh-CN');
    setLanguage(undefined);
    expect(getLocale()).toBe('en');
  });
});
