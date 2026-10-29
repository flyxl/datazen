/**
 * i18n-core acceptance: settingsStore.language change → startLocaleSync
 * subscription → @datazen/ui setLocale → useI18n consumers re-render with
 * the new locale, resolved through the single shared lookup engine.
 */
import { describe, expect, it, afterEach } from 'vitest';
import { render, screen, act, cleanup } from '@testing-library/react';
import { getLocale, registerTranslations, setLocale, useI18n } from '@datazen/ui';
import { startLocaleSync } from '../localeSync';
import { useSettingsStore } from '../../stores/settingsStore';

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

describe('localeSync (host → shared engine wiring)', () => {
  afterEach(() => {
    cleanup();
    setLocale('en');
  });

  it('seeds the engine from the persisted language on start', () => {
    setLanguage('zh-CN');
    const stop = startLocaleSync();
    try {
      expect(getLocale()).toBe('zh-CN');
    } finally {
      stop();
    }
  });

  it('re-renders useI18n consumers when settingsStore.language changes', () => {
    setLanguage('en');
    const stop = startLocaleSync();
    try {
      render(<Probe />);
      expect(screen.getByTestId('probe').textContent).toBe('Language: en');
      act(() => setLanguage('zh-CN'));
      expect(getLocale()).toBe('zh-CN');
      expect(screen.getByTestId('probe').textContent).toBe('语言：中文');
      act(() => setLanguage('en'));
      expect(screen.getByTestId('probe').textContent).toBe('Language: en');
    } finally {
      stop();
    }
  });
});
