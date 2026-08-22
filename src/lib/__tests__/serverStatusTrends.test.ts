import { describe, expect, it } from 'vitest';
import {
  DEFAULT_MIN_WINDOW_MS,
  HISTORY_WINDOW_MS,
  cardRateFromHistory,
  computeTrendTimeAxis,
  formatByteRate,
  formatBytes,
  formatRate,
  latestSeriesValue,
  updateTrendHistory,
  type TrendPrevSample,
} from '../serverStatusTrends';

const emptyPrev: TrendPrevSample = { ts: 0, values: {} };

describe('updateTrendHistory', () => {
  it('seeds chart series from the first snapshot (counter/uptime) so charts appear with cards', () => {
    const result = updateTrendHistory({
      record: {
        uptimeSeconds: 100,
        questionsCounter: 500,
        xactCommitCounter: 300,
        xactRollbackCounter: 200,
        newSessionsCounter: 20,
        bytesInCounter: 1000,
        bytesOutCounter: 2000,
      },
      now: 5_000,
      prev: emptyPrev,
      history: {},
      statements: { select: 50, insert: 25, update: 10, delete: 5 },
    });

    // Each point carries its wall-clock timestamp + value.
    expect(result.history.qps).toEqual([{ t: 5000, v: 5 }]);
    expect(result.history.commits).toEqual([{ t: 5000, v: 3 }]);
    expect(result.history.rollbacks).toEqual([{ t: 5000, v: 2 }]);
    expect(result.history.sessions).toEqual([{ t: 5000, v: 0.2 }]);
    expect(result.history.netIn).toEqual([{ t: 5000, v: 10 }]);
    expect(result.history.netOut).toEqual([{ t: 5000, v: 20 }]);
    // statementCounters → per-command rate series (cumulative / uptime on seed).
    expect(result.history.cmd_select).toEqual([{ t: 5000, v: 0.5 }]);
    expect(result.history.cmd_insert).toEqual([{ t: 5000, v: 0.25 }]);
    // Charts should be visible with a single seeded point (no second poll required).
    expect((result.history.qps ?? []).length).toBeGreaterThanOrEqual(1);
  });

  it('records instantaneous gauges (session state) as-is', () => {
    const result = updateTrendHistory({
      record: { uptimeSeconds: 100, sessionTotal: 5, sessionActive: 2, sessionIdle: 3 },
      now: 5_000,
      prev: emptyPrev,
      history: {},
    });
    expect(result.history.sessionTotal).toEqual([{ t: 5000, v: 5 }]);
    expect(result.history.sessionActive).toEqual([{ t: 5000, v: 2 }]);
    expect(result.history.sessionIdle).toEqual([{ t: 5000, v: 3 }]);
  });

  it('uses delta/dt on subsequent samples (timestamped)', () => {
    const first = updateTrendHistory({
      record: { uptimeSeconds: 100, questionsCounter: 500 },
      now: 5_000,
      prev: emptyPrev,
      history: {},
    });

    const second = updateTrendHistory({
      record: { uptimeSeconds: 105, questionsCounter: 600 },
      now: 10_000,
      prev: first.prev,
      history: first.history,
    });

    // Δquestions=100 over 5s → 20/s
    expect(second.history.qps).toEqual([
      { t: 5000, v: 5 },
      { t: 10000, v: 20 },
    ]);
  });

  it('diffs statementCounters per command on subsequent samples', () => {
    const first = updateTrendHistory({
      record: { uptimeSeconds: 100 },
      now: 5_000,
      prev: emptyPrev,
      history: {},
      statements: { select: 100 },
    });
    const second = updateTrendHistory({
      record: { uptimeSeconds: 102 },
      now: 10_000,
      prev: first.prev,
      history: first.history,
      statements: { select: 150 },
    });
    // Δselect=50 over 5s → 10/s
    expect(second.history.cmd_select).toEqual([
      { t: 5000, v: 1 },
      { t: 10000, v: 10 },
    ]);
  });

  it('trims stale samples outside the 1h rolling window so the left edge stays pinned', () => {
    const start = 1_000_000;
    // Build history with points both inside and outside the window.
    const stale = { t: start - HISTORY_WINDOW_MS - 1000, v: 99 };
    const fresh = { t: start - 5000, v: 7 };
    const result = updateTrendHistory({
      record: { uptimeSeconds: 1, questionsCounter: 1 },
      now: start,
      prev: emptyPrev,
      history: { qps: [stale, fresh] },
    });
    // stale point dropped, fresh point kept (plus the new sample).
    const qps = result.history.qps!;
    expect(qps.some((p) => p.t === stale.t)).toBe(false);
    expect(qps.some((p) => p.t === fresh.t)).toBe(true);
  });

  it('aligns sample timestamps to the refresh period (integer multiples)', () => {
    const result = updateTrendHistory({
      record: { uptimeSeconds: 100, questionsCounter: 500 },
      // 17:20:23.4xxx → 对齐到 5s 整数倍 17:20:25
      now: new Date(2026, 0, 1, 17, 20, 23, 400).getTime(),
      prev: emptyPrev,
      history: {},
    });
    const t = result.history.qps![0].t;
    expect(t % 5000).toBe(0);
    // 目标：HH:mm:ss 的秒数应为 25（整数倍）。
    expect(new Date(t).getSeconds()).toBe(25);
  });

  it('skips series when counter is absent (data-driven hide)', () => {
    const result = updateTrendHistory({
      record: { uptimeSeconds: 100, version: 'x' },
      now: 5_000,
      prev: emptyPrev,
      history: {},
    });
    expect(result.history.qps).toBeUndefined();
    expect(result.history.sessions).toBeUndefined();
    expect(result.history.sessionTotal).toBeUndefined();
  });
});

