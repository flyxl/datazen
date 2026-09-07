/**
 * Function registry: single source of truth for function completions,
 * signature help, and inlay hints.
 *
 * Common ANSI SQL functions are declared here.
 * Dialect-specific functions are contributed by database drivers via
 * DB_REGISTRY (`sqlFunctions` field in driver metadata) — zero hardcoded
 * driver dialect branches in host!
 */

import type { FunctionEntry, FunctionParam } from './sqlFunctionTypes';
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

/** Normalize a function name for case-insensitive lookup. */
function normalizeFunctionName(name: string): string {
  return name.toUpperCase();
}

/**
 * Find a function entry by name (case-insensitive).
 * When multiple entries share a name (e.g. CONCAT in common vs CONCAT in driver),
 * driver-specific entries take precedence.
 */
export function findFunctionEntry(name: string, dialectId?: string): FunctionEntry | undefined {
  const normalized = normalizeFunctionName(name);
  const driverFns = getDriverFunctions(dialectId);
  const driverMatch = driverFns.find((e) => normalizeFunctionName(e.name) === normalized);
  if (driverMatch) return driverMatch;

  return COMMON_FUNCTIONS.find((e) => normalizeFunctionName(e.name) === normalized);
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

/**
 * Count commas at the current parenthesis depth to determine the active
 * parameter index. Only counts commas that are direct children of the
 * target function call — nested calls, strings, and comments are skipped.
 *
 * @param text  Full SQL text from the start of the function call to cursor.
 * @param openParenOffset  Document offset of the opening '(' for this call.
 */
export function countCommasForParamIndex(text: string, openParenOffset: number): number {
  let depth = 0;
  let commas = 0;
  let inSingleQuote = false;
  let inDoubleQuote = false;
  let inBacktick = false;
  let inLineComment = false;
  let inBlockComment = false;
  let i = openParenOffset + 1;
  const len = text.length;

  while (i < len) {
    const ch = text[i]!;
    const next = i + 1 < len ? text[i + 1]! : '';

    if (inLineComment) {
      if (ch === '\n') inLineComment = false;
      i += 1;
      continue;
    }
    if (inBlockComment) {
      if (ch === '*' && next === '/') {
        inBlockComment = false;
        i += 2;
        continue;
      }
      i += 1;
      continue;
    }
    if (inSingleQuote) {
      if (ch === "'" && next === "'") {
        i += 2;
        continue;
      }
      if (ch === "'") inSingleQuote = false;
      i += 1;
      continue;
    }
    if (inDoubleQuote) {
      if (ch === '"') inDoubleQuote = false;
      i += 1;
      continue;
    }
    if (inBacktick) {
      if (ch === '`') inBacktick = false;
      i += 1;
      continue;
    }

    if (ch === "'") {
      inSingleQuote = true;
      i += 1;
      continue;
    }
    if (ch === '"') {
      inDoubleQuote = true;
      i += 1;
      continue;
    }
    if (ch === '`') {
      inBacktick = true;
      i += 1;
      continue;
    }
    if (ch === '-' && next === '-') {
      inLineComment = true;
      i += 2;
      continue;
    }
    if (ch === '/' && next === '*') {
      inBlockComment = true;
      i += 2;
      continue;
    }
    if (ch === '(') {
      depth += 1;
      i += 1;
      continue;
    }
    if (ch === ')') {
      if (depth === 0) break;
      depth -= 1;
      i += 1;
      continue;
    }
    if (ch === ',' && depth === 0) {
      commas += 1;
      i += 1;
      continue;
    }
    i += 1;
  }
  return commas;
}
