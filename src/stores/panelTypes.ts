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
  database?: string;
  tableSchema?: string;
  subTab: SubTabId;
  structureEditing?: boolean;
}

export interface ViewPanel extends PanelBase {
  type: 'view';
  viewName: string;
  database?: string;
  viewSchema?: string;
  subTab: SubTabId;
}

export interface QueryPanel extends PanelBase {
  type: 'query';
  title: string;
  database?: string;
  schema?: string;
  namespacePath?: string[];
}

export interface CreateTablePanel extends PanelBase {
  type: 'create-table';
  database?: string;
  tableSchema?: string;
}

export interface ErDiagramPanel extends PanelBase {
  type: 'er-diagram';
  focusTable?: string;
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
  objectSchema?: string;
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
