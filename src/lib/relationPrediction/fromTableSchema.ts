/**
 * Adapt the ER diagram's table schemas into prediction input.
 *
 * The ER diagram already loads a full schema per table — columns with types, the
 * primary key, every index and every declared foreign key — so it can predict over
 * a whole database without any extra IPC. That makes it the cheapest place to use
 * the engine, and the one where a missing relationship is most visible.
 *
 * Kept separate from the `EditorRelationMetadata` adapter because the two carry
 * the same facts in different shapes; sharing one would mean bending one shape
 * into the other for no gain.
 */

import type { TableSchema } from '../../types';
import type { DeclaredForeignKey, PredictionColumn, PredictionTable } from './types';

/**
 * Every column set a relationship may target: the primary key plus each unique
 * index. Non-unique indexes are not keys and are excluded.
 */
function uniqueColumnSets(schema: TableSchema): string[][] {
  const sets: string[][] = [];
  if (schema.primaryKeys.length > 0) sets.push([...schema.primaryKeys]);
  for (const index of schema.indexes) {
    if (!index.isUnique || index.columns.length === 0) continue;
    sets.push([...index.columns]);
  }
  return sets;
}

function declaredForeignKeys(schema: TableSchema): DeclaredForeignKey[] {
  return schema.foreignKeys.map((fk) => ({
    columns: [...fk.columns],
    referencedTable: fk.referencedTable,
    referencedColumns: [...fk.referencedColumns],
  }));
}

/**
 * Convert one ER table schema into prediction input.
 *
 * @param id - Identity used in the output. The ER diagram keys nodes by table
 *   name, so callers normally pass that.
 */
export function toPredictionTableFromSchema(id: string, schema: TableSchema): PredictionTable {
  const indexed = new Set<string>();
  for (const index of schema.indexes) {
    for (const column of index.columns) indexed.add(column.toLowerCase());
  }

  const columns: PredictionColumn[] = schema.columns.map((column) => ({
    name: column.name,
    dataType: column.dataType,
    nullable: column.nullable,
    indexed: indexed.has(column.name.toLowerCase()),
  }));

  return {
    id,
    name: schema.tableName,
    columns,
    primaryKey: [...schema.primaryKeys],
    uniqueColumnSets: uniqueColumnSets(schema),
    declaredForeignKeys: declaredForeignKeys(schema),
  };
}

/** Convert a whole ER schema into prediction input, keyed by table name. */
export function toPredictionTablesFromSchemas(schemas: readonly TableSchema[]): PredictionTable[] {
  return schemas.map((schema) => toPredictionTableFromSchema(schema.tableName, schema));
}
