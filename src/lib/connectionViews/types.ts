import type { MutableRefObject } from 'react';
import type { DatabaseType } from '../../types';
import type { TableContextInput, TableSqlActionKind } from '../tableSqlActions';

export interface NodeContextMenuPayload {
  kind: string;
  name: string;
  x: number;
  y: number;
  schema?: string;
}

/**
 * 右键菜单打开「服务器仪表盘 / 进程列表」时的显式目标连接。
 * 由调用方（连接树右键）把用户实际点击的连接传进来，面板据此绑定，
 * 不再依赖全局「当前活动连接」，避免 MySQL/PG 面板串数据。
 */
export interface ConnectionOpenTarget {
  /** Persistent connection id the user clicked. */
  connectionId: string;
  /** Live database session id for that connection (may be resolved by the host). */
  dbSessionId: string;
  connectionName: string;
  databaseType: DatabaseType;
}

export interface ConnectionViewActions {
  newQuery: (initialSql?: string, context?: Pick<TableContextInput, 'database' | 'schema'>) => void;
  openTableAction?: (context: TableContextInput, action: TableSqlActionKind) => void;
  openSqlFile?: () => void;
  createTable?: () => void;
  openCreateDatabase?: () => void;
  openCreateSchema?: () => void;
  openCreateUser?: () => void;
  openErDiagram: (focusTable?: string) => void;
  refresh: () => void;
  openObject?: (
    kind: 'function' | 'procedure' | 'trigger' | 'sequence' | 'type',
    name: string,
    schema?: string,
  ) => void;
  openQueryHistory?: () => void;
  /** 打开目标连接的服务器仪表盘；ctx 由右键菜单显式传入被点击的连接。 */
  openServerStatus?: (ctx?: ConnectionOpenTarget) => void;
  /** 打开目标连接的进程列表；ctx 由右键菜单显式传入被点击的连接。 */
  openProcessList?: (ctx?: ConnectionOpenTarget) => void;
}

export interface ConnectionViewProps {
  /** Live database session id used for every query/IPC this view issues. */
  dbSessionId: string;
  /** Persistent saved-connection ID (stable across restarts). */
  connectionId: string;
  connectionName: string;
  databaseType: DatabaseType;
  initialDatabase?: string;
  /** When true, the view hides its own schema sidebar (used when an outer navigator tree is present). */
  hideSidebar?: boolean;
  /** Whether this view is the currently active (visible) tab. Shared refs are only wired when true. */
  isActive?: boolean;
  /** Ref for the parent to receive the view's table-selection handler. */
  selectTableRef?: MutableRefObject<((table: string, schema?: string) => void) | undefined>;
  /** Ref for the parent to receive the view's context-menu handler (for table/view/blank nodes). */
  nodeContextMenuRef?: MutableRefObject<((payload: NodeContextMenuPayload) => void) | undefined>;
  /** Ref for the parent to receive direct action callbacks from the view. */
  actionsRef?: MutableRefObject<ConnectionViewActions | undefined>;
}
