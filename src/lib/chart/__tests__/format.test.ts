import { describe, expect, it } from 'vitest';
import { formatAxisTick, formatNumber, formatPercent } from '../format';

describe('formatNumber', () => {
  it('returns empty for null/undefined', () => {
    expect(formatNumber(null)).toBe('');
    expect(formatNumber(undefined)).toBe('');
  });

  it('formats integers with locale grouping', () => {
    expect(formatNumber(1234567)).toMatch(/1.*234.*567/);
  });

  it('formats decimals with up to 2 fraction digits', () => {
    expect(formatNumber(3.14159)).toMatch(/3\.14/);
  });
});

describe('formatPercent', () => {
  it('returns empty for null/undefined', () => {
    expect(formatPercent(null)).toBe('');
  });

  it('multiplies by 100 and adds percent sign', () => {
    expect(formatPercent(0.256)).toBe('25.6%');
  });
});

describe('formatAxisTick', () => {
  it('delegates numbers to formatNumber', () => {
    expect(formatAxisTick(1000)).toMatch(/1.*000/);
  });

  it('truncates long strings', () => {
    const long = 'a'.repeat(25);
    expect(formatAxisTick(long)).toBe(`${'a'.repeat(18)}…`);
  });

  it('returns empty for null', () => {
    expect(formatAxisTick(null)).toBe('');
  });
});
