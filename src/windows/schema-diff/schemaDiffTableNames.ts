import type { TableInfo } from '../../types';

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
}

export function enabledTableNames(picks: SchemaDiffTablePick[]): string[] {
  return picks.filter((row) => row.enabled).map((row) => row.name);
}
