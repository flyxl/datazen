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
 * - The last resort (the raw key) is reported once per key in dev builds:
 *   see {@link reportMissingKey}.
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
 * Remove a locale's dictionary entirely (external language pack uninstall).
 *
 * Callers must switch the active locale away from `locale` first (the host
 * flow falls back to `en` before removing the pack). If the locale is still
 * active when this runs, it defensively falls back to 'en' and notifies —
 * the en dictionary is always present, so `t()` never renders raw keys.
 */
export function unregisterTranslations(locale: string): void {
  if (!Object.prototype.hasOwnProperty.call(registry, locale)) return;
  // eslint-disable-next-line @typescript-eslint/no-dynamic-delete
  delete registry[locale];
  if (currentLocale === locale) {
    currentLocale = DEFAULT_LOCALE;
    for (const listener of [...listeners]) {
      listener();
    }
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
 * Keys already reported by {@link reportMissingKey}, so a key rendered on
 * every table row costs exactly one console line instead of one per render.
 * Deliberately module-private and never cleared: a missing key is a
 * development-time defect, not a per-navigation event, so reporting it once
 * for the lifetime of the page is the whole point.
 */
const reportedMissingKeys = new Set<string>();

/**
 * Dev-only report for the last-resort branch of {@link t}.
 *
 * Falling back to the raw key keeps the UI renderable, but it is otherwise
 * invisible: a typo, a dictionary entry nobody wrote, or a locale pack that
 * failed to load all paint the same thing — an English key string
 * (`common.close`) where a sentence belongs. At the scale of the locale
 * registry — two host locales eagerly, four lazy domain packs loaded on
 * demand, plus every driver and extension pack self-registering into the same
 * table — that class of bug accumulates silently and is only ever caught by
 * reading the screen.
 *
 * Gated on `import.meta.env.DEV`, the same dev/prod discriminator the rest of
 * the host uses (e.g. `src/lib/globalTextSelection.ts`). Vite folds it to a
 * literal at build time, so a production bundle has no live branch, no
 * allocation and no `Set` traffic: the cost is one predictable comparison on a
 * path that already performed two dictionary lookups.
 */
function reportMissingKey(key: string): void {
  if (!import.meta.env.DEV) return;
  if (reportedMissingKeys.has(key)) return;
  reportedMissingKeys.add(key);
  const locales =
    currentLocale === DEFAULT_LOCALE
      ? `"${DEFAULT_LOCALE}"`
      : `"${currentLocale}" nor for "${DEFAULT_LOCALE}"`;
  console.warn(
    `[i18n] Missing translation for key "${key}": not registered for ${locales}, ` +
      'so the raw key is rendered. Add it to the owning locale pack ' +
      '(en is the source of truth).',
  );
}

/**
 * Translate `key` in the active locale, falling back to 'en', then to the
 * raw key itself. `{param}` tokens are interpolated from `params`.
 */
export function t(key: string, params?: I18nParams): string {
  const message = registry[currentLocale]?.[key] ?? registry[DEFAULT_LOCALE]?.[key];
  if (message === undefined) {
    // Neither the active locale nor 'en' knows this key. An empty string is a
    // real translation, not a miss, so the check is against `undefined` only.
    reportMissingKey(key);
    return formatMessage(key, params);
  }
  return formatMessage(message, params);
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
