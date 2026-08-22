/**
 * Server dashboard trend series: derive per-second rates from cumulative
 * counters returned by `server_status_snapshot` (and instantaneous gauges), and
 * keep a *time-windowed* rolling series that feeds both stat cards and charts.
 *
 * Each sample is stamped with its wall-clock timestamp so charts render a real
 * time axis (`HH:mm:ss`) instead of an incrementing integer index. Series are
 * trimmed to a rolling window of `HISTORY_WINDOW_MS` (1 hour): once the window
 * fills, the oldest points drop out and the left edge stays pinned to
 * "now − 1 hour", so the chart behaves like a fixed-width scrolling window
 * instead of growing without bound / shifting its left start point.
 *
 * First sample seeds rate series as counter/uptime (same basis as lifetime
 * values on cards) so charts can render together with cards from the same
 * snapshot — no wait for a second poll.
 *
 * The series catalog is *data-driven*: a series only appears when its backing
 * field is present in the snapshot. Host does not branch on driver type.
 */

export const HISTORY_WINDOW_MS = 60 * 60 * 1000; // 1 小时
/** 防御性点数封顶（1h / 最小 5s 间隔 = 720；更大间隔点更少）。 */
export const HISTORY_MAX = 720;
/** 冷启动最小显示窗口：数据不足时也从「最近 1 分钟」起展示。 */
export const DEFAULT_MIN_WINDOW_MS = 60 * 1000;
/** 默认刷新周期：用于把采样时间对齐到周期的整数倍（如 5s → 17:20:25）。 */
export const DEFAULT_REFRESH_PERIOD_MS = 5000;

/**
 * 从「最近 1 分钟」起、随数据积累逐步拉大到「最近 1 小时」的刻度步长集合。
 * 越小窗口走更密的轴刻度，越大窗口走更疏的刻度。
 */
export const TICK_STEP_CANDIDATES_MS = [
  1_000, // 1s
  5_000, // 5s
  10_000, // 10s
  30_000, // 30s
  60_000, // 1m
  120_000, // 2m
  300_000, // 5m
  600_000, // 10m
  900_000, // 15m
  1_800_000, // 30m
  3_600_000, // 1h
] as const;

/**
 * 目标刻度数量：窗口越小越密（前期显示更多时间档），窗口越大越疏。
 * 1 分钟窗口约 10 档，1 小时窗口约 4 档，线性过渡。
 */
export function desiredTickCount(windowMs: number): number {
  const w = Math.max(windowMs, 0);
  const MIN_W = DEFAULT_MIN_WINDOW_MS; // 1 分钟
  const MAX_W = HISTORY_WINDOW_MS; // 1 小时
  const DENSE = 10;
  const SPARSE = 4;
  const t = Math.min(Math.max((w - MIN_W) / (MAX_W - MIN_W), 0), 1);
  return Math.round(DENSE + (SPARSE - DENSE) * t);
}

/** 取一个「漂亮」的刻度步长，让窗口内刻度数与窗口大小成正比。 */
export function pickTickStep(windowMs: number): number {
  const target = Math.max(windowMs, 0) / desiredTickCount(windowMs);
  for (const s of TICK_STEP_CANDIDATES_MS) {
    if (s >= target) return s;
  }
  return TICK_STEP_CANDIDATES_MS[TICK_STEP_CANDIDATES_MS.length - 1];
}

export interface TrendTimeAxis {
  domain: [number, number];
  ticks: number[];
}

/**
 * 计算服务器趋势图的时间窗口：
 * 窗口右缘＝最新采样、左缘＝取「数据实际跨度」与 1h/1min 上下限之间的值，
 * 因此冷启动时从 1 分钟开始，随数据积累逐步拉长到 1 小时。
 * `ticks` 随窗口长度自适应选步长，并落在步长的整数倍上。
 */
