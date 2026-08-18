import type { MutableRefObject } from 'react';
import type { DatabaseType } from '../../types';

export interface NodeContextMenuPayload {
  kind: string;
  name: string;
  x: number;
  y: number;
  schema?: string;
}

export interface ConnectionViewActions {
  newQuery: (initialSql?: string) => void;
  openErDiagram: (focusTable?: string) => void;
  refresh: () => void;
  openObject?: (
    kind: 'function' | 'procedure' | 'trigger' | 'sequence' | 'type',
    name: string,
    schema?: string,
  ) => void;
}

export interface ConnectionViewProps {
  connectionId: string;
  /** Persistent saved-connection ID (stable across restarts). */
  configId: string;
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
