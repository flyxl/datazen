import {
  parseSqlParams,
  getParamLabel,
  type SqlParam,
  type SqlParamDialectPolicy,
} from '../../../lib/sqlBindParams';

export interface MissingParam {
  param: SqlParam;
  label: string;
}

/**
 * Checks whether all required bind parameters in the target SQL have values provided.
 * Works purely on Host parser (independent of whether Pro extension is active).
 */
export function findMissingParams(
  sql: string,
  rawValues?: Record<string, unknown> | null,
  policy?: SqlParamDialectPolicy,
): MissingParam[] {
  const params = parseSqlParams(sql, policy);
  if (params.length === 0) return [];

  const missing: MissingParam[] = [];
  for (const p of params) {
    const val =
      rawValues?.[p.stableId] ??
      rawValues?.[p.name] ??
      (p.ordinal !== undefined ? rawValues?.[String(p.ordinal)] : undefined) ??
      (p.name.startsWith('$') ? rawValues?.[p.name.slice(1)] : undefined) ??
      (p.name.startsWith('@') ? rawValues?.[p.name.slice(1)] : undefined) ??
      (p.name.startsWith(':') ? rawValues?.[p.name.slice(1)] : undefined);

    const isMissing =
      val === undefined || val === null || (typeof val === 'string' && val.trim() === '');

    if (isMissing) {
      missing.push({
        param: p,
        label: getParamLabel(p),
      });
    }
  }
  return missing;
}
