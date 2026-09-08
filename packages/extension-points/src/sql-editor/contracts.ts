import type { SQLDialect } from '@codemirror/lang-sql';
import { PostgreSQL, MySQL, MariaSQL, SQLite, MSSQL, StandardSQL } from '@codemirror/lang-sql';
import type { DatabaseType } from './databaseTypes';
import { DB_REGISTRY } from './databaseTypes';
import type { EditorMetadataSnapshot } from './types';

export type SqlNamespace = { [name: string]: SqlNamespace } | readonly string[];

/** Nested schema for CodeMirror SQL autocompletion */
export type SqlSchema = SqlNamespace;

export interface DroppedTablePayload {
  tables: Array<{ tableName: string; schema?: string }>;
  connectionId?: string;
  dbSessionId?: string;
  databaseType?: string;
}

export interface SqlEditorHandle {
  getSelection: () => string;
  /** Toggle `-- ` comments on selected lines (or the line containing the cursor). */
  toggleLineComment: () => void;
  /** Insert SQL text at the given position (or append at the end if pos is null/undefined). */
  insertAt: (text: string, pos?: number | null) => void;
  /** Get the current document version counter (for execution state tracking). */
  getDocumentVersion?: () => number;
  /** Get the full document text (for context-aware drop insertion). */
  getDocument?: () => string;
  /** Insert raw text at a position without newline padding. */
  rawInsert?: (text: string, pos: number) => void;
  /** Focus the editor. */
  focus?: () => void;
}

export interface SqlEditorProps {
  value: string;
  onChange: (value: string) => void;
  onExecute?: () => void;
  onExecuteSelection?: (sql: string) => void;
  /** Execute all statements in the query (Cmd/Ctrl + Shift + Enter). */
  onExecuteAll?: () => void;
  /** Save query (Cmd/Ctrl + S). */
  onSaveQuery?: () => void;
  onContextMenu?: (e: MouseEvent, selectedSql: string) => void;
  onQualifiedPath?: (parents: string[]) => void;
  placeholder?: string;
  schema?: SqlSchema;
  databaseType?: string;
  /** Active database name for metadata isolation and hover resolution. */
  database?: string;
  /** True while a lazy namespace path is fetching for autocomplete. */
  namespaceLoading?: boolean;
  /** CodeMirror: tables in this schema complete without a prefix (`public.users` → `users`). */
  defaultSchema?: string;
  /** CodeMirror: columns of this table complete at the top level (WHERE / SELECT). */
  defaultTable?: string;
  className?: string;
  /** Callback when tables are dropped into the editor from SchemaTree. */
  onDropTable?: (payload: DroppedTablePayload, pos: number | null) => void;

  // ── S6-D: Metadata & execution wiring ──────────────────────────────
  /** Immutable metadata snapshot for completion/hover/navigation. */
  metadataSnapshot?: EditorMetadataSnapshot;
  /** Current execution state for gutter spinner display. */
  executionStatus?: 'idle' | 'running' | 'cancelling';
  /** Range of the currently executing statement (null when idle). */
  executingRange?: { from: number; to: number } | null;
  /** Connection identity for cross-connection drop validation. */
  connectionId?: string;

  // ── S6-D: Navigation & DDL callbacks ───────────────────────────────
  /** Navigate to a table data page (Mod+Click on table name). */
  onNavigateToTable?: (target: { database?: string; schema?: string; name: string }) => void;
  /** Navigate to a table structure page (Mod+Click on column name). */
  onNavigateToStructure?: (target: {
    database?: string;
    schema?: string;
    name: string;
    columnName?: string;
  }) => void;
  /** Navigate to a relation DDL page (hover action). */
  onNavigateToDdl?: (target: {
    database?: string;
    schema?: string;
    name: string;
    kind: 'table' | 'view';
  }) => void;

  // ── Completion quoting policy ──────────────────────────────────────
  /** Quoting policy for column autocomplete ('unquoted' | 'always' | 'both'). Default 'unquoted'. */
  completionQuotePolicy?: CompletionQuotePolicy;
}

export type CompletionQuotePolicy = 'unquoted' | 'always' | 'both';

// ── Dialect mapping ───────────────────────────────────────────────────

const CM_DIALECT_MAP: Record<string, SQLDialect> = {
  postgresql: PostgreSQL,
  mysql: MySQL,
  mariadb: MariaSQL,
  sqlite: SQLite,
  sqlserver: MSSQL,
};

/** Resolve CodeMirror SQL dialect; plugins may map via `sqlDialect` (e.g. kiwi → mysql). */
export function resolveCmDialect(dbType?: string): SQLDialect {
  if (!dbType) return StandardSQL;
  if (CM_DIALECT_MAP[dbType]) return CM_DIALECT_MAP[dbType];
  const mapped = DB_REGISTRY[dbType as DatabaseType]?.sqlDialect;
  if (mapped && CM_DIALECT_MAP[mapped]) return CM_DIALECT_MAP[mapped];
  return StandardSQL;
}
