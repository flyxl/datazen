import { useEffect, useRef, type MutableRefObject } from 'react';
import { getUrlParam } from './windowKind';
import type { ConnectionConfig } from '../types';

/** URL query params shared by Data Sync / Transfer / Schema Diff sub-windows. */
export interface MigrationWindowPrefill {
  sourceId?: string;
  targetId?: string;
  sourceDatabase?: string;
  targetDatabase?: string;
  sourceSchema?: string;
  targetSchema?: string;
}

const PREFILL_KEYS = [
  'sourceId',
  'targetId',
  'sourceDatabase',
  'targetDatabase',
  'sourceSchema',
  'targetSchema',
] as const satisfies readonly (keyof MigrationWindowPrefill)[];

export function readMigrationPrefillFromUrl(): MigrationWindowPrefill {
  const prefill: MigrationWindowPrefill = {};
  for (const key of PREFILL_KEYS) {
    const value = getUrlParam(key);
    if (value) prefill[key] = value;
  }
  return prefill;
}

export function buildMigrationWindowUrlParams(
  windowKind: 'data-sync' | 'data-transfer' | 'schema-diff',
  prefill?: MigrationWindowPrefill,
): Record<string, string> {
  const params: Record<string, string> = { window: windowKind };
  if (!prefill) return params;
  for (const key of PREFILL_KEYS) {
    const value = prefill[key];
    if (value) params[key] = value;
  }
  return params;
}

export function resolveDefaultDatabase(
  databases: string[] | null | undefined,
  preferred: string,
  prev: string,
): string {
  const list = databases ?? [];
  if (list.includes(preferred)) return preferred;
  if (prev && list.includes(prev)) return prev;
  return list[0] ?? '';
}

export function pickPrefillDatabase(
  prefillRef: MutableRefObject<MigrationWindowPrefill>,
  side: 'source' | 'target',
  databases: string[],
  fallback: (prev: string) => string,
  prev: string,
): string {
  const key = side === 'source' ? 'sourceDatabase' : 'targetDatabase';
  const prefill = prefillRef.current[key];
  if (prefill && Array.isArray(databases) && databases.includes(prefill)) {
    prefillRef.current = { ...prefillRef.current, [key]: undefined };
    return prefill;
  }
  return fallback(prev);
}

export function pickPrefillSchema(
  prefillRef: MutableRefObject<MigrationWindowPrefill>,
  side: 'source' | 'target',
  schemas: string[],
  fallback: (prev: string) => string,
  prev: string,
): string {
  const key = side === 'source' ? 'sourceSchema' : 'targetSchema';
  const prefill = prefillRef.current[key];
  if (prefill && Array.isArray(schemas) && schemas.includes(prefill)) {
    prefillRef.current = { ...prefillRef.current, [key]: undefined };
    return prefill;
  }
  return fallback(prev);
}

/** Apply connection ids from URL prefill once connections are loaded. */
export function useMigrationEndpointPrefill(
  connections: ConnectionConfig[],
  setSourceId: (id: string) => void,
  setTargetId: (id: string) => void,
): MutableRefObject<MigrationWindowPrefill> {
  const prefillRef = useRef(readMigrationPrefillFromUrl());
  const appliedRef = useRef(false);

  useEffect(() => {
    if (appliedRef.current || connections.length === 0) return;
    const { sourceId, targetId } = prefillRef.current;
    if (!sourceId && !targetId) return;

    appliedRef.current = true;
    if (sourceId && connections.some((c) => c.id === sourceId)) {
      setSourceId(sourceId);
    }
    if (targetId && connections.some((c) => c.id === targetId)) {
      setTargetId(targetId);
    }
  }, [connections, setSourceId, setTargetId]);

  return prefillRef;
}
