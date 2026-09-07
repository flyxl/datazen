import { useCallback, useMemo, useRef, useState } from 'react';
import {
  buildBindPayloadV2,
  parseSqlParams,
  SENSITIVE_PARAM_NAMES,
  paramFingerprint,
  getParamLabel,
  type SqlParam,
  type SqlBindPayloadV2,
  type SqlParamDialectPolicy,
} from '../../../lib/sqlBindParams';

// ---------------------------------------------------------------------------
// History
// ---------------------------------------------------------------------------

const HISTORY_STORAGE_KEY = 'datazen-bind-param-history';
const HISTORY_MAX_ENTRIES = 5;

export interface ParamHistoryEntry {
  value: string;
  timestamp: number;
}

type ParamHistoryStore = Record<string, ParamHistoryEntry[]>;

function readHistory(): ParamHistoryStore {
  try {
    const raw = localStorage.getItem(HISTORY_STORAGE_KEY);
    if (!raw) return {};
    const parsed = JSON.parse(raw);
    if (parsed && typeof parsed === 'object' && !Array.isArray(parsed)) {
      return parsed as ParamHistoryStore;
    }
    return {};
  } catch {
    // localStorage unavailable, quota exceeded, or corrupted JSON → silent degradation
    return {};
  }
}

function writeHistory(store: ParamHistoryStore): void {
  try {
    localStorage.setItem(HISTORY_STORAGE_KEY, JSON.stringify(store));
  } catch {
    // silent degradation
  }
}

function isSensitiveParam(name: string): boolean {
  const lower = name.toLowerCase();
  return SENSITIVE_PARAM_NAMES.has(lower);
}

function appendToHistory(
  store: ParamHistoryStore,
  stableId: string,
  paramName: string,
  value: string,
): ParamHistoryStore {
  if (isSensitiveParam(paramName) || value === '') return store;
  const existing = store[stableId] ?? [];
  const deduped = existing.filter((e) => e.value !== value);
  const next: ParamHistoryEntry = { value, timestamp: Date.now() };
  const updated = [next, ...deduped].slice(0, HISTORY_MAX_ENTRIES);
  return { ...store, [stableId]: updated };
}

// ---------------------------------------------------------------------------
// Hook
// ---------------------------------------------------------------------------

export interface UseBindParametersResult {
  /** Current param descriptors for the given SQL. */
  params: SqlParam[];
  /** Map from stableId → display label. */
  labels: Record<string, string>;
  /** Current param values keyed by stable ID. */
  values: Record<string, string>;
  /** Update a single param value. */
  setValue: (stableId: string, value: string) => void;
  /** Build v2 wire payload for a target SQL string. */
  buildPayloadForTarget: (targetSql: string) => SqlBindPayloadV2;
  /**
   * Record current snapshot to history before actual submit.
   * Only S5-C calls this before execution.
   */
  markSubmitted: (snapshot: Record<string, string>) => void;
  /** Stable fingerprint of current params (for S5-C). */
  fingerprint: string;
  /** History entries for a specific param stableId. */
  getHistory: (stableId: string) => ParamHistoryEntry[];
  /** Clear history for a specific param stableId. */
  clearHistory: (stableId: string) => void;
  /** Clear all param history. */
  clearAllHistory: () => void;
}

export function useBindParameters(
  sql: string,
  policy?: SqlParamDialectPolicy,
): UseBindParametersResult {
  const params = useMemo(() => parseSqlParams(sql, policy), [sql, policy]);
  const [values, setValues] = useState<Record<string, string>>({});
  const historyRef = useRef<ParamHistoryStore>(readHistory());

  // When SQL changes, keep surviving values; drop stale keys.
  // Typed as Set<string> for easier compatibility with Object.keys() etc.
  const stableIds = useMemo(() => new Set<string>(params.map((p) => p.stableId)), [params]);
  const [valuesSnapshot, setValuesSnapshot] = useState<Record<string, string>>({});
  const prevStableIdsRef = useRef<Set<string>>(new Set());

  // Reconcile values when params change: keep values for stable IDs that
  // survived the SQL change, drop those that disappeared.
  if (params.length > 0 && !setsEqual(stableIds, prevStableIdsRef.current)) {
    prevStableIdsRef.current = stableIds;
    const reconciled: Record<string, string> = {};
    for (const id of stableIds) {
      reconciled[id] = values[id] ?? valuesSnapshot[id] ?? '';
    }
    if (JSON.stringify(reconciled) !== JSON.stringify(valuesSnapshot)) {
      setValuesSnapshot(reconciled);
    }
  }

  const effectiveValues = useMemo(() => {
    const merged = { ...valuesSnapshot };
    for (const id of Object.keys(values)) {
      if (stableIds.has(id)) {
        merged[id] = values[id];
      }
    }
    return merged;
  }, [values, valuesSnapshot, stableIds]);

  const labels = useMemo(() => {
    const map: Record<string, string> = {};
    for (const p of params) {
      map[p.stableId] = getParamLabel(p);
    }
    return map;
  }, [params]);

  const fingerprint = useMemo(() => paramFingerprint(params), [params]);

  const setValue = useCallback((stableId: string, value: string) => {
    setValues((prev) => ({ ...prev, [stableId]: value }));
  }, []);

  const buildPayloadForTarget = useCallback(
    (targetSql: string): SqlBindPayloadV2 => {
      return buildBindPayloadV2(targetSql, effectiveValues, policy);
    },
    [effectiveValues, policy],
  );

  const markSubmitted = useCallback(
    (snapshot: Record<string, string>) => {
      const paramsForSnapshot = parseSqlParams(sql, policy);
      let store = historyRef.current;
      for (const p of paramsForSnapshot) {
        const raw = snapshot[p.stableId] ?? snapshot[p.name] ?? '';
        if (raw !== '') {
          store = appendToHistory(store, p.stableId, p.name, raw);
        }
      }
      historyRef.current = store;
      writeHistory(store);
    },
    [sql, policy],
  );

  const getHistory = useCallback((stableId: string): ParamHistoryEntry[] => {
    return historyRef.current[stableId] ?? [];
  }, []);

  const clearHistory = useCallback((stableId: string) => {
    const store = { ...historyRef.current };
    delete store[stableId];
    historyRef.current = store;
    writeHistory(store);
  }, []);

  const clearAllHistory = useCallback(() => {
    historyRef.current = {};
    writeHistory({});
  }, []);

  return {
    params,
    labels,
    values: effectiveValues,
    setValue,
    buildPayloadForTarget,
    markSubmitted,
    fingerprint,
    getHistory,
    clearHistory,
    clearAllHistory,
  };
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

function setsEqual(a: Set<string>, b: Set<string>): boolean {
  if (a.size !== b.size) return false;
  for (const v of a) {
    if (!b.has(v)) return false;
  }
  return true;
}
