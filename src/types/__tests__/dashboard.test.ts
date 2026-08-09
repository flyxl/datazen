import { describe, expect, it } from 'vitest';
import { clampRefreshSec, DEFAULT_MONITOR_SETTINGS } from '../dashboard';

describe('clampRefreshSec', () => {
  it('enforces minimum 30', () => {
    expect(clampRefreshSec(5)).toBe(30);
    expect(clampRefreshSec(30)).toBe(30);
    expect(clampRefreshSec(120)).toBe(120);
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
