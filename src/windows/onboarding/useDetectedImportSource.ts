import { useEffect, useState } from 'react';
import { connectionCommands } from '../../commands/connection';
import type { ConnectionImportApp } from '../../components/connection/ConnectionShareDialog';

/**
 * Pre-check order for the "import connections" entry card. The first client
 * whose config is found on this machine is offered as the default import
 * source (Q6: detection over known config paths only).
 */
export const IMPORT_APP_ORDER: readonly ConnectionImportApp[] = [
  'dbeaver',
  'datagrip',
  'navicat',
  'tableplus',
  'dbx',
];

export interface DetectedImportSource {
  /** First installed client detected on this machine, `null` when none was found. */
  detected: ConnectionImportApp | null;
  /** Every client whose config was found, in {@link IMPORT_APP_ORDER} order. */
  detectedApps: readonly ConnectionImportApp[];
  loading: boolean;
}

/**
 * Detect which supported client config exists locally. Failures degrade to
 * "nothing detected" — the entry card then shows the generic copy and the user
 * can still pick a file.
 */
export function useDetectedImportSource(enabled = true): DetectedImportSource {
  const [detectedApps, setDetectedApps] = useState<readonly ConnectionImportApp[]>([]);
  const [loading, setLoading] = useState(enabled);

  useEffect(() => {
    if (!enabled) {
      setDetectedApps([]);
      setLoading(false);
      return;
    }
    let cancelled = false;
    setLoading(true);
    void Promise.allSettled(
      IMPORT_APP_ORDER.map((app) => connectionCommands.detectConnectionImportPath(app)),
    )
      .then((results) => {
        if (cancelled) return;
        const hits = IMPORT_APP_ORDER.filter(
          (_, i) => results[i]?.status === 'fulfilled' && results[i].value?.found === true,
        );
        setDetectedApps(hits);
      })
      .finally(() => {
        if (!cancelled) setLoading(false);
      });
    return () => {
      cancelled = true;
    };
  }, [enabled]);

  return { detected: detectedApps[0] ?? null, detectedApps, loading };
}