export function computeTrendTimeAxis(
  dataEarliest: number,
  lastSample: number,
  opts?: { minWindowMs?: number; maxWindowMs?: number },
): TrendTimeAxis {
  const minWindow = opts?.minWindowMs ?? DEFAULT_MIN_WINDOW_MS;
  const maxWindow = opts?.maxWindowMs ?? HISTORY_WINDOW_MS;
  const span = lastSample - dataEarliest;
  const window = Math.min(Math.max(span, minWindow), maxWindow);
  const left = lastSample - window;
  const right = lastSample;

  const step = pickTickStep(window);
  const first = Math.ceil(left / step) * step;
  const ticks: number[] = [];
  for (let t = first; t <= right; t += step) ticks.push(t);
  return { domain: [left, right], ticks };
}

/**
 * 把 wall-clock ms 对齐到 `periodMs` 的整数倍（就近取整，如 5s → 17:20:25），
 * 避免出现 17:20:23 这类非整数倍时间点。
 */
export function alignTimestamp(now: number, periodMs = DEFAULT_REFRESH_PERIOD_MS): number {
  if (periodMs <= 0) return now;
  return Math.round(now / periodMs) * periodMs;
}

/** A single trend sample with its wall-clock timestamp (ms). */
export interface TrendPoint {
  t: number;
  v: number;
}
export type TrendSeries = TrendPoint[];

/** Series kind. */
export type TrendKind = 'rate' | 'gauge';

/**
 * How one trend series is sourced from a status snapshot.
 * - `rate`: cumulative counter → Δcounter/Δt (first sample seeds counter/uptime).
 * - `gauge`: instantaneous value stored as-is (sessions / threads / etc.).
 */
export interface TrendSeriesDef {
  kind: TrendKind;
  /** Counter field (rate series). */
  counter?: string;
  /** Instantaneous field (gauge series). */
  field?: string;
}

/**
 * Trend series catalog keyed by stable chart key. Each maps to a single snapshot
 * field. Drivers expose these fields opportunistically; missing → series hidden.
 */
export const TREND_SERIES: Record<string, TrendSeriesDef> = {
  // Per-second rates (cumulative counters diffed over time).
  qps: { kind: 'rate', counter: 'questionsCounter' },
  commits: { kind: 'rate', counter: 'xactCommitCounter' },
  rollbacks: { kind: 'rate', counter: 'xactRollbackCounter' },
  sessions: { kind: 'rate', counter: 'newSessionsCounter' },
  netIn: { kind: 'rate', counter: 'bytesInCounter' },
  netOut: { kind: 'rate', counter: 'bytesOutCounter' },
  blksRead: { kind: 'rate', counter: 'blksReadCounter' },
  blksHit: { kind: 'rate', counter: 'blksHitCounter' },
  tupFetched: { kind: 'rate', counter: 'tupFetchedCounter' },
  tupReturned: { kind: 'rate', counter: 'tupReturnedCounter' },
  tupInserted: { kind: 'rate', counter: 'tupInsertedCounter' },
  tupUpdated: { kind: 'rate', counter: 'tupUpdatedCounter' },
  tupDeleted: { kind: 'rate', counter: 'tupDeletedCounter' },
  tempBytes: { kind: 'rate', counter: 'tempBytesCounter' },
  // Instantaneous gauges (session state counts etc.).
  sessionTotal: { kind: 'gauge', field: 'sessionTotal' },
  sessionActive: { kind: 'gauge', field: 'sessionActive' },
  sessionIdle: { kind: 'gauge', field: 'sessionIdle' },
};

/** Command names inside `statementCounters` (MySQL Com_*, PG tup_*). */
export const COMMAND_KEYS = ['select', 'insert', 'update', 'delete'] as const;
export type CommandKey = (typeof COMMAND_KEYS)[number];

export type StatusRecord = Record<string, string | number | boolean | null>;

/**
 * Previous-sample state for rate differencing & gauge seeding.
 * Holds the last raw value observed for every rate counter.
 */
export interface TrendPrevSample {
  ts: number;
  values: Record<string, number | null>;
}

