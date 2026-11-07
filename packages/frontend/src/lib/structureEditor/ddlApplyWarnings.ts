import type { TranslationKey } from '../../locales';
import type { StructureChangeMode, StructureChangePlan } from './types';

/** Row estimate at or above this threshold triggers an online DDL warning. */
export const LARGE_TABLE_ROW_THRESHOLD = 100_000;

export function planHasRiskyAlter(plan: StructureChangePlan): boolean {
  return plan.statements.some((s) => s.risk === 'destructive' || s.risk === 'rewrite');
}

export function isLargeTableEstimate(rows: number | null | undefined): boolean {
  return rows != null && rows >= LARGE_TABLE_ROW_THRESHOLD;
}

export function shouldConfirmAlterApply(args: {
  mode: StructureChangeMode;
  plan: StructureChangePlan;
  estimatedRows?: number | null;
}): boolean {
  if (args.mode !== 'alter') return false;
  return planHasRiskyAlter(args.plan) || isLargeTableEstimate(args.estimatedRows);
}

export function buildAlterApplyWarningMessage(args: {
  t: (key: TranslationKey, params?: Record<string, string | number>) => string;
  plan: StructureChangePlan;
  estimatedRows?: number | null;
}): string {
  const parts: string[] = [];
  if (planHasRiskyAlter(args.plan)) {
    const risks = [
      ...new Set(
        args.plan.statements
          .map((s) => s.risk)
          .filter((r) => r === 'destructive' || r === 'rewrite'),
      ),
    ];
    parts.push(args.t('structEditor.ddlWarn.risky', { risks: risks.join(', ') }));
  }
  if (isLargeTableEstimate(args.estimatedRows)) {
    parts.push(args.t('structEditor.ddlWarn.largeTable', { rows: args.estimatedRows as number }));
  }
  parts.push(args.t('structEditor.ddlWarn.footer'));
  return parts.join('\n\n');
}
