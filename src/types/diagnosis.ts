/** Schema docs, connection diagnosis, query analysis, and data-sync types. */

// ── Phase 8: Schema docs + Connection diagnosis + Query analysis ──

export interface ConnectionDiagnosis {
  diagnosis: string;
  possibleCauses: string[];
  solutions: ConnectionSolution[];
  category: string;
}

export interface ConnectionSolution {
  description: string;
  command?: string;
}

export interface QueryCategory {
  name: string;
  count: number;
  examples: string[];
}

export interface QueryAnalysis {
  summary: string;
  categories: QueryCategory[];
  insights: string[];
  frequentTables: string[];
  recommendations: string[];
}

// ── Data Sync types ──

export type TableCompareStatus = 'identical' | 'different' | 'source_only' | 'target_only';

export type SyncObjectKind = 'table' | 'view' | 'function' | 'procedure';

export interface TableComparison {
  table: string;
  kind?: SyncObjectKind;
  status: TableCompareStatus;
  sourceRows: number | null;
  targetRows: number | null;
}

export interface ColumnDiffEntry {
  name: string;
  dataType: string;
  nullable: boolean;
  isPrimaryKey: boolean;
}

export interface ChangedColumnDiff {
  name: string;
  source: ColumnDiffEntry;
  target: ColumnDiffEntry;
  changes: string[];
}

export interface TableSchemaDiff {
  table: string;
  /** Present on source, missing on target → ADD on deploy. */
  addedColumns: ColumnDiffEntry[];
  /** Present on target, missing on source → DROP on deploy. */
  removedColumns: ColumnDiffEntry[];
  changedColumns: ChangedColumnDiff[];
}

export interface McpServerConfig {
  id: string;
  name: string;
  transport: 'stdio' | 'sse' | 'streamable_http';
  command?: string;
  args?: string[];
  env?: Record<string, string>;
  url?: string;
  headers?: Record<string, string>;
  enabled?: boolean;
}

export interface McpToolInfo {
  serverId: string;
  toolName: string;
  qualifiedName: string;
  description?: string;
  inputSchema: Record<string, unknown>;
}
