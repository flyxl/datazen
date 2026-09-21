/**
 * @datazen/ui i18n — the ONE translation engine shared by the host, all
 * database-driver UIs and all privileged extensions.
 *
 * Design (approved spec, track i18n-core):
 * - Module-private `currentLocale`, a per-locale entry registry and a
 *   listener set. No bridge, no host fallback, no second lookup engine.
 * - `setLocale` is called ONLY by the host (src/lib/localeSync.ts wires
 *   settingsStore.language → setLocale). Compilation cannot enforce this;
 *   a Wave 4 lint rule will.
 * - Everyone (host dictionaries, driver locale packs, extension
 *   dictionaries) feeds the same registry through `registerTranslations`.
 * - Lookup chain per key: `registry[locale] ?? registry['en'] ?? key`,
 *   then `{param}` interpolation.
 */

import { useSyncExternalStore } from 'react';

export type I18nParams = Record<string, string | number>;

const DEFAULT_LOCALE = 'en';

/** locale → (key → message). Built exclusively via registerTranslations(). */
const registry: Record<string, Record<string, string>> = {};

let currentLocale: string = DEFAULT_LOCALE;

const listeners = new Set<() => void>();

/**
 * Switch the active locale and notify all subscribers.
 * Host-only entry point (see header note).
 */
export function setLocale(locale: string): void {
  if (locale === currentLocale) return;
  currentLocale = locale;
  // Copy first: a listener may unsubscribe during notification.
  for (const listener of [...listeners]) {
    listener();
  }
}

/** Current active locale ('en', 'zh-CN', …). */
export function getLocale(): string {
  return currentLocale;
}

/**
 * The single registration entry point for translations: host dictionaries,
 * driver locale packs and extension resources all merge into the shared
 * per-locale registry here (later registrations win on key collisions).
 */
export function registerTranslations(resources: Record<string, Record<string, string>>): void {
  for (const [locale, dict] of Object.entries(resources)) {
    const target = registry[locale] ?? (registry[locale] = {});
    Object.assign(target, dict);
  }
}

/**
 * Read-only snapshot of the dictionary currently registered for `locale`
 * (host eager + lazy packs already loaded + every driver/extension pack that
 * registered itself). Returns a shallow copy: callers must never write back
 * into the registry through it. Unknown locale codes yield an empty object.
 *
 * Intended for tooling, export/templating and tests — not render paths (it
 * does not subscribe to locale changes).
 */
export function getRegisteredTranslations(locale: string): Record<string, string> {
  return { ...(registry[locale] ?? {}) };
}

/** Interpolate `{param}` tokens in a template string. */
function formatMessage(template: string, params?: I18nParams): string {
  if (!params) return template;
  return template.replace(/\{(\w+)\}/g, (match, name: string) => {
    const value = params[name];
    return value !== undefined ? String(value) : match;
  });
}

/**
 * Translate `key` in the active locale, falling back to 'en', then to the
 * raw key itself. `{param}` tokens are interpolated from `params`.
 */
export function t(key: string, params?: I18nParams): string {
  const raw = registry[currentLocale]?.[key] ?? registry[DEFAULT_LOCALE]?.[key];
  return formatMessage(raw ?? key, params);
}

function subscribeLocale(listener: () => void): () => void {
  listeners.add(listener);
  return () => {
    listeners.delete(listener);
  };
}

/**
 * Universal React hook for i18n (host components, driver UIs, extensions).
 * Re-renders the consumer when the active locale changes.
 */
export function useI18n(): { t: typeof t; language: string } {
  const language = useSyncExternalStore(subscribeLocale, getLocale, getLocale);
  return { t, language };
}
