import { useEffect, useMemo, useState } from 'react';
import { invokeModulesList } from '../value-editors/JsonEditor';
import { hasRedisJson } from '../value-editors/hasRedisJson';

/**
 * Loaded Redis modules of one session, plus the create-key type list they
 * unlock (`ReJSON` only exists when the module is actually loaded).
 *
 * `null` means "still unknown" (nothing renders the extra entry yet), `[]` means
 * "MODULE LIST answered empty/unavailable" — the distinction keeps the create
 * dialog from flickering an option in and out.
 */

const BASE_CREATE_TYPES = ['string', 'hash', 'list', 'set', 'zset'];

export function buildCreateTypes(modules: string[] | null): string[] {
  return modules && hasRedisJson(modules) ? [...BASE_CREATE_TYPES, 'ReJSON'] : BASE_CREATE_TYPES;
}

export function useReJsonModules(dbSessionId: string): string[] | null {
  const [modules, setModules] = useState<string[] | null>(null);

  useEffect(() => {
    let cancelled = false;
    setModules(null);
    void invokeModulesList(dbSessionId)
      .then((list) => {
        if (!cancelled) setModules(list);
      })
      .catch(() => {
        if (!cancelled) setModules([]);
      });
    return () => {
      cancelled = true;
    };
  }, [dbSessionId]);

  return modules;
}

export function useCreateTypes(modules: string[] | null): string[] {
  return useMemo(() => buildCreateTypes(modules), [modules]);
}