describe('cardRateFromHistory / latestSeriesValue', () => {
  it('uses the same latest trend point as the chart for card QPS', () => {
    expect(
      cardRateFromHistory(
        {
          qps: [
            { t: 1, v: 1.2 },
            { t: 2, v: 5.5 },
          ],
        },
        'qps',
        '99.00',
      ),
    ).toBe('5.50');
  });

  it('falls back to snapshot value when history is empty', () => {
    expect(cardRateFromHistory({}, 'qps', '3.14')).toBe('3.14');
  });

  it('latestSeriesValue returns the final v', () => {
    expect(
      latestSeriesValue([
        { t: 1, v: 3 },
        { t: 2, v: 9 },
      ]),
    ).toBe(9);
    expect(latestSeriesValue([])).toBeNull();
  });
});

describe('format helpers', () => {
  it('formats rates compactly', () => {
    expect(formatRate(5)).toBe('5.00');
    expect(formatRate(500)).toBe('500.0');
    expect(formatRate(2500)).toBe('2.5K');
  });
  it('formats bytes with units', () => {
    expect(formatBytes(512)).toBe('512.0 B');
    expect(formatBytes(2048)).toBe('2.0 KB');
  });
  it('formats byte rates with a /s suffix', () => {
    expect(formatByteRate(24)).toBe('24.0 B/s');
    expect(formatByteRate(3380)).toBe('3.3 KB/s');
  });
});

describe('computeTrendTimeAxis', () => {
  const ref = new Date(2026, 0, 1, 17, 0, 0, 0).getTime(); // 17:00:00 整点

  it('grows from a 1-minute window on cold start (little data)', () => {
    // 只有约 2 个采样点（5s 间隔 → 数据跨度 ~10s）时，窗口放大到最小 1 分钟。
    const axis = computeTrendTimeAxis(ref - 10_000, ref);
    // 左缘 = 右缘 - 1 分钟（因为数据跨度 < 1 分钟）。
    expect(axis.domain[0]).toBe(ref - DEFAULT_MIN_WINDOW_MS);
    expect(axis.domain[1]).toBe(ref);
    // 1 分钟窗口 → 刻度步长也应是「漂亮」的小间隔（< 5 分钟）。
    const step = axis.ticks[1] - axis.ticks[0];
    expect(step).toBeLessThan(300_000);
  });

  it('expands the window as data accumulates toward 1 hour', () => {
    // 10 分钟数据 → 窗口 ≈ 10 分钟（> 1 分钟且 < 1 小时）。
    const axis = computeTrendTimeAxis(ref - 10 * 60_000, ref);
    const span = axis.domain[1] - axis.domain[0];
    expect(span).toBeGreaterThan(9 * 60_000);
    expect(span).toBeLessThanOrEqual(11 * 60_000);
    expect(span).toBeGreaterThan(DEFAULT_MIN_WINDOW_MS);
  });

  it('caps the window at 1 hour once data spans a full hour', () => {
    // 2 小时数据 → 窗口封顶 1 小时。
    const axis = computeTrendTimeAxis(ref - 2 * HISTORY_WINDOW_MS, ref);
    expect(axis.domain[1] - axis.domain[0]).toBe(HISTORY_WINDOW_MS);
    expect(axis.domain[0]).toBe(ref - HISTORY_WINDOW_MS);
  });

  it('places ticks on integer multiples of the step', () => {
    const axis = computeTrendTimeAxis(ref - 10 * 60_000, ref);
    const step = axis.ticks[1] - axis.ticks[0];
    for (const t of axis.ticks) {
      expect(t % step).toBe(0);
    }
    expect(axis.ticks[axis.ticks.length - 1]).toBeLessThanOrEqual(axis.domain[1]);
  });

  it('uses denser tick steps early and sparser steps as the window grows', () => {
    const small = computeTrendTimeAxis(ref - 60_000, ref);
    const medium = computeTrendTimeAxis(ref - 10 * 60_000, ref);
    const large = computeTrendTimeAxis(ref - 60 * 60_000, ref);
    const stepOf = (axis: { ticks: number[] }) => axis.ticks[1] - axis.ticks[0];
    // 前期间隔小（1 分钟窗口）<< 后期间隔大（1 小时窗口）。
    expect(stepOf(small)).toBeLessThan(stepOf(medium));
    expect(stepOf(medium)).toBeLessThan(stepOf(large));
  });
});
