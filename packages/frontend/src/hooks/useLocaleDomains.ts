import { useEffect, useState } from 'react';
import { ensureLocaleDomains, type LazyDomain } from '../locales';
import { useSettingsStore } from '../stores/settingsStore';

/**
 * Ensure lazy i18n domain packs are loaded for the active language.
 * Call from feature windows (Data Sync, Transfer, Schema Diff, Workflows, Dashboard, MCP settings).
 *
 * The effective dependency is a stable string key over the sorted domain list,
 * NOT the `domains` array identity — callers pass inline literals like
 * `useLocaleDomains(['sync'])` and would otherwise re-run the effect on every
 * render (new array reference → setReady(false) → re-import flash loop).
 */
export function useLocaleDomains(domains: readonly LazyDomain[]): boolean {
  const language = useSettingsStore((s) => s.settings?.language) ?? 'en';
  const [ready, setReady] = useState(false);

  // Inline ['sync'] is a new array each render, but the key stays identical →
  // the effect below does not re-run.
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
    // `language` is included so switching language re-ensures the packs.
    // Intentionally NOT `domains` — only its stable string key.
  }, [language, domainsKey]);

  return ready;
}
