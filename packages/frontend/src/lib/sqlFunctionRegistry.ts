/**
 * Function registry: single source of truth for function completions,
 * signature help, and inlay hints.
 *
 * Common ANSI SQL functions are declared here.
 * Dialect-specific functions are contributed by database drivers via
 * DB_REGISTRY (`sqlFunctions` field in driver metadata) — zero hardcoded
 * driver dialect branches in host!
 */

import type { FunctionEntry, FunctionParam } from '@datazen/extension-points';
import { DB_REGISTRY } from './databaseTypes';
import type { DatabaseType } from '../types';

export type { FunctionEntry, FunctionParam };

/* -------------------------------------------------------------------------- */
/*  Common ANSI-SQL functions                                                 */
/* -------------------------------------------------------------------------- */

export const COMMON_FUNCTIONS: readonly FunctionEntry[] = [
  {
    name: 'COUNT',
    description: 'Count rows or non-null values',
    params: [{ name: 'expr', type: 'any' }],
  },
  {
    name: 'SUM',
    description: 'Sum of numeric values',
    params: [{ name: 'expr', type: 'number' }],
  },
  {
    name: 'AVG',
    description: 'Average of numeric values',
    params: [{ name: 'expr', type: 'number' }],
  },
  {
    name: 'MIN',
    description: 'Minimum value',
    params: [{ name: 'expr', type: 'any' }],
  },
  {
    name: 'MAX',
    description: 'Maximum value',
    params: [{ name: 'expr', type: 'any' }],
  },
  {
    name: 'COALESCE',
    description: 'First non-null value',
    params: [
      { name: 'expr1', type: 'any' },
      { name: 'expr2', type: 'any' },
    ],
  },
  {
    name: 'NULLIF',
    description: 'NULL if two values are equal',
    params: [
      { name: 'value1', type: 'any' },
      { name: 'value2', type: 'any' },
    ],
  },
  {
    name: 'CAST',
    description: 'Convert value to another type',
    params: [{ name: 'expr', type: 'any' }],
  },
  {
    name: 'CONCAT',
    description: 'Concatenate strings',
    params: [
      { name: 'str1', type: 'string' },
      { name: 'str2', type: 'string' },
    ],
  },
];

/* -------------------------------------------------------------------------- */
/*  Driver dialect function resolution via DB_REGISTRY                        */
/* -------------------------------------------------------------------------- */

/** Dialect id aliases that map to canonical driver types in DB_REGISTRY. */
const DIALECT_ALIASES: Record<string, string> = {
  postgres: 'postgresql',
  pg: 'postgresql',
  cockroach: 'postgresql',
  mariadb: 'mysql',
  tidb: 'mysql',
  mssql: 'sqlserver',
};

function resolveDialectFamily(dialectId: string): string {
  const lower = dialectId.toLowerCase();
  return DIALECT_ALIASES[lower] ?? lower;
}

/**
 * Load dialect-specific functions contributed by driver packages via DB_REGISTRY.
 */
export function getDriverFunctions(dialectId?: string): readonly FunctionEntry[] {
  if (!dialectId) {
    // Collect all functions contributed across active drivers in DB_REGISTRY
    const allDriverFns: FunctionEntry[] = [];
    const seenKeys = new Set<string>();
    for (const meta of Object.values(DB_REGISTRY)) {
      if (meta?.sqlFunctions) {
        for (const fn of meta.sqlFunctions) {
          const key = `${fn.name.toUpperCase()}::${fn.params.length}`;
          if (!seenKeys.has(key)) {
            seenKeys.add(key);
            allDriverFns.push(fn);
          }
        }
      }
    }
    return allDriverFns;
  }

  const normalized = dialectId.toLowerCase();
  const canonical = resolveDialectFamily(normalized);

  // 1. Direct match in DB_REGISTRY by driver type
  const directMeta =
    DB_REGISTRY[canonical as DatabaseType] ?? DB_REGISTRY[normalized as DatabaseType];
  if (directMeta?.sqlFunctions && directMeta.sqlFunctions.length > 0) {
    return directMeta.sqlFunctions;
  }

  // 2. Match by sqlDialect family in DB_REGISTRY
  for (const [id, meta] of Object.entries(DB_REGISTRY)) {
    if (meta?.sqlDialect?.toLowerCase() === canonical || id.toLowerCase() === canonical) {
      if (meta?.sqlFunctions && meta.sqlFunctions.length > 0) {
        return meta.sqlFunctions;
      }
    }
  }

  return [];
}

/**
 * Get all function entries matching the given dialect.
 * Merges ANSI COMMON functions with dialect functions contributed by driver packages.
 */
export function getFunctionEntries(dialectId?: string): readonly FunctionEntry[] {
  const driverFns = getDriverFunctions(dialectId);
  return [...COMMON_FUNCTIONS, ...driverFns];
}

/**
 * Build CodeMirror Completion objects from the registry, filtered by dialect.
 */
export function buildFunctionCompletions(
  dialectId?: string,
): { label: string; type: 'function'; detail: string; apply: string }[] {
  return getFunctionEntries(dialectId).map((e) => ({
    label: e.name,
    type: 'function' as const,
    detail: e.description,
    apply: `${e.name}(`,
  }));
}
