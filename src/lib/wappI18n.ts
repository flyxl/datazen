import { wappCommands } from '../commands/wapps';

/**
 * Wapp-provided translations for the `i18n.getString` bridge API.
 *
 * A wapp ships flat JSON dictionaries under `locales/<locale>.json`
 * (`{ "greet": "Hello" }`). The host resolves the key against the locale the
 * bridge was attached with, falling back to `en.json`, then to `null`
 * (rendered by the wapp however it likes). Lookup failures are soft: a
 * missing/broken dictionary yields `null`, never an error.
 */

const KEY_RE = /^[A-Za-z0-9][A-Za-z0-9._-]{0,127}$/;
const LOCALE_RE = /^[a-zA-Z]{2,3}(-[a-zA-Z0-9]{1,8})?$/;

type Dictionary = Record<string, string>;

const dictionaryCache = new Map<string, Dictionary | null>();

function readDictionary(wappId: string, locale: string): Promise<Dictionary | null> {
  const cacheKey = `${wappId}:${locale}`;
  const cached = dictionaryCache.get(cacheKey);
  if (cached !== undefined) return Promise.resolve(cached);
  return wappCommands
    .readWappFile(wappId, `locales/${locale}.json`)
    .then((bytes) => {
      let parsed: unknown;
      try {
        parsed = JSON.parse(new TextDecoder().decode(new Uint8Array(bytes)));
      } catch {
        dictionaryCache.set(cacheKey, null);
        return null;
      }
      if (!parsed || typeof parsed !== 'object' || Array.isArray(parsed)) {
        dictionaryCache.set(cacheKey, null);
        return null;
      }
      const dict: Dictionary = {};
      for (const [k, v] of Object.entries(parsed as Record<string, unknown>)) {
        if (typeof v === 'string') dict[k] = v;
      }
      dictionaryCache.set(cacheKey, dict);
      return dict;
    })
    .catch(() => {
      dictionaryCache.set(cacheKey, null);
      return null;
    });
}

/** Resolve one wapp translation key for `locale` (falls back to `en`). */
export async function resolveWappString(
  wappId: string,
  key: string,
  locale: string,
): Promise<string | null> {
  if (!KEY_RE.test(key) || !LOCALE_RE.test(locale)) return null;

  // Exact locale first (e.g. zh-CN), then base language (zh), then en.
  const candidates = [...new Set([locale, locale.split('-')[0], 'en'])];
  for (const candidate of candidates) {
    const dict = await readDictionary(wappId, candidate);
    const value = dict?.[key];
    if (typeof value === 'string') return value;
  }
  return null;
}

/** Backward compatibility alias. */
export const resolvePluginString = resolveWappString;

/** Test hook: drop cached dictionaries (they live for the webview session). */
export function clearWappI18nCacheForTests(): void {
  dictionaryCache.clear();
}

/** Backward compatibility alias. */
export const clearPluginI18nCacheForTests = clearWappI18nCacheForTests;
