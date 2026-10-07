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
  existing: TransferColumnMapping[] = [],
): TransferColumnMapping[] {
  const existingBySource = new Map(existing.map((m) => [m.sourceColumn, m]));
  const targetSet = new Set(targetColumns);
  return sourceColumns.map((sourceColumn) => {
    const matched = targetSet.has(sourceColumn);
    const prev = existingBySource.get(sourceColumn);
    return {
      sourceColumn,
      targetColumn: matched ? sourceColumn : '',
      skip: !matched,
      targetNativeType: prev?.targetNativeType,
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

/**
 * Whether one row, on its own, is complete enough to carry the gate.
 *
 * A table that does not exist yet has nothing the backend can look the name up
 * for, so the name has to be one somebody typed. An enabled create-new row
 * behind nothing but whitespace is the one shape the backend will happily echo
 * back and then write as a nameless table, so it does not clear the gate.
 */
function rowClearsGate(row: TransferTableResult): boolean {
  if (!row.enabled) return false;
  if (row.createNew && !row.targetTable.trim()) return false;
  return tableHasActiveMappings(row);
}

/**
 * The one definition of "the mapping step may be left".
 *
 * The Next gate and the post-prepare re-check both call this on the *rows they
 * hold at the moment they run*, so a rule written twice cannot drift into two
 * components disagreeing about the same table.
 */
export function mappingGateAllowsAdvance(rows: TransferTableResult[]): boolean {
  return rows.some(rowClearsGate);
}

/**
 * Why the gate is shut for a reason the reader can act on, as an i18n key.
 *
 * Only the unnamed-create-new case is reported. "Nothing is mapped yet" is the
 * state the user is still working in, not a dead end that deserves a banner.
 */
export function mappingGateBlockReason(
  rows: TransferTableResult[],
): 'transfer.mapping.targetNameRequired' | null {
  if (rows.some(rowClearsGate)) return null;
  const blocked = rows.some(
    (row) => row.enabled && row.createNew && tableHasActiveMappings(row),
  );
  return blocked ? 'transfer.mapping.targetNameRequired' : null;
}
