/**
 * Extension I18n Bridge & Unified Translation API.
 *
 * Allows privileged extensions to register their own independent translation files
 * while reusing the host's current locale state, language switching events,
 * and translation engine.
 */

import { useSyncExternalStore } from 'react';

export interface HostLocaleBridge {
  getLocale: () => string;
  subscribe: (listener: (locale: string) => void) => () => void;
  /** Optional bridge to host's full translation engine (can translate both host and extension keys) */
  translate?: (key: string, params?: Record<string, string | number>) => string;
}

let activeHostLocaleBridge: HostLocaleBridge | null = null;
const globalListeners = new Set<(locale: string) => void>();

const registeredExtensionTranslations: Record<string, Record<string, string>> = {
  en: {},
  'zh-CN': {},
};

/**
 * Register extension translations for specific locales.
 * Can be called during extension initialization or at module level.
 */
export function registerTranslations(resources: Record<string, Record<string, string>>): void {
  for (const [locale, dict] of Object.entries(resources)) {
    if (!registeredExtensionTranslations[locale]) {
      registeredExtensionTranslations[locale] = {};
    }
    Object.assign(registeredExtensionTranslations[locale], dict);
  }
}

/**
 * Lookup an extension translation by key and locale.
 * Used by host getTranslation to resolve extension keys.
 */
export function getExtensionTranslation(
  locale: string,
  key: string,
  params?: Record<string, string | number>,
): string | undefined {
  const currentDict = registeredExtensionTranslations[locale];
  const defaultDict = registeredExtensionTranslations['en'];
  const raw = currentDict?.[key] ?? defaultDict?.[key];
  if (raw === undefined) return undefined;
  return formatI18nMessage(raw, params);
}

/**
 * Configure host locale bridge. Called by host entry (e.g. main.tsx).
 */
export function setHostLocaleBridge(bridge: HostLocaleBridge | null): void {
  activeHostLocaleBridge = bridge;
  if (bridge) {
    bridge.subscribe((locale) => {
      for (const listener of globalListeners) {
        listener(locale);
      }
    });
  }
}

/**
 * Get the current host locale ('en', 'zh-CN', etc.), falling back to fallbackLocale.
 */
export function getCurrentHostLocale(fallbackLocale = 'en'): string {
  return activeHostLocaleBridge?.getLocale() ?? fallbackLocale;
}

/**
 * Subscribe to host locale changes.
 */
export function subscribeHostLocale(listener: (locale: string) => void): () => void {
  globalListeners.add(listener);
  return () => {
    globalListeners.delete(listener);
  };
}

/**
 * Interpolate `{param}` tokens in a template string.
 */
export function formatI18nMessage(
  template: string,
  params?: Record<string, string | number>,
): string {
  if (!params) return template;
  return template.replace(/\{(\w+)\}/g, (_, key) => {
    const val = params[key];
    return val !== undefined ? String(val) : `{${key}}`;
  });
}

/**
 * Universal translation function.
 *
 * In host environment: delegates to host translation engine, which checks
 * both host dictionaries and registered extension dictionaries.
 *
 * In standalone test environment: resolves from registered extension dictionaries
 * without requiring host runtime or stores.
 */
export function t(key: string, params?: Record<string, string | number>): string {
  if (activeHostLocaleBridge?.translate) {
    return activeHostLocaleBridge.translate(key, params);
  }
  const locale = getCurrentHostLocale('en');
  const res = getExtensionTranslation(locale, key, params);
  return res ?? formatI18nMessage(key, params);
}

/**
 * Universal React hook for i18n in extensions and components.
 * Automatically re-renders when host language changes.
 */
export function useI18n() {
  const language = useSyncExternalStore(
    subscribeHostLocale,
    () => getCurrentHostLocale('en'),
    () => 'en',
  );

  return {
    t: (key: string, params?: Record<string, string | number>) => t(key, params),
    language,
  };
}
