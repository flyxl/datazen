import type { SelectOption } from '../../ui/Select';
import type { QbColumnSelection } from '../types';

export interface FieldKey {
  table: string;
  column: string;
}

/** `table.column` → parts. Returns null for a malformed key. */
export function parseFieldKey(key: string): FieldKey | null {
  const dot = key.indexOf('.');
  if (dot === -1) return null;
  return { table: key.slice(0, dot), column: key.slice(dot + 1) };
}

/** The qualifier the generated SQL will use for a table (its alias when set). */
export function qualifierOf(table: string, aliases: Record<string, string>): string {
  return aliases[table] || table;
}

/** `q.column`, i.e. exactly the identifier the SQL emits. */
export function qualifiedRef(
  table: string,
  column: string,
  aliases: Record<string, string>,
): string {
  return `${qualifierOf(table, aliases)}.${column}`;
}

/**
 * Every column of every selected table as a picker option, labelled with the
 * qualifier the SQL will use — so the picker and the generated statement agree
 * about what `fa` means.
 */
export function buildColumnOptions(
  tables: string[],
  allColumns: Record<string, string[]>,
  aliases: Record<string, string>,
): SelectOption[] {
  const options: SelectOption[] = [];
  for (const table of tables) {
    for (const column of allColumns[table] ?? []) {
      options.push({
        value: `${table}.${column}`,
        label: qualifiedRef(table, column, aliases),
      });
    }
  }
  return options;
}

/**
 * The three text parts of a SELECT chip: `SUM(` + `fa.amount` + `) AS total`.
 */
export function fieldChipParts(
  selection: QbColumnSelection,
  aliases: Record<string, string>,
): { label: string; prefix: string; suffix: string } {
  const ref = qualifiedRef(selection.table, selection.column, aliases);
  return {
    label: ref,
    prefix: selection.aggregate ? `${selection.aggregate}(` : '',
    suffix: `${selection.aggregate ? ')' : ''}${selection.alias ? ` AS ${selection.alias}` : ''}`,
  };
}
