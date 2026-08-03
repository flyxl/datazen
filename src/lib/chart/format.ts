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
