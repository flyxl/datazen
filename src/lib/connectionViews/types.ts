import type { DatabaseType } from '../../types';

export interface ConnectionViewProps {
  connectionId: string;
  /** Persistent saved-connection ID (stable across restarts). */
  configId: string;
  connectionName: string;
  databaseType: DatabaseType;
  initialDatabase?: string;
}
