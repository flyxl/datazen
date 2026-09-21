/** Schema comparison and data-sync types. */

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
