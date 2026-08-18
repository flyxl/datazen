import type { MutableRefObject } from 'react';
import type { DatabaseType } from '../../types';

export interface ConnectionViewProps {
  connectionId: string;
  /** Persistent saved-connection ID (stable across restarts). */
  configId: string;
  connectionName: string;
  databaseType: DatabaseType;
  initialDatabase?: string;
  /** When true, the view hides its own schema sidebar (used when an outer navigator tree is present). */
  hideSidebar?: boolean;
  /** Ref for the parent to receive the view's table-selection handler. */
  selectTableRef?: MutableRefObject<((table: string, schema?: string) => void) | undefined>;
}
