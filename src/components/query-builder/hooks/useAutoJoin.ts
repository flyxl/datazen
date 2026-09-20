import { useMemo } from 'react';
import type { QbJoin } from '../types';

/** Foreign key relationship metadata from the schema store. */
export interface ForeignKeyRelation {
  fromTable: string;
  fromColumn: string;
  toTable: string;
  toColumn: string;
  /** Constraint name (or a synthesised key when the engine reports none). */
  constraint: string;
  /** 1-based position of this column pair inside the constraint. */
  ordinal: number;
  /** How many column pairs the constraint has (>1 = composite). */
  pairCount: number;
}

/** Stable identity for one constraint between a table and its referenced table. */
export function constraintKey(fromTable: string, constraint: string): string {
  return `${fromTable}::${constraint}`;
}

/**
 * Automatically detect FK relationships between selected tables.
 *
 * Returns one `QbJoin` per **column pair** (a composite FK yields several pairs
 * sharing a `constraint`), for every foreign key where both the source and the
 * referenced table are on the canvas.
 *
 * Self-referencing FKs are skipped: the builder cannot express a self join
 * (`validation.ts` reports `self-join`), so offering them as confirmable
 * candidates would let a user promote something that then blocks OK.
 *
 * @param selectedTables - Currently selected table names.
 * @param foreignKeys     - All known FK relationships from the schema.
 */
export function useAutoJoin(selectedTables: string[], foreignKeys: ForeignKeyRelation[]): QbJoin[] {
  return useMemo(() => {
    if (selectedTables.length === 0 || foreignKeys.length === 0) return [];

    const tableSet = new Set(selectedTables);

    return foreignKeys
      .filter(
        (fk) =>
          tableSet.has(fk.fromTable) && tableSet.has(fk.toTable) && fk.fromTable !== fk.toTable,
      )
      .map((fk) => ({
        id: `auto-${fk.fromTable}.${fk.fromColumn}-${fk.toTable}.${fk.toColumn}`,
        type: 'INNER' as const,
        leftTable: fk.fromTable,
        leftColumn: fk.fromColumn,
        rightTable: fk.toTable,
        rightColumn: fk.toColumn,
        isManual: false as const,
        constraint: constraintKey(fk.fromTable, fk.constraint),
      }));
  }, [selectedTables, foreignKeys]);
}
