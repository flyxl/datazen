import type { TableInfo } from '../types';
import { overlayColumnMap, type SqlNamespace } from './sqlNamespace';

export interface BuildEditorSchemaInput {
  namespaceTree: SqlNamespace;
  tables: TableInfo[];
  views: TableInfo[];
  columnMap: Record<string, string[]>;
  currentDatabase?: string | null;
  hoistPath?: readonly string[];
}
