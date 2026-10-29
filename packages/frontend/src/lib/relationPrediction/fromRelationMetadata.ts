/**
 * Adapt the shared relation metadata into prediction input.
 *
 * The engine deliberately knows nothing about the metadata cache; this is the one
 * place that bridges them, so the editor and the Visual Query Builder feed it
 * identical input and cannot disagree about the same schema.
 */

import type { EditorRelationMetadata } from '../relationMetadata/types';
import type { DeclaredForeignKey, PredictionColumn, PredictionTable } from './types';

/**
 * Column names covered by at least one index.
 *
 * `IndexInfo.columns` is a list; a column counts as indexed if any index mentions
 * it, single-column or composite.
 */
function indexedColumnNames(relation: EditorRelationMetadata): Set<string> {
  const indexed = new Set<string>();
  for (const index of relation.indexes) {
    for (const column of index.columns) indexed.add(column.toLowerCase());
  }
  return indexed;
}

/**
 * Every column set a relationship may target: the primary key plus each unique
 * index. Non-unique indexes are not keys and are excluded.
 */
function uniqueColumnSets(relation: EditorRelationMetadata): string[][] {
  const sets: string[][] = [];
  if (relation.primaryKey.length > 0) sets.push([...relation.primaryKey]);
  for (const index of relation.indexes) {
    if (!index.isUnique || index.columns.length === 0) continue;
    sets.push([...index.columns]);
  }
  return sets;
}

function declaredForeignKeys(relation: EditorRelationMetadata): DeclaredForeignKey[] {
  return relation.foreignKeys.map((fk) => ({
    columns: [...fk.columns],
    referencedTable: fk.referencedTable,
    referencedColumns: [...fk.referencedColumns],
  }));
}

/**
 * Convert one loaded relation into prediction input.
 *
 * @param id - Stable identity used in the output; the caller decides what a table
 *   is keyed by (the builder uses its canvas identity).
 */
export function toPredictionTable(
  id: string,
  relation: EditorRelationMetadata,
  schema?: string,
): PredictionTable {
  const indexed = indexedColumnNames(relation);
  const columns: PredictionColumn[] = relation.columns.map((column) => ({
    name: column.name,
    dataType: column.dataType,
    nullable: column.nullable,
    indexed: indexed.has(column.name.toLowerCase()),
  }));

  return {
    id,
    name: relation.identity.name.name,
    schema,
    columns,
    primaryKey: [...relation.primaryKey],
    uniqueColumnSets: uniqueColumnSets(relation),
    declaredForeignKeys: declaredForeignKeys(relation),
  };
}

/**
 * Convert a set of loaded relations into prediction input.
 *
 * @param relations - Loaded metadata keyed by caller identity.
 * @param schemaOf  - Schema for each identity, when the caller tracks one.
 */
export function toPredictionTables(
  relations: ReadonlyMap<string, EditorRelationMetadata>,
  schemaOf?: (id: string) => string | undefined,
): PredictionTable[] {
  return [...relations].map(([id, relation]) => toPredictionTable(id, relation, schemaOf?.(id)));
}
