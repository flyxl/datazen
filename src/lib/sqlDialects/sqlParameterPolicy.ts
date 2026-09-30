import { DB_REGISTRY } from '../databaseTypes';
import type { SqlDialectProfile, SqlParameterPolicy, SqlParameterStrategy } from './types';
import type { SqlParamDialectPolicy } from '../sqlBindParams';

interface SqlParameterMeta {
  sqlParameterPolicy?: SqlParameterPolicy;
  sqlDialectProfile?: Pick<SqlDialectProfile, 'parameterPolicy'>;
  sqlParameterStrategy?: SqlParameterStrategy;
}

const GENERIC_SQL_PARAMETER_POLICY: SqlParamDialectPolicy = {
  enableAt: false,
  enableQuestion: false,
  enableDollarPositional: false,
  enableDollarNamed: false,
  enableTemplate: true,
};

/** Resolve bind scanning from the selected driver's own metadata. */
export function resolveSqlParameterPolicy(
  databaseType?: string,
  registry: Record<string, SqlParameterMeta> = DB_REGISTRY,
): SqlParamDialectPolicy {
  const key = databaseType?.trim().toLowerCase();
  const meta = key ? registry[key] : undefined;
  const nativePolicy = meta?.sqlParameterPolicy ?? meta?.sqlDialectProfile?.parameterPolicy;

  if (!nativePolicy) {
    return { ...GENERIC_SQL_PARAMETER_POLICY, strategy: meta?.sqlParameterStrategy };
  }

  return {
    enableAt: nativePolicy.atNamed ?? false,
    enableQuestion: nativePolicy.question ?? false,
    enableDollarPositional: nativePolicy.dollarPositional ?? false,
    enableDollarNamed: nativePolicy.dollarNamed ?? false,
    enableTemplate: nativePolicy.template ?? false,
    strategy: meta?.sqlParameterStrategy,
  };
}
