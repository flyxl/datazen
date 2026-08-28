import type { TransferColumnMapping, TransferTableResult } from '../../commands/transfer';

/** Ensure every source column has a mapping row for the editor. */
export function normalizeColumnMappings(table: TransferTableResult): TransferColumnMapping[] {
  const sourceColumns = table.sourceColumns ?? [
    ...new Set(table.columnMappings.map((m) => m.sourceColumn)),
  ];
  const bySource = new Map(table.columnMappings.map((m) => [m.sourceColumn, m]));
  return sourceColumns.map((sourceColumn) => {
    const existing = bySource.get(sourceColumn);
    return (
      existing ?? {
        sourceColumn,
        targetColumn: '',
        skip: true,
      }
    );
  });
}

/** Match source columns to target columns by identical name. */
export function autoMatchColumnMappings(
  sourceColumns: string[],
  targetColumns: string[],
): TransferColumnMapping[] {
  const targetSet = new Set(targetColumns);
  return sourceColumns.map((sourceColumn) => {
    const matched = targetSet.has(sourceColumn);
    return {
      sourceColumn,
      targetColumn: matched ? sourceColumn : '',
      skip: !matched,
    };
  });
}

/** Mark rows without a target column as skipped. */
export function clearUnmappedColumnMappings(
  mappings: TransferColumnMapping[],
): TransferColumnMapping[] {
  return mappings.map((m) => (m.targetColumn.trim() ? m : { ...m, targetColumn: '', skip: true }));
}

/** Target columns not referenced by any active mapping. */
export function unmappedTargetColumns(table: TransferTableResult): string[] {
  if (table.createNew || !table.targetColumns?.length) return [];
  const used = new Set(
    table.columnMappings.filter((m) => !m.skip && m.targetColumn).map((m) => m.targetColumn),
  );
  return table.targetColumns.filter((col) => !used.has(col));
}

export function tableHasActiveMappings(table: TransferTableResult): boolean {
  return normalizeColumnMappings(table).some((m) => !m.skip && m.targetColumn.trim());
}

export function mergeInspectTables(
  prev: TransferTableResult[],
  inspected: TransferTableResult[],
): TransferTableResult[] {
  const bySource = new Map(inspected.map((t) => [t.sourceTable, t]));
  return prev.map((t) => {
    const next = bySource.get(t.sourceTable);
    if (!next) return t;
    return {
      ...next,
      enabled: t.enabled,
      columnMappings: t.columnMappings.length > 0 ? t.columnMappings : next.columnMappings,
      targetTable: t.targetTable || next.targetTable,
      createNew: t.createNew,
      ddlOverride: t.ddlOverride ?? next.ddlOverride,
    };
  });
}