export interface TrendUpdateInput {
  record: StatusRecord;
  /** Wall-clock ms for this sample. */
  now: number;
  /** 刷新周期（ms）：采样时间对齐到其整数倍。默认 5s。 */
  periodMs?: number;
  prev: TrendPrevSample;
  history: Record<string, TrendSeries>;
  /** statementCounters from the snapshot, if any. */
  statements?: Record<string, unknown> | null;
}

export interface TrendUpdateResult {
  history: Record<string, TrendSeries>;
  prev: TrendPrevSample;
}

function numberFrom(value: string | number | boolean | null | undefined): number | null {
  if (typeof value === 'number') return Number.isFinite(value) ? value : null;
  if (typeof value === 'string' && value.trim() !== '') {
    const n = Number(value);
    return Number.isFinite(n) ? n : null;
  }
  return null;
}

/** Append a sample, then trim to a fixed rolling time window (and hard cap). */
function appendPoint(series: TrendSeries | undefined, now: number, value: number): TrendSeries {
  const next: TrendSeries = [...(series ?? []), { t: now, v: value }].filter(
    (p) => p.t >= now - HISTORY_WINDOW_MS,
  );
  return next.slice(-HISTORY_MAX);
}

/** Compute a per-second rate for a cumulative counter, seeding with lifetime avg. */
function rateFor(
  counterKey: string,
  record: StatusRecord,
  prev: TrendPrevSample,
  dtSec: number,
  uptime: number | null,
): number | null {
  const cur = numberFrom(record[counterKey]);
  if (cur == null) return null;
  const prevVal = prev.values[counterKey];
  if (prevVal != null && dtSec > 0) {
    const delta = cur - prevVal;
    return delta < 0 ? 0 : delta / dtSec;
  }
  // First sample (or missing prev): seed with lifetime average — same basis as card value.
  if (uptime != null && uptime > 0) {
    return cur / uptime;
  }
  return null;
}

/**
 * Update rolling trend history from one status snapshot.
 * - `rate` series: Δcounter/Δt (first sample seeds counter/uptime).
 * - `gauge` series: instantaneous value.
 * - command series (COMMAND_KEYS): derived from `statementCounters` as rates.
 *
 * Each sample is stamped with `now`; series are trimmed to a 1-hour rolling
 * window so the X axis is real time and the left edge stays pinned.
 */
