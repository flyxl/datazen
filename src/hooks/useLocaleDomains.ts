import { useEffect, useState } from 'react';
import { ensureLocaleDomains, type LazyDomain } from '../locales';
import { useSettingsStore } from '../stores/settingsStore';

/**
 * Ensure lazy i18n domain packs are loaded for the active language.
 * Call from feature windows (Data Sync, Transfer, Schema Diff, Workflows, Dashboard, MCP settings).
 *
 * `domains` may be an inline array literal — identity is ignored; only the ordered
 * domain list (joined) is used as the effect dependency so packs are not reloaded
 * on every render.
 *
 * @returns `ready` — false until the requested packs are in the registry.
 * Prefer gating lazy-copy UI with `LocaleDomainLoading` while false.
 */
export function useLocaleDomains(domains: readonly LazyDomain[]): boolean {
  const language = useSettingsStore((s) => s.settings?.language) ?? 'en';
  const [ready, setReady] = useState(false);

  // Stable string key: inline `['sync']` gets a new array each render, but the key does not.
  const domainsKey = [...domains].sort().join('\0');

  useEffect(() => {
    let cancelled = false;
    setReady(false);
    const list = domainsKey ? (domainsKey.split('\0') as LazyDomain[]) : [];
    void ensureLocaleDomains(language, list).then(() => {
      if (!cancelled) setReady(true);
    });
    return () => {
      cancelled = true;
    };
  }, [language, domainsKey]);

  return ready;
}
