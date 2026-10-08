import type { TableInfo, TableSchemaDiff } from '../../types';

export function tableDiffHasChanges(diff: TableSchemaDiff): boolean {
  if (diff.targetOnly) return true;
  const missing = diff.missingOnTarget ?? diff.added;
  const extra = diff.extraOnTarget ?? diff.removed;
  return (
    missing.length > 0 ||
    extra.length > 0 ||
    diff.changed.length > 0 ||
    (diff.missingCheckConstraints?.length ?? 0) > 0 ||
    (diff.extraCheckConstraints?.length ?? 0) > 0 ||
    Boolean(diff.tableOptions)
  );
}

/** Build the table identifier passed to Schema Diff IPC (schema-qualified when needed). */
export function qualifySchemaDiffTableName(table: TableInfo, activeSchema?: string): string {
  const schema = table.schema?.trim();
  if (schema) {
    if (!activeSchema || schema === activeSchema) {
      return `${schema}.${table.name}`;
    }
  }
  return table.name;
}

export function filterTablesForSchema(tables: TableInfo[], activeSchema?: string): TableInfo[] {
  return tables.filter((table) => {
    if (table.tableType !== 'table') return false;
    const schema = table.schema?.trim();
    if (activeSchema && schema) {
      return schema === activeSchema;
    }
    return true;
  });
}

export interface SchemaDiffTablePick {
  name: string;
  enabled: boolean;
  /** Stable UI identity for source/target metadata rows. */
  origin: 'both' | 'source-only' | 'target-only';
  /** Exact selector used when reading the source endpoint. */
  sourceName?: string;
  /** Exact selector used when reading the target endpoint. */
  targetName?: string;
}

export function enabledTableNames(picks: SchemaDiffTablePick[]): string[] {
  return picks.filter((row) => row.enabled).map((row) => row.name);
}

export function enabledSourceTableNames(picks: SchemaDiffTablePick[]): string[] {
  return picks
    .filter((row) => row.enabled && row.origin !== 'target-only')
    .map((row) => row.sourceName ?? row.name);
}

export function enabledTargetOnlyTableNames(picks: SchemaDiffTablePick[]): string[] {
  return picks
    .filter((row) => row.enabled && row.origin === 'target-only')
    .map((row) => row.targetName ?? row.name);
}

/**
 * Merge the two endpoint inventories using the relation name in the selected
 * schema as the logical identity. Each endpoint still retains its own exact
 * qualified selector for IPC, which matters when the dialects use different
 * schema qualification rules.
 */
export function mergeSchemaDiffTablePicks(
  sourceTables: TableInfo[],
  targetTables: TableInfo[],
  sourceSchema?: string,
  targetSchema?: string,
): SchemaDiffTablePick[] {
  const source = filterTablesForSchema(sourceTables, sourceSchema);
  const target = filterTablesForSchema(targetTables, targetSchema);
  const sourceByRelation = new Map(source.map((table) => [table.name, table]));
  const targetByRelation = new Map(target.map((table) => [table.name, table]));
  const relationNames = new Set([...sourceByRelation.keys(), ...targetByRelation.keys()]);

  return [...relationNames]
    .map((relation) => {
      const sourceTable = sourceByRelation.get(relation);
      const targetTable = targetByRelation.get(relation);
      const sourceName = sourceTable
        ? qualifySchemaDiffTableName(sourceTable, sourceSchema)
        : undefined;
      const targetName = targetTable
        ? qualifySchemaDiffTableName(targetTable, targetSchema)
        : undefined;
      const origin =
        sourceTable && targetTable ? 'both' : sourceTable ? 'source-only' : 'target-only';
      return {
        name: sourceName ?? targetName ?? relation,
        enabled: origin !== 'target-only',
        origin,
        sourceName,
        targetName,
      } satisfies SchemaDiffTablePick;
    })
    .sort((a, b) => a.name.localeCompare(b.name));
}
