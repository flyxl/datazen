import type { DatabaseType } from '../../types';

export interface ConnectionViewProps {
  connectionId: string;
  connectionName: string;
  databaseType: DatabaseType;
  initialDatabase?: string;
}
