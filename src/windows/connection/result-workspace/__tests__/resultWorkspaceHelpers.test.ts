import { describe, expect, it } from 'vitest';
import type { ColumnInfo, StatementResult } from '../../../../types';
import { DEFAULT_CHART_CONFIG } from '../../../../types/chart';
import {
  canRenderResultChart,
  resolveResultWorkspaceView,
} from '../resultWorkspaceHelpers';

function column(name: string, dataType: string): ColumnInfo {
  return { name, dataType, nullable: true };
}

function result(overrides: Partial<StatementResult> = {}): StatementResult {
  return {
    sql: 'select 1',
    columns: [column('label', 'text'), column('amount', 'int4')],
    rows: [['one', 1]],
    executionTimeMs: 1,
    ...overrides,
  };
}

describe('result workspace view helpers', () => {
  it('keeps table view explicit and reports chart capability separately', () => {
    expect(resolveResultWorkspaceView(result(), 'table', DEFAULT_CHART_CONFIG)).toEqual({
      view: 'table',
      chartAvailable: true,
    });
  });

  it('allows chart view only for a chartable result with caller-owned config', () => {
    expect(resolveResultWorkspaceView(result(), 'chart', DEFAULT_CHART_CONFIG)).toEqual({
      view: 'chart',
      chartAvailable: true,
    });
    expect(canRenderResultChart(result(), DEFAULT_CHART_CONFIG)).toBe(true);
  });

  it('falls back to table for an empty result', () => {
    expect(resolveResultWorkspaceView(result({ rows: [] }), 'chart', DEFAULT_CHART_CONFIG)).toEqual({
      view: 'table',
      chartAvailable: false,
      fallbackReason: 'empty-result',
    });
  });

  it('falls back to table when chart config is missing', () => {
    expect(resolveResultWorkspaceView(result(), 'chart', undefined)).toEqual({
      view: 'table',
      chartAvailable: false,
      fallbackReason: 'missing-chart-config',
    });
  });

  it('falls back to table when no numeric field can be inferred', () => {
    const textOnly = result({
      columns: [column('label', 'text'), column('category', 'text')],
      rows: [['one', 'a']],
    });
    expect(resolveResultWorkspaceView(textOnly, 'chart', DEFAULT_CHART_CONFIG)).toEqual({
      view: 'table',
      chartAvailable: false,
      fallbackReason: 'not-chartable',
    });
  });

  it('treats a missing active statement as an empty workspace', () => {
    expect(resolveResultWorkspaceView(undefined, 'chart', DEFAULT_CHART_CONFIG)).toEqual({
      view: 'table',
      chartAvailable: false,
      fallbackReason: 'empty-result',
    });
  });
});
