import { describe, expect, it } from 'vitest';
import {
  clampRefreshSec,
  DEFAULT_MONITOR_SETTINGS,
  normalizeRefreshPolicy,
  shouldWarnRefreshSec,
} from '../dashboard';

describe('clampRefreshSec', () => {
  it('enforces minimum 30', () => {
    expect(clampRefreshSec(5)).toBe(30);
    expect(clampRefreshSec(30)).toBe(30);
    expect(clampRefreshSec(120)).toBe(120);
  });
});

describe('normalizeRefreshPolicy', () => {
  it('clamps interval refreshSec', () => {
    expect(normalizeRefreshPolicy({ mode: 'interval', refreshSec: 5 })).toEqual({
      mode: 'interval',
      refreshSec: 30,
    });
  });

  it('clears refreshSec for manual mode', () => {
    expect(normalizeRefreshPolicy({ mode: 'manual', refreshSec: 60 })).toEqual({
      mode: 'manual',
    });
  });
});

describe('shouldWarnRefreshSec', () => {
  it('warns below 60 seconds for interval mode', () => {
    expect(shouldWarnRefreshSec({ mode: 'interval', refreshSec: 45 })).toBe(true);
    expect(shouldWarnRefreshSec({ mode: 'interval', refreshSec: 120 })).toBe(false);
    expect(shouldWarnRefreshSec({ mode: 'manual' })).toBe(false);
  });
});

describe('DEFAULT_MONITOR_SETTINGS', () => {
  it('has safe defaults', () => {
    expect(DEFAULT_MONITOR_SETTINGS.maxConcurrentQueries).toBe(2);
    expect(DEFAULT_MONITOR_SETTINGS.runRetentionCount).toBe(200);
    expect(DEFAULT_MONITOR_SETTINGS.runRetentionDays).toBe(30);
    expect(DEFAULT_MONITOR_SETTINGS.exportIncludeDashboardRuns).toBe(true);
  });
});
