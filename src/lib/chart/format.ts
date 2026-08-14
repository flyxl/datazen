export function formatNumber(value: number | null | undefined): string {
  if (value == null) return '';
  if (Number.isInteger(value)) {
    return value.toLocaleString();
  }
  return value.toLocaleString(undefined, {
    minimumFractionDigits: 0,
    maximumFractionDigits: 2,
  });
}

export function formatCompact(value: number | null | undefined): string {
  if (value == null) return '';
  const abs = Math.abs(value);
  if (abs >= 1e9) return `${(value / 1e9).toFixed(1)}B`;
  if (abs >= 1e6) return `${(value / 1e6).toFixed(1)}M`;
  if (abs >= 1e4) return `${(value / 1e3).toFixed(1)}K`;
  if (Number.isInteger(value)) return value.toString();
  return value.toFixed(abs >= 100 ? 0 : abs >= 1 ? 1 : 2);
}

export function formatPercent(value: number | null | undefined): string {
  if (value == null) return '';
  return `${(value * 100).toFixed(1)}%`;
}

export function formatAxisTick(value: unknown): string {
  if (value == null) return '';
  if (typeof value === 'number') return formatNumber(value);
  const str = String(value);
  return str.length > 20 ? `${str.slice(0, 18)}…` : str;
}
