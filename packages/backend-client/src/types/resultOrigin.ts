/**
 * Result-origin DTO mirrors: the wire shapes of the query result grid's
 * write-back verdict.
 *
 * These are copies, not designs — the backend owns the rules, this file only
 * names the shapes that travel. The verdict itself is **not** recomputed on
 * this side: only the driver knows where the rows came from, so a second
 * implementation would drift and show up as "the grid says you may edit, the
 * write always fails".
 */

import type { TableValue } from './tableData';

/** A relation the result reads from — the real name, never an alias. */
export interface ResultTableRef {
  catalog: string | null;
  schema: string | null;
  name: string;
  aliased: boolean;
}

/** One column in both names: the result's name and the base column's name. */
export interface IdentityColumn {
  column: string;
  sourceColumn: string;
}

/**
 * Why the backend refuses to write a result back.
 *
 * The list is closed: the backend has no other way to say "no", and every
 * entry has a message on the interface side.
 */
export type GridReadOnlyReason =
  | 'unknownOrigin'
  | 'ambiguousOrigin'
  | 'multiTable'
  | 'aggregate'
  | 'distinct'
  | 'setOperation'
  | 'derivedSource'
  | 'view'
  | 'noPrimaryKey'
  | 'keyNotSelected'
  | 'sessionReadOnly';

/** The verdict on one result. */
export interface ResultEditability {
  /** Whether the result may be written back at all. */
  writable: boolean;
  /** The key columns, in both names. */
  identityColumns: IdentityColumn[];
  /** Every column that is a straight copy of a base-table column. */
  editableColumns: IdentityColumn[];
  /** The relation the rows come from. */
  table: ResultTableRef | null;
  /** Why the result is read-only; `null` exactly when it is writable. */
  reason: GridReadOnlyReason | null;
}

/**
 * Ask where a result came from, and whether it may be written back.
 *
 * The connection is **not** part of the request: the back end resolves it from
 * the session the result was read from, so both surfaces of a result (the
 * grid and the row-detail drawer) can ask without knowing which connection
 * the session belongs to.
 */
export interface ResolveResultOriginRequest {
  /** The session the result was read from. */
  dbSessionId: string;
  /** The database the result was read from. */
  database: string | null;
  /** The schema the result was read from. */
  schema: string | null;
  /** The statement the result came from. */
  sql: string;
  /** The column names the executed result reports. */
  resultColumns: string[];
}

/** One cell edit of a query result. */
export interface ResultCellChangeRequest {
  /** The connection the result was read from. */
  connectionId: string;
  /** The session the result was read from. */
  dbSessionId: string;
  /** The driver that read the result. */
  driverType: string;
  /** The database the relation lives in. */
  database: string | null;
  /** The schema the relation lives in. */
  schema: string | null;
  /** The real name of the relation. */
  table: string;
  /** The statement the result came from. */
  sql: string;
  /** The column names the executed result reports. */
  resultColumns: string[];
  /** The key columns, in both names. */
  identityColumns: IdentityColumn[];
  /** The row identity: source column name → the value the result showed. */
  rowIdentity: Record<string, TableValue | null>;
  /** The result column the user edited. */
  column: string;
  /** The real column name of the edited result column. */
  sourceColumn: string;
  /** The value the result showed in that column. */
  originalValue: TableValue | null;
  /** The new value. */
  value: TableValue | null;
  /** How many rows the request would write. Only `1` is allowed. */
  rowCount: number;
}
