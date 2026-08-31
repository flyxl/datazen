import { escapeIdent } from './databaseTypes';
import type { DatabaseType } from '../types';

export type TableSqlActionKind = 'openData' | 'select' | 'insert' | 'update' | 'ddl';

export type QueryOpenSource = 'table-action' | 'object-search' | 'ai-action';

/** Stable table identity shared by TablePanel and SQL-opening actions. */
export interface TableContext {
  connectionId: string;
  dbSessionId: string;
  databaseType: DatabaseType;
  database?: string;
  schema?: string;
  tableName: string;
}

/** Structural input accepted from an existing TablePanel, ViewPanel, or search result. */
export interface TableContextInput {
  connectionId: string;
  dbSessionId: string;
  databaseType: DatabaseType;
  database?: string | null;
  schema?: string | null;
  tableName?: string;
  tableSchema?: string | null;
  viewName?: string;
  viewSchema?: string | null;
  objectType?: string;
  name?: string;
}

export interface QueryOpenContext extends TableContext {
  source: QueryOpenSource;
  action: TableSqlActionKind;
  initialSql?: string;
  focus?: 'editor' | 'result';
}

export interface TableSqlActionSpec {
  kind: TableSqlActionKind;
  source?: QueryOpenSource;
  /** An explicitly driver-generated draft may be supplied by a later integration layer. */
  initialSql?: string;
  focus?: 'editor' | 'result';
}

export interface TableSqlAction {
  id: TableSqlActionKind;
  kind: TableSqlActionKind;
  label: string;
  description: string;
  context: QueryOpenContext;
  /** Alias for consumers that use the contract name from the implementation plan. */
  queryOpenContext: QueryOpenContext;
  /** Draft text only; it is never sent to a command by this module. */
  sqlTemplate?: string;
  execution: 'draft-only';
  /** Insert/update/DDL need a driver or existing command to make a dialect-specific draft. */
  requiresDriverGeneration: boolean;
}

export const TABLE_SQL_ACTION_KINDS: readonly TableSqlActionKind[] = [
  'openData',
  'select',
  'insert',
  'update',
  'ddl',
];

const ACTION_COPY: Record<
  TableSqlActionKind,
  { label: string; description: string; requiresDriverGeneration: boolean }
> = {
  openData: {
    label: 'Open Data',
    description: 'Open the existing Table Panel data view.',
    requiresDriverGeneration: false,
  },
  select: {
    label: 'SELECT',
    description: 'Open a read-only SELECT draft for the table.',
    requiresDriverGeneration: false,
  },
  insert: {
    label: 'INSERT',
    description: 'Open an INSERT draft with driver-specific columns and values to be supplied.',
    requiresDriverGeneration: true,
  },
  update: {
    label: 'UPDATE',
    description: 'Open an UPDATE draft with a driver-specific assignment and predicate.',
    requiresDriverGeneration: true,
  },
  ddl: {
    label: 'DDL',
    description: 'Open the driver-generated DDL entry point for the table.',
    requiresDriverGeneration: true,
  },
};

function normalizeRequired(value: string | undefined, field: string): string {
  const normalized = value?.trim();
  if (!normalized) throw new Error(`Missing ${field} for table action`);
  return normalized;
}

function normalizeOptional(value: string | null | undefined): string | undefined {
  const normalized = value?.trim();
  return normalized ? normalized : undefined;
}

function readTableName(input: TableContextInput): string {
  const objectType = input.objectType?.trim().toLowerCase();
  if (
    objectType &&
    !['table', 'view', 'column', 'materializedview'].includes(objectType)
  ) {
    throw new Error(`Object type ${input.objectType} cannot produce a table context`);
  }

  return normalizeRequired(
    input.tableName ?? input.viewName ?? input.name,
    'tableName',
  );
}

/**
 * Build the one table context accepted by TablePanel and SQL actions.
 * Only identity/context fields are copied; credentials and execution state are not accepted.
 */
export function buildTableContext(input: TableContextInput): TableContext {
  return {
    connectionId: normalizeRequired(input.connectionId, 'connectionId'),
    dbSessionId: normalizeRequired(input.dbSessionId, 'dbSessionId'),
    databaseType: input.databaseType,
    database: normalizeOptional(input.database),
    schema: normalizeOptional(input.schema ?? input.tableSchema ?? input.viewSchema),
    tableName: readTableName(input),
  };
}

/** Quote each schema/table segment through the existing database metadata registry. */
export function quoteTableIdentifier(context: TableContext): string {
  return [context.schema, context.tableName]
    .filter((segment): segment is string => Boolean(segment))
    .map((segment) => escapeIdent(segment, context.databaseType))
    .join('.');
}

/**
 * Safe draft templates. SELECT is portable; mutating/DDL templates intentionally contain
 * comments instead of guessed columns, values, predicates, or dialect syntax. A driver or
 * existing command must replace those placeholders before any later integration executes SQL.
 */
export function buildSafeTableSqlTemplate(
  context: TableContext,
  action: TableSqlActionKind,
): string | undefined {
  const quotedTable = quoteTableIdentifier(context);
  switch (action) {
    case 'openData':
      return undefined;
    case 'select':
      return `SELECT * FROM ${quotedTable};`;
    case 'insert':
      return `INSERT INTO ${quotedTable} /* driver-generated columns and values */;`;
    case 'update':
      return `UPDATE ${quotedTable} SET /* driver-generated assignments */ WHERE /* driver-generated predicate */;`;
    case 'ddl':
      return `/* Driver-generated DDL for ${quotedTable}; use the existing DDL command. */`;
  }
}

function normalizeAction(
  action: TableSqlActionKind | TableSqlActionSpec,
): TableSqlActionSpec {
  return typeof action === 'string' ? { kind: action } : action;
}

/** Build the context handed to the existing QueryPanel/SQL Editor integration. */
export function buildQueryOpenContext(
  tableContext: TableContextInput,
  action: TableSqlActionKind | TableSqlActionSpec,
): QueryOpenContext {
  const context = buildTableContext(tableContext);
  const spec = normalizeAction(action);
  const initialSql =
    spec.initialSql ?? buildSafeTableSqlTemplate(context, spec.kind);
  return {
    ...context,
    source: spec.source ?? 'table-action',
    action: spec.kind,
    ...(initialSql === undefined ? {} : { initialSql }),
    focus: spec.focus ?? (spec.kind === 'openData' ? 'result' : 'editor'),
  };
}

/** Build one non-executing action description for a table or view result. */
export function buildTableSqlAction(
  tableContext: TableContextInput,
  action: TableSqlActionKind | TableSqlActionSpec,
): TableSqlAction {
  const spec = normalizeAction(action);
  const queryOpenContext = buildQueryOpenContext(tableContext, spec);
  const copy = ACTION_COPY[spec.kind];
  return {
    id: spec.kind,
    kind: spec.kind,
    label: copy.label,
    description: copy.description,
    context: queryOpenContext,
    queryOpenContext,
    sqlTemplate: queryOpenContext.initialSql,
    execution: 'draft-only',
    requiresDriverGeneration: copy.requiresDriverGeneration,
  };
}

/** Build the stable action list in the product-defined order. */
export function buildTableSqlActions(
  tableContext: TableContextInput,
  actions: readonly TableSqlActionKind[] = TABLE_SQL_ACTION_KINDS,
): TableSqlAction[] {
  return actions.map((kind) => buildTableSqlAction(tableContext, kind));
}
