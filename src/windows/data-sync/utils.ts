import type { TableInfo } from '../../types';

export function uniqueSchemasFromTables(tables: TableInfo[]): string[] {
  const set = new Set<string>();
  for (const t of tables) {
    const s = t.schema?.trim();
    if (s) set.add(s);
  }
  return [...set].sort();
}

export function pickDefaultSchema(schemas: string[], previous?: string): string {
  if (previous && schemas.includes(previous)) return previous;
  if (schemas.includes('public')) return 'public';
  return schemas[0] ?? '';
}

export type SyncState = 'idle' | 'inspecting' | 'comparing' | 'compared' | 'executing' | 'unknown' | 'done';

/** Whether the current state carries a reviewable comparison result. */
export function isComparedSyncState(syncState: SyncState): boolean {
  return (
    syncState === 'compared' ||
    syncState === 'executing' ||
    syncState === 'unknown' ||
    syncState === 'done'
  );
}
