import { cpus, platform, arch, totalmem } from 'node:os';

/** Minimum sample count for benchmark runs (per implementation plan). */
export const BENCHMARK_MIN_SAMPLES = 30;

export interface BenchmarkOptions {
  /** Warmup iterations discarded before sampling. Default 5. */
  warmup?: number;
  /** Measured sample count. Default 30, never below {@link BENCHMARK_MIN_SAMPLES}. */
  samples?: number;
  /** Human-readable label attached to the result. */
  label?: string;
}

export interface MachineInfo {
  platform: string;
  arch: string;
  cpuModel: string;
  cpuCount: number;
  totalMemoryGb: number;
  nodeVersion: string;
}

export interface BenchmarkResult {
  label: string;
  warmup: number;
  samples: number;
  medianMs: number;
  p95Ms: number;
  minMs: number;
  maxMs: number;
  rawMs: number[];
  machine: MachineInfo;
}

/** Collect host machine metadata for benchmark result records. */
export function collectMachineInfo(): MachineInfo {
  const cpuList = cpus();
  return {
    platform: platform(),
    arch: arch(),
    cpuModel: cpuList[0]?.model ?? 'unknown',
    cpuCount: cpuList.length,
    totalMemoryGb: Math.round((totalmem() / 1024 ** 3) * 10) / 10,
    nodeVersion: process.version,
  };
}

/** Compute the median of a numeric array (average of two middle values when even length). */
export function median(values: number[]): number {
  if (values.length === 0) return 0;
  const sorted = [...values].sort((a, b) => a - b);
  const mid = Math.floor(sorted.length / 2);
  if (sorted.length % 2 === 0) {
    return (sorted[mid - 1] + sorted[mid]) / 2;
  }
  return sorted[mid];
}

/**
 * Compute a percentile using linear interpolation (p in [0, 100]).
 * p95 uses the 95th percentile per the SQL editor performance plan.
 */
export function percentile(values: number[], p: number): number {
  if (values.length === 0) return 0;
  if (values.length === 1) return values[0];
  const sorted = [...values].sort((a, b) => a - b);
  const rank = (p / 100) * (sorted.length - 1);
  const lower = Math.floor(rank);
  const upper = Math.ceil(rank);
  if (lower === upper) return sorted[lower];
  const weight = rank - lower;
  return sorted[lower] * (1 - weight) + sorted[upper] * weight;
}

function resolveSampleCount(samples?: number): number {
  const requested = samples ?? BENCHMARK_MIN_SAMPLES;
  return Math.max(BENCHMARK_MIN_SAMPLES, requested);
}

function nowMs(): number {
  return typeof performance !== 'undefined' && typeof performance.now === 'function'
    ? performance.now()
    : Date.now();
}

/**
 * Run a synchronous or async benchmark with warmup and repeated sampling.
 * Returns median and p95 durations in milliseconds.
 */
export async function runBenchmark(
  fn: () => void | Promise<void>,
  options: BenchmarkOptions = {},
): Promise<BenchmarkResult> {
  const warmup = Math.max(0, options.warmup ?? 5);
  const sampleCount = resolveSampleCount(options.samples);
  const label = options.label ?? 'benchmark';

  for (let i = 0; i < warmup; i += 1) {
    await fn();
  }

  const rawMs: number[] = [];
  for (let i = 0; i < sampleCount; i += 1) {
    const start = nowMs();
    await fn();
    rawMs.push(nowMs() - start);
  }

  return {
    label,
    warmup,
    samples: sampleCount,
    medianMs: median(rawMs),
    p95Ms: percentile(rawMs, 95),
    minMs: Math.min(...rawMs),
    maxMs: Math.max(...rawMs),
    rawMs,
    machine: collectMachineInfo(),
  };
}

/** Format a benchmark result as a single-line summary for logs. */
export function formatBenchmarkSummary(result: BenchmarkResult): string {
  const { label, medianMs, p95Ms, samples, warmup, machine } = result;
  return (
    `[${label}] warmup=${warmup} samples=${samples} ` +
    `median=${medianMs.toFixed(3)}ms p95=${p95Ms.toFixed(3)}ms ` +
    `host=${machine.platform}/${machine.arch} cpus=${machine.cpuCount}`
  );
}
