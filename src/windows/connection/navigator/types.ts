import type { ConnectionConfig, DatabaseObject, TableInfo } from '../../../types';
import type { ConnectionMatch } from '../../../lib/connectionLocator';
import type { ConnectionOpenTarget } from '../../../lib/connectionViews/types';
import type { SchemaTreeCategoryDef } from '../schema-tree/schemaTreeCategories';
import type { TableContextInput, TableSqlActionKind } from '../../../lib/tableSqlActions';
import type { TreeRowLevel } from '@datazen/ui';

/**
 * `TreeRowLevel` makes `levelDepth` optional so that a tree which never shifts
 * its levels can leave it out. The navigator always shifts them — a search
 * removes exactly one rung — so here it is required: a row that forgets to
 * announce its level would silently fall back to its painted depth and
 * over-nest by one under a search, which is the drift this contract exists to
 * prevent. Building it from the shared type keeps the two in step.
 */
type Announced = Required<Pick<TreeRowLevel, 'depth' | 'levelDepth'>>;

/**
 * Every row that sits inside a nesting ladder: where it is *painted*, and
 * optionally where it is *announced*.
 *
 * These two are not the same number, and they used to disagree — the renderer
 * subtracted a level for a search while the builder painted the unchanged
 * depth, so the rule that made `aria-level` match the visible parent lived in
 * the component. `levelDepth` moves that decision onto the row that owns it;
 * `ariaLevelOf` from `@datazen/ui` is then the only conversion in the tree.
 */
type DepthBearing =
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
      /**
       * Whether this database is open: the user expanded it in the tree, or the
       * driver still holds a per-database resource for it. A closed database
       * renders no marker at all — there is no "closed" indicator.
       */
      isOpen: boolean;
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
      dbName: string;
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
  | {
      type: 'object';
      obj: DatabaseObject;
      depth: number;
      catId: string;
      /** Owner tuple — without it two connections' identically named objects collide. */
      connectionId: string;
      dbName: string;
      schemaName?: string;
    }
  | {
      type: 'kv-db';
      connectionId: string;
      dbSessionId: string;
      dbName: string;
      depth: number;
      isSelected: boolean;
      dbCountsCommand?: string;
    }
  | {
      type: 'db-loading';
      depth: number;
      /**
       * Identity of the row this spinner stands in for (the connection, the
       * session, or the database whose tables are loading). A placeholder keyed
       * by its own list position renames itself the moment the window scrolls.
       */
      ownerKey: string;
    }
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
    };

export type UnifiedRow =
  /**
   * A `section` and a `group` are the tree's roots: they have no parent to be
   * nested under, so they carry no depth and announce at
   * `TREE_TOP_LEVEL`. They are siblings of each other by construction —
   * `buildFlatRows` emits a `section` XOR a `group`, never one inside the
   * other.
   */
  | (Announced & {
      type: 'section';
      section: 'pinned' | 'recent';
      displayName: string;
      count: number;
      expanded: boolean;
    })
  | (Announced & {
      type: 'group';
      groupName: string;
      displayName: string;
      count: number;
      expanded: boolean;
    })
  | (DepthBearing & Announced)
  | (Announced & {
      /**
       * A hint row, not a tree item: the shared shell renders it with no ARIA
       * at all. `groupName` is always present at the emit site, and keeping it
       * optional is what allowed an index to leak into the key.
       */
      type: 'empty-group';
      groupName: string;
    })
  | (Announced & { type: 'no-connections' });

export interface ConnectionNavigatorTreeHandle {
  refreshAllConnections: () => Promise<void>;
  refreshConnection: (connectionId: string) => Promise<void>;
}

export interface ConnectionNavigatorTreeProps {
  onSelectConnection: (connectionId: string) => void;
  onSelectTable: (tableName: string, schema: string | null, database: string) => void;
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
      title?: string,
    ) => boolean | void;
    openTableAction?: (context: TableContextInput, action: TableSqlActionKind) => void;
    openSqlFile?: () => void;
    createTable?: () => void;
    openCreateDatabase?: () => void;
    openCreateSchema?: () => void;
    openCreateUser?: () => void;
    openErDiagram?: (focusTable?: string, database?: string) => void;
    /**
     * Open a table's structure (or the standalone structure editor when the
     * driver supports one). The navigator's table context menu needs this to
     * offer 打开结构; without it `buildSchemaTreeContextMenuItems` omits the
     * item, because the navigator does not set `showOpenStructure` by default.
     */
    openTableStructure?: (tableName: string) => void;
    refresh?: () => void;
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
    openServerStatus?: (ctx?: ConnectionOpenTarget) => void;
    openProcessList?: (ctx?: ConnectionOpenTarget) => void;
  };
}

export const NAVIGATOR_ROW_HEIGHT = 28;
