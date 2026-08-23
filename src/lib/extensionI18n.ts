import { pluginCommands } from '../commands/plugins';

/**
 * Plugin-provided translations for the `i18n.getString` bridge API.
 *
 * An extension ships flat JSON dictionaries under `locales/<locale>.json`
 * (`{ "greet": "Hello" }`). The host resolves the key against the locale the
 * bridge was attached with, falling back to `en.json`, then to `null`
 * (rendered by the plugin however it likes). Lookup failures are soft: a
 * missing/broken dictionary yields `null`, never an error.
 */

const KEY_RE = /^[A-Za-z0-9][A-Za-z0-9._-]{0,127}$/;
const LOCALE_RE = /^[a-zA-Z]{2,3}(-[a-zA-Z0-9]{1,8})?$/;

type Dictionary = Record<string, string>;

const dictionaryCache = new Map<string, Dictionary | null>();

function readDictionary(pluginId: string, locale: string): Promise<Dictionary | null> {
  const cacheKey = `${pluginId}:${locale}`;
  const cached = dictionaryCache.get(cacheKey);
  if (cached !== undefined) return Promise.resolve(cached);
  return pluginCommands
    .readPluginFile(pluginId, `locales/${locale}.json`)
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

/** Resolve one plugin translation key for `locale` (falls back to `en`). */
export async function resolvePluginString(
  pluginId: string,
  key: string,
  locale: string,
): Promise<string | null> {
  if (!KEY_RE.test(key) || !LOCALE_RE.test(locale)) return null;

  // Exact locale first (e.g. zh-CN), then base language (zh), then en.
  const candidates = [...new Set([locale, locale.split('-')[0], 'en'])];
  for (const candidate of candidates) {
    const dict = await readDictionary(pluginId, candidate);
    const value = dict?.[key];
    if (typeof value === 'string') return value;
  }
  return null;
}

/** Test hook: drop cached dictionaries (they live for the webview session). */
export function clearPluginI18nCacheForTests(): void {
  dictionaryCache.clear();
}
