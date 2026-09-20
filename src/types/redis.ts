/** Key-Value (Redis) types. */

// ── Key-Value (Redis) types ──

export interface KeyEntry {
  key: string;
  keyType: string;
  ttl: number;
  size: number;
  preview: string;
}

export interface KeyScanResult {
  cursor: number;
  keys: KeyEntry[];
  dbSize: number;
}

export interface KeyDetail {
  key: string;
  keyType: string;
  ttl: number;
  value: unknown;
}

/** Raw backend response — rows are 2D arrays. */
export interface TableDataResult {
  columns: ColumnSchema[];
  rows: (Value | null)[][];
  totalRows?: number;
  page: number;
  pageSize: number;
}
