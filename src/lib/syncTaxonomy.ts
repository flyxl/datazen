/**
 * Driver id aliases for sync/transfer taxonomy lookup (mirrors driver-api
 * `normalize_driver_id`).
 */
export function normalizeDriverId(raw: string): string {
  switch (raw.toLowerCase()) {
    case 'postgres':
      return 'postgresql';
    case 'mssql':
      return 'sqlserver';
    case 'presto':
      return 'trino';
    case 'tidb':
    case 'oceanbase':
      return 'mysql';
    default:
      return raw.toLowerCase();
  }
}
