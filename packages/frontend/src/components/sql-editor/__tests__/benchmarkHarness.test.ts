import { describe, expect, it, vi } from 'vitest';
import {
  BENCHMARK_MIN_SAMPLES,
  collectMachineInfo,
  formatBenchmarkSummary,
  median,
  percentile,
  runBenchmark,
} from './benchmarkHarness';

describe('benchmarkHarness', () => {
  describe('median', () => {
    it('returns 0 for empty input', () => {
      expect(median([])).toBe(0);
    });

    it('returns middle value for odd length', () => {
      expect(median([3, 1, 2])).toBe(2);
    });

    it('averages two middle values for even length', () => {
      expect(median([4, 1, 3, 2])).toBe(2.5);
    });
  });

  describe('percentile', () => {
    it('returns 0 for empty input', () => {
      expect(percentile([], 95)).toBe(0);
    });

    it('returns sole value for single-element input', () => {
      expect(percentile([42], 95)).toBe(42);
    });

    it('computes p95 with linear interpolation', () => {
      const values = Array.from({ length: 100 }, (_, i) => i + 1);
      expect(percentile(values, 95)).toBeCloseTo(95.05, 5);
    });
  });

  describe('collectMachineInfo', () => {
    it('returns stable host metadata fields', () => {
      const info = collectMachineInfo();
      expect(info.platform).toBeTruthy();
      expect(info.arch).toBeTruthy();
      expect(info.cpuCount).toBeGreaterThan(0);
      expect(info.totalMemoryGb).toBeGreaterThan(0);
      expect(info.nodeVersion).toMatch(/^v\d+/);
    });
  });

  describe('runBenchmark', () => {
    it('enforces minimum sample count', async () => {
      const fn = vi.fn(async () => {});

      const result = await runBenchmark(fn, { warmup: 2, samples: 5, label: 'min-samples' });

      expect(result.samples).toBe(BENCHMARK_MIN_SAMPLES);
      expect(fn).toHaveBeenCalledTimes(2 + BENCHMARK_MIN_SAMPLES);
    });

    it('computes median and p95 from timed samples', async () => {
      let tick = 0;
      const nowSpy = vi.spyOn(performance, 'now').mockImplementation(() => {
        tick += 1;
        return tick;
      });

      const result = await runBenchmark(async () => {}, {
        warmup: 0,
        samples: 30,
        label: 'stats',
      });

      nowSpy.mockRestore();

      expect(result.rawMs).toHaveLength(30);
      expect(result.rawMs.every((ms) => ms === 1)).toBe(true);
      expect(result.medianMs).toBe(median(result.rawMs));
      expect(result.p95Ms).toBe(percentile(result.rawMs, 95));
      expect(result.minMs).toBe(1);
      expect(result.maxMs).toBe(1);
      expect(result.machine.platform).toBeTruthy();
    });
  });

  describe('formatBenchmarkSummary', () => {
    it('includes label, stats, and host info', () => {
      const summary = formatBenchmarkSummary({
        label: 'scan-20k',
        warmup: 5,
        samples: 30,
        medianMs: 1.234,
        p95Ms: 2.345,
        minMs: 0.5,
        maxMs: 3.0,
        rawMs: [1, 2, 3],
        machine: collectMachineInfo(),
      });
      expect(summary).toContain('scan-20k');
      expect(summary).toContain('median=1.234ms');
      expect(summary).toContain('p95=2.345ms');
      expect(summary).toContain('samples=30');
    });
  });

  describe('[tester] benchmarkHarness edge paths', () => {
    it('percentile handles exact-rank boundaries (p0/p100)', () => {
      const values = [10, 20, 30, 40];
      expect(percentile(values, 0)).toBe(10);
      expect(percentile(values, 100)).toBe(40);
    });

    it('runBenchmark uses default label when omitted', async () => {
      const fn = vi.fn(async () => {});
      const result = await runBenchmark(fn, { warmup: 0, samples: 30 });
      expect(result.label).toBe('benchmark');
      expect(result.warmup).toBe(0);
    });

    it('runBenchmark runs default warmup iterations', async () => {
      const fn = vi.fn(async () => {});
      await runBenchmark(fn, { samples: 30 });
      expect(fn).toHaveBeenCalledTimes(5 + BENCHMARK_MIN_SAMPLES);
    });
  });
});
