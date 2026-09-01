import type { ConnectionConfig, DatabaseObject, TableInfo } from '../../../types';
import type { ConnectionMatch } from '../../../lib/connectionLocator';
import type { ConnectionOpenTarget } from '../../../lib/connectionViews/types';
import type { SchemaTreeCategoryDef } from '../schema-tree/schemaTreeCategories';
import type { TableContextInput, TableSqlActionKind } from '../../../lib/tableSqlActions';

export type UnifiedRow =
  | {
      type: 'section';
      section: 'pinned' | 'recent';
      displayName: string;
      count: number;
    }
  | { type: 'group'; groupName: string; displayName: string; count: number; expanded: boolean }
  | {
      type: 'connection';
      conn: ConnectionConfig;
      sectionGroup: string;
      isSelected: boolean;
      status: string;
      expanded: boolean;
      depth: number;
      match?: ConnectionMatch;
    }
  | {
      type: 'db';
      connectionId: string;
      dbSessionId: string;
      dbName: string;
      expanded: boolean;
      loading: boolean;
      depth: number;
    }
  | {
      type: 'schema';
      connectionId: string;
      dbName: string;
      schemaName: string;
      expanded: boolean;
      depth: number;
    }
  | {
      type: 'category';
      key: string;
      cat: SchemaTreeCategoryDef;
      count: number;
      expanded: boolean;
      depth: number;
    }
  | {
      type: 'table';
      item: TableInfo;
      depth: number;
      catId: string;
      isSelected: boolean;
      connectionId: string;
      dbSessionId: string;
      dbName: string;
    }
  | { type: 'object'; obj: DatabaseObject; depth: number; catId: string }
  | {
      type: 'kv-db';
      connectionId: string;
      dbSessionId: string;
      dbName: string;
      depth: number;
      isSelected: boolean;
    }
  | { type: 'db-loading'; depth: number }
  | {
      type: 'namespace-node';
      name: string;
      depth: number;
      expanded: boolean;
      isLeaf: boolean;
      leafKind?:
        | 'table'
        | 'view'
        | 'materializedView'
        | 'systemTable'
        | 'function'
        | 'procedure'
        | 'trigger';
      segments: string[];
      key: string;
      connectionId: string;
      dbSessionId: string;
    }
  | { type: 'empty-group' }
  | { type: 'no-connections' };

export interface ConnectionNavigatorTreeHandle {
  refreshAllConnections: () => Promise<void>;
  refreshConnection: (connectionId: string) => Promise<void>;
}

export interface ConnectionNavigatorTreeProps {
  onSelectConnection: (connectionId: string) => void;
  onSelectTable: (tableName: string, schema?: string, database?: string) => void;
  onSelectKvDb?: (connectionId: string, dbName: string) => void;
  activeConnectionId: string | null;
  onNewConnection: (defaultGroup?: string) => void;
  onRefresh?: () => void;
  onEditConnection: (connectionId: string) => void;
  onDeleteConnection: (connectionId: string) => void;
  onDisconnect: (connectionId: string) => void;
  onExportConnections?: () => void;
  onImportConnections?: () => void;
  onCollapseSidebar?: () => void;
  onShowMessage?: (text: string, kind: 'error' | 'success') => void;
  onNodeContextMenu?: (payload: {
    kind: string;
    name: string;
    x: number;
    y: number;
    schema?: string;
  }) => void;
  viewActions?: {
    newQuery?: (
      initialSql?: string,
      context?: Pick<TableContextInput, 'database' | 'schema'>,
    ) => void;
    openTableAction?: (context: TableContextInput, action: TableSqlActionKind) => void;
    openSqlFile?: () => void;
    createTable?: () => void;
    openCreateDatabase?: () => void;
    openCreateSchema?: () => void;
    openCreateUser?: () => void;
    openErDiagram?: (focusTable?: string) => void;
    refresh?: () => void;
    openObject?: (
      kind: 'function' | 'procedure' | 'trigger' | 'sequence' | 'type',
      name: string,
      schema?: string,
    ) => void;
    openQueryHistory?: () => void;
    openServerStatus?: (ctx?: ConnectionOpenTarget) => void;
    openProcessList?: (ctx?: ConnectionOpenTarget) => void;
  };
}

export const NAVIGATOR_ROW_HEIGHT = 28;
