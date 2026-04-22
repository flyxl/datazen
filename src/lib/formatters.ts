import type { Value } from '../types';
import { t } from '../locales/t';

export function formatTimestamp(value: unknown): string {
  if (value === null || value === undefined) return 'NULL';
  if (typeof value === 'string' || typeof value === 'number') {
    const d = new Date(value);
    if (!Number.isNaN(d.getTime())) return d.toISOString();
  }
  return String(value);
}

export function formatCell(value: unknown): string {
  if (value === null || value === undefined) return 'NULL';
  if (typeof value === 'boolean') return value ? 'true' : 'false';
  if (typeof value === 'object') {
    try {
      return JSON.stringify(value);
    } catch {
      return String(value);
    }
  }
  return String(value);
}

export function formatLastConnected(iso?: string): string {
  if (!iso) return t('conn.neverConnected');
  const d = new Date(iso);
  if (Number.isNaN(d.getTime())) return iso;
  return d.toLocaleString();
}

export function displayValueForTitle(value: Value | unknown): string {
  return formatCell(value);
}
