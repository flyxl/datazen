import type {
  SqlCteBinding,
  SqlDialectAdapter,
  SqlProjectionAlias,
  SqlRelationBinding,
  SqlScope,
  SqlTextRange,
  SqlToken,
} from '../types';
import {
  parseDeleteTarget,
  parseFromClause,
  parseInsertTarget,
  parseJoinClauses,
  parseSelectList,
  parseUpdateTarget,
  parseWithClause,
} from './clauseParsers';
import { TokenCursor } from './tokenCursor';
import { expandOtherTokens, meaningfulTokens, tokenKeyword } from './utils';

export class ScopeBuilder {
  readonly scopes: SqlScope[] = [];
  private counter = 0;
  readonly adapter: SqlDialectAdapter;
  readonly source: string;

  constructor(source: string, adapter: SqlDialectAdapter) {
    this.source = source;
    this.adapter = adapter;
  }

  addScope(
    range: SqlTextRange,
    kind: SqlScope['kind'],
    parentId?: string,
    ctes: SqlCteBinding[] = [],
  ): string {
    const id = `scope-${this.counter++}`;
    this.scopes.push({
      id,
      range,
      parentId,
      kind,
      ctes,
      relations: [],
      projectionAliases: [],
    });
    return id;
  }

  addSubqueryScope(range: SqlTextRange, kind: 'subquery' | 'cte', parentId?: string): string {
    return this.addScope(range, kind, parentId);
  }

  finalizeScope(
    scopeId: string,
    relations: SqlRelationBinding[],
    projectionAliases: SqlProjectionAlias[],
  ): void {
    const scope = this.scopes.find((s) => s.id === scopeId);
    if (!scope) return;
    scope.relations = relations;
    scope.projectionAliases = projectionAliases;
  }
}

function detectScopeKind(header: SqlToken | undefined): SqlScope['kind'] {
  const kw = header ? tokenKeyword(header) : null;
  if (kw === 'insert') return 'insert';
  if (kw === 'update') return 'update';
  if (kw === 'delete') return 'delete';
  return 'select';
}

function buildScopeCore(
  stmtTokens: readonly SqlToken[],
  scopeRange: SqlTextRange,
  parentId: string | undefined,
  defaultKind: SqlScope['kind'],
  builder: ScopeBuilder,
  adapter: SqlDialectAdapter,
): string {
  const scopeId = builder.addScope(scopeRange, defaultKind, parentId);
  const cursor = new TokenCursor(stmtTokens);
  const ctes = parseWithClause(cursor, adapter, builder, scopeId, (innerTokens, innerRange) =>
    buildScopeCore(innerTokens, innerRange, scopeId, 'cte', builder, adapter),
  );
  const scope = builder.scopes.find((s) => s.id === scopeId);
  if (scope) scope.ctes = ctes;

  const kind = defaultKind === 'select' ? detectScopeKind(cursor.peek()) : defaultKind;
  if (scope) scope.kind = kind;

  const cteNames = new Set(ctes.map((c) => adapter.foldUnquotedIdentifier(c.name)));
  const createSubqueryScope = (innerTokens: readonly SqlToken[], innerRange: SqlTextRange) =>
    buildScopeCore(innerTokens, innerRange, scopeId, 'subquery', builder, adapter);

  let relations: SqlRelationBinding[] = ctes.map((cte) => ({
    relation: { namespacePath: [], name: { name: cte.name, quoted: false } },
    alias: cte.name,
    aliasRange: cte.nameRange,
    sourceRange: cte.nameRange,
    sourceKind: 'cte' as const,
  }));
  let projectionAliases: SqlProjectionAlias[] = [];

  if (kind === 'insert') {
    const target = parseInsertTarget(cursor, adapter);
    if (target) relations.push(target);
  } else if (kind === 'update') {
    const target = parseUpdateTarget(cursor, adapter);
    if (target) relations.push(target);
  } else if (kind === 'delete') {
    const target = parseDeleteTarget(cursor, adapter);
    if (target) relations.push(target);
  } else {
    projectionAliases = parseSelectList(cursor, adapter);
    relations.push(...parseFromClause(cursor, adapter, cteNames, createSubqueryScope));
    relations.push(...parseJoinClauses(cursor, adapter, cteNames, createSubqueryScope));
  }

  builder.finalizeScope(scopeId, relations, projectionAliases);
  return scopeId;
}

export function buildScopesForStatement(
  tokens: readonly SqlToken[],
  stmtRange: SqlTextRange,
  adapter: SqlDialectAdapter,
  source: string,
): SqlScope[] {
  const builder = new ScopeBuilder(source, adapter);
  const stmtTokens = expandOtherTokens(
    meaningfulTokens(tokens.filter((t) => t.from >= stmtRange.from && t.to <= stmtRange.to)),
  );
  buildScopeCore(stmtTokens, stmtRange, undefined, 'select', builder, adapter);
  if (builder.scopes.length > 0 && builder.scopes[0]) {
    builder.scopes[0].range = {
      from: Math.min(builder.scopes[0].range.from, stmtRange.from),
      to: Math.max(builder.scopes[0].range.to, stmtRange.to),
    };
  }
  return builder.scopes;
}
