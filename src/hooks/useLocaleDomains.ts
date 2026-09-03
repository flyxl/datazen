import { useEffect, useState } from 'react';
import { ensureLocaleDomains, type LazyDomain } from '../locales';
import { useSettingsStore } from '../stores/settingsStore';

/**
 * Ensure lazy i18n domain packs are loaded for the active language.
 * Call from feature windows (Data Sync, Transfer, Schema Diff, Workflows, Dashboard, MCP settings).
 */
export function useLocaleDomains(domains: readonly LazyDomain[]) {
  const language = useSettingsStore((s) => s.settings.language) ?? 'en';
  const [ready, setReady] = useState(false);

  useEffect(() => {
    let cancelled = false;
    setReady(false);
    void ensureLocaleDomains(language, domains).then(() => {
      if (!cancelled) setReady(true);
    });
    return () => {
      cancelled = true;
    };
  }, [language, domains]);

  return ready;
}
