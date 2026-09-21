/**
 * Key-Value (Redis-family) browser data contracts shared by host and drivers.
 *
 * Pure data shapes — no host runtime dependency.
 */

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
