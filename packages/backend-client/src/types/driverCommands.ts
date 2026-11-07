/**
 * Driver command catalog DTOs and query-stream events (the port shapes).
 *
 * Moved from the host `src/types` so that `@datazen/driver-sdk` can consume
 * them without reaching into host code (a package must never import the host).
 */

import type { ResultCompleteness } from './jobs';
import type { ResultProvenance, RuntimeResultBinding, StatementResultSource } from './session';

export type Value = string | number | boolean | null | Record<string, unknown> | unknown[]
;

export interface ColumnInfo {
  name: string;
  dataType: string;
  nullable: boolean;
}

export interface QueryResult {
  columns: ColumnInfo[];
  rows: (Value | null)[][];
  rowsAffected?: number;
  executionTimeMs: number;
}

export interface StatementResult {
  source?: StatementResultSource | null;
  runtimeBinding?: RuntimeResultBinding | null;
  provenance?: ResultProvenance | null;
  sql: string;
  columns: ColumnInfo[];
  rows: (Value | null)[][];
  rowsAffected?: number;
  executionTimeMs: number;
  truncated?: boolean;
  pinned?: boolean;
}

export interface MultiQueryResult {
  results: StatementResult[];
  totalTimeMs: number;
}

export interface QueryResultArtifacts {
  executionId: string;
  artifactIds: readonly string[];
  resultCompleteness: ResultCompleteness;
}

export type QueryStreamEvent =
  | { type: 'executionStarted'; executionId: string }
  | { type: 'publishedArtifacts'; artifacts: QueryResultArtifacts }
  | { type: 'statementStart'; index: number; sql: string; columns: ColumnInfo[] }
  | { type: 'rows'; index: number; rows: (Value | null)[][] }
  | { type: 'statementSnapshot'; index: number; result: StatementResult }
  | {
      type: 'statementEnd';
      index: number;
      rowsAffected?: number;
      executionTimeMs: number;
      truncated: boolean;
    }
  | { type: 'done'; totalTimeMs: number };

export type CommandCategory = 'query' | 'mutate' | 'admin' | 'observe' | 'pubSub' | 'stream' | 'io'
;

export type CommandAccessLevel = 'read' | 'write' | 'highRisk'
;

export interface DriverSaveDialogSpec {
  fileNameField: string;
  dataBase64Field: string;
  filterName: string;
  extensions: string[];
  resultPathField: string;
}

export interface DriverCommandMetadata {
  category: CommandCategory;
  risk?: CommandAccessLevel | null;
  workflow: boolean;
  ui: boolean;
  deprecated: boolean;
  replacedBy?: string | null;
  requiresConnection: boolean;
  saveDialog?: DriverSaveDialogSpec | null;
}

export interface DriverCommandDefinition {
  id: string;
  name: string;
  description?: string | null;
  inputSchema: Record<string, unknown>;
  outputSchema?: Record<string, unknown> | null;
  permissions: string[];
  metadata: DriverCommandMetadata;
}
