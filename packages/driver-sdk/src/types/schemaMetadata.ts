export type TableType = 'table' | 'view' | 'materializedView' | 'systemTable';

export interface TableOptionsSnapshot {
  comment?: string | null;
  engine?: string | null;
  charset?: string | null;
}

export interface TableInfo {
  name: string;
  /** `driver-api` declares both as `Option<_>` without `skip_serializing_if`, so
   *  serde puts `null` on the wire — not `undefined`. `?:` alone is a lie that
   *  silently rejects the backend's own payload. */
  schema?: string | null;
  tableType: TableType;
  rowCount?: number | null;
}

export interface ColumnSchema {
  name: string;
  dataType: string;
  nullable: boolean;
  defaultValue?: string | null;
  isPrimaryKey?: boolean;
  isAutoIncrement?: boolean;
  comment?: string;
}

export interface IndexInfo {
  name: string;
  columns: string[];
  isUnique: boolean;
  isPrimary: boolean;
  indexType?: string;
}

export interface ForeignKeyInfo {
  name: string;
  columns: string[];
  referencedTable: string;
  referencedColumns: string[];
  onUpdate?: string;
  onDelete?: string;
  deferrability?:
    | 'unknown'
    | 'notDeferrable'
    | 'deferrableInitiallyImmediate'
    | 'deferrableInitiallyDeferred';
}

export interface TableSchema {
  tableName: string;
  columns: ColumnSchema[];
  primaryKeys: string[];
  indexes: IndexInfo[];
  foreignKeys: ForeignKeyInfo[];
  tableOptions?: TableOptionsSnapshot;
}

/** Catalog names are exact names, not quoted SQL text. */
export interface CatalogScope {
  database: string;
  schema: string | null;
}

export interface RelationRef extends CatalogScope {
  name: string;
}

export interface SessionRelationRef extends RelationRef {
  dbSessionId: string;
}

export interface RelationSummary {
  ref: RelationRef;
  kind: TableType;
  rowCount: number | null;
}

export interface RelationColumns {
  ref: RelationRef;
  columns: ColumnSchema[];
  primaryKeys: string[];
}

export interface RelationSchema {
  ref: RelationRef;
  definition: TableSchema;
}

export interface ListCatalogOutput {
  database: string;
  schemas: string[];
  relations: RelationSummary[];
}

export interface MetadataReadError {
  code: 'unsupported' | 'read-failed';
  message: string;
}

export type ColumnsReadResult =
  | { status: 'ok'; value: RelationColumns }
  | { status: 'error'; ref: RelationRef; error: MetadataReadError };

export interface ReadColumnsOutput {
  results: ColumnsReadResult[];
}

export type MetadataRefreshScope =
  | { kind: 'session' }
  | { kind: 'database'; database: string }
  | { kind: 'relation'; relation: RelationRef };

/** Tuple encoding preserves names containing dots, colons or slashes. */
export function relationKey(ref: SessionRelationRef): string {
  return JSON.stringify([ref.dbSessionId, ref.database, ref.schema, ref.name]);
}
