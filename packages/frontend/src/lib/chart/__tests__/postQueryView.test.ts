import { beforeEach, describe, expect, it } from 'vitest';
import { useSettingsStore } from '../../../stores/settingsStore';
import type { ColumnInfo, StatementResult } from '../../../types';
import { resolvePostQueryViewMode } from '../postQueryView';

function col(name: string, dataType: string): ColumnInfo {
  return { name, dataType, nullable: true };
}

function chartable(): StatementResult {
  return {
    sql: '',
    columns: [col('day', 'text'), col('count', 'int4')],
    rows: [
      ['Mon', 1],
      ['Tue', 2],
    ],
    executionTimeMs: 1,
  };
}

describe('resolvePostQueryViewMode', () => {
  beforeEach(() => {
    useSettingsStore.setState({
      settings: {
        ...useSettingsStore.getState().settings,
        autoChartOnQuery: true,
      },
    });
  });

  it('returns chart when enabled and chartable', () => {
    expect(resolvePostQueryViewMode(chartable())).toBe('chart');
  });

  it('returns table when auto chart disabled', () => {
    useSettingsStore.setState({
      settings: {
        ...useSettingsStore.getState().settings,
        autoChartOnQuery: false,
      },
    });
    expect(resolvePostQueryViewMode(chartable())).toBe('table');
  });

  it('returns table when the setting is unset', () => {
    useSettingsStore.setState({
      settings: {
        ...useSettingsStore.getState().settings,
        autoChartOnQuery: undefined as unknown as boolean,
      },
    });
    expect(resolvePostQueryViewMode(chartable())).toBe('table');
  });
});
