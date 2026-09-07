import type { SqlTokenKind } from './tokens';

/** Inclusive-exclusive document offset range. */
export type SqlTextRange = {
  from: number;
  to: number;
};

export type SqlStatementKind =
  | 'select'
  | 'insert'
  | 'update'
  | 'delete'
  | 'merge'
  | 'ddl'
  | 'transaction'
  | 'other';

export type SqlStatementRange = SqlTextRange & {
  index: number;
  contentFrom: number;
  contentTo: number;
  delimiterFrom: number | null;
  delimiterTo: number | null;
  firstExecutableLine: number;
  kindHint?: SqlStatementKind;
  confidence: 'exact' | 'degraded';
};

export type SqlExecutionTarget = {
  source: 'selection' | 'current-statement' | 'gutter' | 'all';
  sql: string;
  range: SqlTextRange | null;
  statementIndex: number | null;
  documentVersion: number;
};

export type SqlToken = {
  kind: SqlTokenKind;
  from: number;
  to: number;
  text: string;
  /** Parenthesis depth at token start (outside strings/comments). */
  parenDepth: number;
};

export type ScanResult = {
  tokens: readonly SqlToken[];
  /** Final parenthesis depth; non-zero may indicate incomplete SQL. */
  finalParenDepth: number;
};

export type StatementIndexCacheKey = {
  /** Stable document identity (e.g. panel id + doc hash prefix). */
  docIdentity: string;
  dialectId: string;
  revision: number;
};

export type StatementIndexSnapshot = {
  revision: number;
  ranges: readonly SqlStatementRange[];
  sourceLength: number;
};

/** One segment of a possibly qualified SQL identifier. */
export type SqlIdentifierSegment = {
  name: string;
  quoted: boolean;
};

/** Database / schema / relation path without alias. */
export type QualifiedRelationId = {
  namespacePath: readonly SqlIdentifierSegment[];
  name: SqlIdentifierSegment;
};

export type SqlRelationSourceKind = 'table' | 'view' | 'cte' | 'subquery';

export type SqlRelationBinding = {
  relation: QualifiedRelationId;
  alias?: string;
  sourceRange: SqlTextRange;
  aliasRange?: SqlTextRange;
  sourceKind: SqlRelationSourceKind;
};

export type SqlCteBinding = {
  name: string;
  nameRange: SqlTextRange;
  columnNames?: readonly string[];
  columnListRange?: SqlTextRange;
  queryScopeId: string;
};

export type SqlProjectionAlias = {
  alias: string;
  aliasRange: SqlTextRange;
  expressionRange: SqlTextRange;
};

export type SqlScope = {
  id: string;
  range: SqlTextRange;
  parentId?: string;
  kind: 'select' | 'insert' | 'update' | 'delete' | 'subquery' | 'cte';
  ctes: readonly SqlCteBinding[];
  relations: readonly SqlRelationBinding[];
  projectionAliases: readonly SqlProjectionAlias[];
};

export type SqlReferenceKind = 'relation' | 'column' | 'alias';

export type SqlReference = {
  kind: SqlReferenceKind;
  text: string;
  range: SqlTextRange;
  scopeId: string;
  qualifier?: string;
  resolved?: boolean;
};

export type SqlSemanticDiagnosticSeverity = 'info' | 'warning' | 'error';

export type SqlSemanticDiagnostic = {
  message: string;
  range: SqlTextRange;
  severity: SqlSemanticDiagnosticSeverity;
};

export type SqlCursorIntentKind =
  | 'relation'
  | 'qualified_column'
  | 'projection'
  | 'join_target'
  | 'function_call'
  | 'insert_values'
  | 'insert_columns'
  | 'unknown';

export type SqlCursorIntent = {
  kind: SqlCursorIntentKind;
  prefix: string;
  replacementRange: SqlTextRange;
  qualifierParts: readonly string[];
  scopeId?: string;
};

export type SqlSemanticModel = {
  statement: SqlStatementRange;
  tokens: readonly SqlToken[];
  scopes: readonly SqlScope[];
  references: readonly SqlReference[];
  cursorIntent: SqlCursorIntent;
  diagnostics: readonly SqlSemanticDiagnostic[];
};

export type SqlDialectQuoteStyle = 'double' | 'backtick' | 'bracket' | 'none';

export type SqlProjectionAliasVisibility = 'select-only' | 'order-group' | 'broad';

export type SqlParameterPolicy = {
  atNamed: boolean;
  question: boolean;
  dollarPositional: boolean;
  template: boolean;
};

export type SqlDialectAdapter = {
  dialectId: string;
  quoteStyle: SqlDialectQuoteStyle;
  foldUnquotedIdentifier(value: string): string;
  shouldQuoteIdentifier(value: string): boolean;
  quoteIdentifier(value: string): string;
  unquoteIdentifier(value: string): string | null;
  parseQualifiedName(text: string): QualifiedRelationId | null;
  compareIdentifiers(a: string, b: string): boolean;
  projectionAliasVisibility: SqlProjectionAliasVisibility;
  parameterPolicy: SqlParameterPolicy;
};

export type RelationResolveStatus = 'unique' | 'ambiguous' | 'unresolved';

export type RelationResolveResult = {
  status: RelationResolveStatus;
  binding?: SqlRelationBinding;
  candidates?: readonly SqlRelationBinding[];
};
