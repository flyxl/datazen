import type { DatabaseTypeMeta } from '../databaseMeta';

export function canOpenStructureEditor(meta: DatabaseTypeMeta | undefined): boolean {
  if (!meta) return false;
  if (meta.isKeyValue || meta.connectionView === 'document') return false;
  if (meta.structureEditor?.enabled === false) return false;
  if (!meta.supportsSQL) return false;
  return true;
}
