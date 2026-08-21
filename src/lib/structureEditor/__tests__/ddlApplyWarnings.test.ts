import { describe, expect, it } from 'vitest';
import {
  buildAlterApplyWarningMessage,
  isLargeTableEstimate,
  planHasRiskyAlter,
  shouldConfirmAlterApply,
} from '../ddlApplyWarnings';
import type { StructureChangePlan } from '../types';

const t = (key: string, params?: Record<string, string | number>) => {
  if (params) {
    return `${key}:${JSON.stringify(params)}`;
  }
  return key;
};

function plan(risks: Array<'additive' | 'destructive' | 'rewrite'>): StructureChangePlan {
  return {
    statements: risks.map((risk, i) => ({
      sql: `stmt ${i}`,
      summary: `summary ${i}`,
      risk,
    })),
  };
}

describe('ddlApplyWarnings', () => {
  it('detects risky alter statements', () => {
    expect(planHasRiskyAlter(plan(['additive']))).toBe(false);
    expect(planHasRiskyAlter(plan(['rewrite']))).toBe(true);
    expect(planHasRiskyAlter(plan(['destructive']))).toBe(true);
  });

  it('requires confirm for alter with rewrite or large table', () => {
    expect(
      shouldConfirmAlterApply({ mode: 'create', plan: plan(['rewrite']), estimatedRows: null }),
    ).toBe(false);
    expect(
      shouldConfirmAlterApply({ mode: 'alter', plan: plan(['additive']), estimatedRows: 50_000 }),
    ).toBe(false);
    expect(
      shouldConfirmAlterApply({ mode: 'alter', plan: plan(['rewrite']), estimatedRows: null }),
    ).toBe(true);
    expect(
      shouldConfirmAlterApply({
        mode: 'alter',
        plan: plan(['additive']),
        estimatedRows: 200_000,
      }),
    ).toBe(true);
  });

  it('builds warning message from plan metadata and row estimate', () => {
    const message = buildAlterApplyWarningMessage({
      t,
      plan: plan(['rewrite', 'destructive']),
      estimatedRows: 150_000,
    });
    expect(message).toContain('structEditor.ddlWarn.risky');
    expect(message).toContain('structEditor.ddlWarn.largeTable');
    expect(message).toContain('structEditor.ddlWarn.footer');
  });

  it('treats large row threshold inclusively', () => {
    expect(isLargeTableEstimate(99_999)).toBe(false);
    expect(isLargeTableEstimate(100_000)).toBe(true);
  });
});
