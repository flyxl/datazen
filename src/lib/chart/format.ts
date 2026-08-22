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

const EPOCH_MS_MIN = Date.UTC(2000, 0, 1); // 946684800000
const EPOCH_MS_MAX = Date.UTC(2100, 0, 1); // 4102444800000

/** Detect a value in the reasonable epoch-ms range (2000–2100). */
function isEpochMs(v: number): boolean {
  return v >= EPOCH_MS_MIN && v <= EPOCH_MS_MAX;
}

/** Format epoch-ms as a wall-clock `HH:mm:ss`. */
export function formatEpochMs(ts: number): string {
  const d = new Date(ts);
  const pad = (n: number) => String(n).padStart(2, '0');
  return `${pad(d.getHours())}:${pad(d.getMinutes())}:${pad(d.getSeconds())}`;
}

export function formatAxisTick(value: unknown): string {
  if (value == null) return '';
  if (typeof value === 'number') {
    // A time-axis value (epoch ms) renders as a clock time; other numbers as-is.
    return isEpochMs(value) ? formatEpochMs(value) : formatNumber(value);
  }
  const str = String(value);
  return str.length > 20 ? `${str.slice(0, 18)}…` : str;
}
