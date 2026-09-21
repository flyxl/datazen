import type { DatabaseType } from '../types';
import type { TrendSeries } from '../lib/serverStatusTrends';

export type SubTabId = 'data' | 'structure' | 'indexes' | 'foreignKeys' | 'ddl';

interface PanelBase {
  id: string;
  connectionId: string;
  dbSessionId: string;
  connectionName: string;
  databaseType: DatabaseType;
}

export interface TablePanel extends PanelBase {
  type: 'table';
  tableName: string;
  database: string;
  tableSchema: string | null;
  subTab: SubTabId;
  structureEditing?: boolean;
  targetColumn?: string;
}

export interface ViewPanel extends PanelBase {
  type: 'view';
  viewName: string;
  database: string;
  viewSchema: string | null;
  subTab: SubTabId;
}

export interface QueryPanel extends PanelBase {
  type: 'query';
  title: string;
  database: string;
  schema: string | null;
  namespacePath?: string[];
}

export interface CreateTablePanel extends PanelBase {
  type: 'create-table';
  database: string;
  tableSchema: string | null;
}

export interface ErDiagramPanel extends PanelBase {
  type: 'er-diagram';
  focusTable?: string;
  /**
   * Schema the diagram reads its tables from, captured from the panel that was
   * active when it was opened. The diagram is database-wide, so it cannot use
   * `resolveTableSchema` (which is per-relation); without this it would fall
   * back to the connection default and render a different namespace than the
   * one the user is looking at. `null` = let the host/driver decide.
   */
  schema?: string | null;
}

export interface ObjectsPanel extends PanelBase {
  type: 'objects';
}

export interface PrivilegesPanel extends PanelBase {
  type: 'privileges';
}

export interface ServerStatusCache {
  status: Record<string, string | number | boolean | null>;
  variables?: { name: string; value: string | null }[];
  history?: Record<string, TrendSeries>;
  updatedAt?: number;
}

export interface ServerStatusPanel extends PanelBase {
  type: 'server-status';
  data?: ServerStatusCache;
}

export interface ProcessListCacheData {
  rows: (string | number | boolean | null)[][];
  columns?: { name: string; dataType: string; nullable?: boolean }[];
}

export interface ProcessesPanel extends PanelBase {
  type: 'processes';
  data?: ProcessListCacheData;
}

export interface DatabaseObjectPanel extends PanelBase {
  type: 'db-object';
  objectKind: 'function' | 'procedure' | 'trigger' | 'sequence' | 'type';
  objectName: string;
  objectSchema: string | null;
}

export interface RedisDbPanel extends PanelBase {
  type: 'redis-db';
  dbName: string;
}

export type Panel =
  | TablePanel
  | ViewPanel
  | QueryPanel
  | CreateTablePanel
  | ErDiagramPanel
  | ObjectsPanel
  | PrivilegesPanel
  | ServerStatusPanel
  | ProcessesPanel
  | DatabaseObjectPanel
  | RedisDbPanel;

let counter = 0;
export function nextPanelId(prefix: string): string {
  counter += 1;
  return `panel-${prefix}-${counter}`;
}

export function resetPanelIdCounter(): void {
  counter = 0;
}

export interface ConnectionContext {
  connectionId: string;
  dbSessionId: string;
  connectionName: string;
  databaseType: DatabaseType;
}

export function resolveNextActive(
  panels: Panel[],
  removedId: string,
  currentActiveId: string | null,
): string | null {
  if (currentActiveId !== removedId) return currentActiveId;
  const idx = panels.findIndex((p) => p.id === removedId);
  if (idx < 0) return null;
  const remaining = panels.filter((p) => p.id !== removedId);
  if (remaining.length === 0) return null;
  return remaining[Math.min(idx, remaining.length - 1)].id;
}

/**
 * The schema a panel is currently scoped to, or `null` when it carries none.
 *
 * Used by panels that need a *database-wide* schema (the ER diagram) so they
 * follow the panel the user is looking at instead of the connection default.
 * `null` is meaningful and must not be replaced by the database name: a schema
 * is a namespace *inside* a database, and schema-less drivers reject it.
 */
export function panelSchema(panel: Panel | null | undefined): string | null {
  if (!panel) return null;
  switch (panel.type) {
    case 'table':
      return panel.tableSchema ?? null;
    case 'view':
      return panel.viewSchema ?? null;
    case 'query':
      return panel.schema ?? null;
    case 'create-table':
      return panel.tableSchema ?? null;
    case 'er-diagram':
      return panel.schema ?? null;
    case 'db-object':
      return panel.objectSchema ?? null;
    default:
      return null;
  }
}
