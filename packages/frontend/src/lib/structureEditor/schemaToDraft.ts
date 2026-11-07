import type { TableSchema } from '../../types';
import type { StructureColumnDraft, StructureIndexDraft } from './types';
import { newColumnId, newIndexId } from './draftIds';

export function schemaToDraft(schema: TableSchema): {
  columns: StructureColumnDraft[];
  indexes: StructureIndexDraft[];
} {
  const columns: StructureColumnDraft[] = schema.columns.map((c) => ({
    id: newColumnId(),
    name: c.name,
    dataType: c.dataType,
    nullable: c.nullable,
    defaultValue: c.defaultValue ?? null,
    comment: c.comment ?? null,
    isPrimaryKey: schema.primaryKeys.includes(c.name),
    isAutoIncrement: c.isAutoIncrement ?? false,
    isUnique: schema.indexes.some(
      (idx) =>
        idx.isUnique &&
        !idx.isPrimary &&
        idx.columns.length === 1 &&
        idx.columns[0] === c.name,
    ),
  }));

  const indexes: StructureIndexDraft[] = schema.indexes
    .filter((idx) => !idx.isPrimary)
    .map((idx) => ({
      id: newIndexId(),
      name: idx.name,
      columns: [...idx.columns],
      isUnique: idx.isUnique,
      isPrimary: false,
      indexType: idx.indexType ?? '',
    }));

  return { columns, indexes };
}
