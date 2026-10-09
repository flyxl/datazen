/**
 * Connection view contracts shared by host tab/shell and driver connection views.
 *
 * Type-only: describes the props a driver-provided ConnectionView receives
 * from the host workspace shell, plus the ref-callback payloads it may fill.
 */
import type { MutableRefObject } from 'react';
import type { DatabaseType } from '../../../../src/types';
import type { TableContextInput, TableSqlActionKind } from '../../../../src/lib/tableSqlActions';
import type { KvSlotState } from './kv-slots';

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
  newQuery: (
    initialSql?: string,
    context?: Pick<TableContextInput, 'database' | 'schema'>,
    title?: string,
  ) => boolean | void;
  openTableAction?: (context: TableContextInput, action: TableSqlActionKind) => void;
  openSqlFile?: () => void;
  createTable?: () => void;
  openCreateDatabase?: () => void;
  openCreateSchema?: () => void;
  openCreateUser?: () => void;
  openErDiagram: (focusTable?: string, database?: string) => void;
  /**
   * Open a table's structure. The navigator's table context menu needs this to
   * offer 打开结构 — `buildSchemaTreeContextMenuItems` drops the item unless
   * `showOpenStructure` is set, and the navigator has no other way to know the
   * host exposes a structure action.
   */
  openTableStructure?: (tableName: string) => void;
  refresh: () => void;
  openObject?: (
    kind: 'function' | 'procedure' | 'trigger' | 'sequence' | 'type',
    name: string,
    schema?: string,
    signature?: string,
    targetSchema?: string,
    targetName?: string,
    database?: string,
  ) => void;
  openQueryHistory?: () => void;
  /** 打开目标连接的服务器仪表盘；ctx 由右键菜单显式传入被点击的连接。 */
  openServerStatus?: (ctx?: ConnectionOpenTarget) => void;
  /** 打开目标连接的进程列表；ctx 由右键菜单显式传入被点击的连接。 */
  openProcessList?: (ctx?: ConnectionOpenTarget) => void;
}

export interface ConnectionViewProps {
  /**
   * Identity of the tab this view is rendered into.
   *
   * **Opaque.** It is a key and nothing else: do not parse it, do not assume a
   * format or a counter, do not infer anything from it beyond "this string is
   * the one and only identity of my tab". Its format is host-internal and may
   * change at any time; only its uniqueness and stability matter.
   *
   * Unique per tab and stable for the tab's whole life. Unlike `dbSessionId` it
   * does NOT repeat across tabs: two tabs on the same database of one connection
   * share a `dbSessionId` but never share a `panelId`. Use it to key any state
   * that must stay with this tab without leaking into a sibling.
   *
   * Session-scoped, like the tab itself. The host does not persist it, so do not
   * persist anything keyed by it either.
   */
  panelId: string;
  /**
   * Register a callback for when the host closes this tab, and get an
   * unsubscribe back. The callback fires at most once, after the tab is gone.
   *
   * Use it to drop per-tab caches. It is the ONLY reliable disposal signal:
   *
   * - Do **not** clean up on unmount. The host renders only the active tab, so
   *   switching tabs unmounts this view too — and the state must survive that.
   * - Do clean up here, because a tab can be closed while it is unmounted, and
   *   only the host knows it is gone.
   *
   * The host keeps the registration alive across unmounts, so registering once
   * is enough; re-registering on every mount is harmless but pointless.
   */
  onPanelClosed: (handler: () => void) => () => void;
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
  /**
   * Host-owned per-panel KV state relay, present only on a KV panel whose driver
   * declared a `kvWorkspace` capability in its `DatabaseTypeMeta`. The workbench
   * publishes the selected key and the dirty flag here so the host-rendered KV
   * slots (context bar / status bar / key-props sidebar) read the same values.
   */
  kvSlotState?: KvSlotState;
}
