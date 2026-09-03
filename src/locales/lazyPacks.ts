/**
 * Runtime registry for lazy locale domains.
 *
 * `t()` / getTranslation stay synchronous: they read whatever packs have already
 * been merged into the registry. Feature windows call {@link ensureLocaleDomains}
 * on mount so the needed packs are present before the UI renders labels.
 */
import type { BuiltinLocale } from './builtinLocales';
import { LAZY_DOMAINS, type LazyDomain } from './domains';

type Pack = Record<string, string>;

const registry: Partial<Record<BuiltinLocale, Partial<Record<LazyDomain, Pack>>>> = {};
const inflight: Partial<Record<BuiltinLocale, Partial<Record<LazyDomain, Promise<Pack>>>>> = {};

type Loader = () => Promise<{ default: Pack }>;

const loaders: Record<BuiltinLocale, Record<LazyDomain, Loader>> = {
  en: {
    sync: () => import('./en/sync'),
    transfer: () => import('./en/transfer'),
    schemaDiff: () => import('./en/schemaDiff'),
    workflows: () => import('./en/workflows'),
    dashboard: () => import('./en/dashboard'),
    mcp: () => import('./en/mcp'),
  },
  'zh-CN': {
    sync: () => import('./zh-CN/sync'),
    transfer: () => import('./zh-CN/transfer'),
    schemaDiff: () => import('./zh-CN/schemaDiff'),
    workflows: () => import('./zh-CN/workflows'),
    dashboard: () => import('./zh-CN/dashboard'),
    mcp: () => import('./zh-CN/mcp'),
  },
};

function isBuiltin(locale: string): locale is BuiltinLocale {
  return locale === 'en' || locale === 'zh-CN';
}

/** Synchronously read a key from already-loaded lazy packs (or undefined). */
export function lookupLazyTranslation(locale: string, key: string): string | undefined {
  if (!isBuiltin(locale)) return undefined;
  const packs = registry[locale];
  if (!packs) return undefined;
  for (const d of LAZY_DOMAINS) {
    const text = packs[d]?.[key];
    if (text !== undefined) return text;
  }
  return undefined;
}

/** Whether a lazy domain is already in the registry for this locale. */
export function isDomainLoaded(locale: string, domain: LazyDomain): boolean {
  if (!isBuiltin(locale)) return false;
  return Boolean(registry[locale]?.[domain]);
}

/**
 * Ensure the given lazy domains are loaded for a locale.
 * Safe to call multiple times; concurrent callers share the same promise.
 */
export async function ensureLocaleDomains(
  locale: string,
  domains: readonly LazyDomain[] = LAZY_DOMAINS,
): Promise<void> {
  if (!isBuiltin(locale)) return;
  const tasks: Promise<Pack>[] = [];
  for (const domain of domains) {
    if (registry[locale]?.[domain]) continue;
    let p = inflight[locale]?.[domain];
    if (!p) {
      p = loaders[locale][domain]().then((m) => {
        const pack = m.default as Pack;
        if (!registry[locale]) registry[locale] = {};
        registry[locale]![domain] = pack;
        if (inflight[locale]) delete inflight[locale]![domain];
        return pack;
      });
      if (!inflight[locale]) inflight[locale] = {};
      inflight[locale]![domain] = p;
    }
    tasks.push(p);
  }
  await Promise.all(tasks);
}

/** Preload every lazy domain for a locale (tests / optional warm-up). */
export async function ensureAllLazyDomains(locale: string): Promise<void> {
  await ensureLocaleDomains(locale, LAZY_DOMAINS);
}

/** Test helper: clear lazy registry. */
export function __resetLazyPacksForTests(): void {
  for (const k of Object.keys(registry)) delete registry[k as BuiltinLocale];
  for (const k of Object.keys(inflight)) delete inflight[k as BuiltinLocale];
}