export function updateTrendHistory(input: TrendUpdateInput): TrendUpdateResult {
  const { record, prev, statements } = input;
  // 采样时间对齐到刷新周期的整数倍（如 5s → 17:20:25），避免非整数倍时间点。
  const now = alignTimestamp(input.now, input.periodMs);
  const dtSec = prev.ts > 0 ? (now - prev.ts) / 1000 : 0;
  const uptime = numberFrom(record.uptimeSeconds);
  // Only carry samples inside the rolling window forward; drop stale ones.
  const nextHist: Record<string, TrendSeries> = {};
  for (const [seriesKey, series] of Object.entries(input.history)) {
    const kept = (series ?? []).filter((p) => p.t >= now - HISTORY_WINDOW_MS).slice(-HISTORY_MAX);
    if (kept.length > 0) nextHist[seriesKey] = kept;
  }
  const nextPrevValues: Record<string, number | null> = { ...prev.values };

  // Rate (counter) series.
  for (const [seriesKey, def] of Object.entries(TREND_SERIES)) {
    if (def.kind !== 'rate' || !def.counter) continue;
    const rate = rateFor(def.counter, record, prev, dtSec, uptime);
    if (rate != null) {
      nextHist[seriesKey] = appendPoint(nextHist[seriesKey], now, rate);
    }
  }

  // Gauge (instant) series.
  for (const [seriesKey, def] of Object.entries(TREND_SERIES)) {
    if (def.kind !== 'gauge' || !def.field) continue;
    const value = numberFrom(record[def.field]);
    if (value != null && value >= 0) {
      nextHist[seriesKey] = appendPoint(nextHist[seriesKey], now, value);
    }
  }

  // Command series from `statementCounters` (per-command cumulative rates).
  if (statements && typeof statements === 'object') {
    for (const cmd of COMMAND_KEYS) {
      const raw = (statements as Record<string, unknown>)[cmd];
      const cur = numberFrom(raw as string | number | boolean | null | undefined);
      if (cur == null) continue;
      const prevKey = `__stmt_${cmd}`;
      const prevVal = prev.values[prevKey];
      let rate: number | null = null;
      if (prevVal != null && dtSec > 0) {
        const delta = cur - prevVal;
        rate = delta < 0 ? 0 : delta / dtSec;
      } else if (uptime != null && uptime > 0) {
        rate = cur / uptime;
      }
      if (rate != null) {
        nextHist[`cmd_${cmd}`] = appendPoint(nextHist[`cmd_${cmd}`], now, rate);
      }
      nextPrevValues[prevKey] = cur;
    }
  }

  // Persist counter baselines for next diff.
  const nextPrev: TrendPrevSample = {
    ts: now,
    values: {
      ...nextPrevValues,
      questionsCounter: numberFrom(record.questionsCounter),
      xactCommitCounter: numberFrom(record.xactCommitCounter),
      xactRollbackCounter: numberFrom(record.xactRollbackCounter),
      newSessionsCounter: numberFrom(record.newSessionsCounter),
      bytesInCounter: numberFrom(record.bytesInCounter),
      bytesOutCounter: numberFrom(record.bytesOutCounter),
      blksReadCounter: numberFrom(record.blksReadCounter),
      blksHitCounter: numberFrom(record.blksHitCounter),
      tupFetchedCounter: numberFrom(record.tupFetchedCounter),
      tupReturnedCounter: numberFrom(record.tupReturnedCounter),
      tupInsertedCounter: numberFrom(record.tupInsertedCounter),
      tupUpdatedCounter: numberFrom(record.tupUpdatedCounter),
      tupDeletedCounter: numberFrom(record.tupDeletedCounter),
      tempBytesCounter: numberFrom(record.tempBytesCounter),
    },
  };

  return { history: nextHist, prev: nextPrev };
}

/**
 * Prefer latest trend point for card display so cards and charts share one source.
 * If the series is a byte-rate use formatRate, otherwise fall back to the live value.
 */
export function cardRateFromHistory(
  history: Record<string, TrendSeries>,
  chartKey: string,
  fallback: string | number | boolean | null | undefined,
): string {
  const series = history[chartKey];
  if (series && series.length > 0) {
    return formatRate(series[series.length - 1].v);
  }
  if (fallback == null || fallback === '') return '—';
  return String(fallback);
}

/** Latest numeric value of a series, or null if empty. */
export function latestSeriesValue(series?: TrendSeries): number | null {
  if (!series || series.length === 0) return null;
  return series[series.length - 1].v;
}

/** Format a numeric rate compactly (B/s, /s, KB/s etc.). */
export function formatRate(value: number): string {
  if (value < 10) return value.toFixed(2);
  if (value < 1000) return value.toFixed(1);
  if (value < 1000000) return `${(value / 1000).toFixed(1)}K`;
  return `${(value / 1000000).toFixed(1)}M`;
}

/** Bytes with a unit: for ultimate card values that display bytes. */
export function formatBytes(value: number): string {
  if (value < 1024) return `${value.toFixed(1)} B`;
  if (value < 1024 * 1024) return `${(value / 1024).toFixed(1)} KB`;
  if (value < 1024 * 1024 * 1024) return `${(value / 1024 / 1024).toFixed(1)} MB`;
  return `${(value / 1024 / 1024 / 1024).toFixed(1)} GB`;
}

/** Bytes per second with a unit suffix (e.g. `24 B/s`, `3.3 KB/s`). */
export function formatByteRate(value: number): string {
  return `${formatBytes(value)}/s`;
}
