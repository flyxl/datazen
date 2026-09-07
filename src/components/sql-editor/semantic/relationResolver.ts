import { getDialectAdapter } from './dialectAdapter';
import { foldSegment, relationKey } from './quoteHelper';
import { findScopeAtCursor, getScopeChain } from './scopeModel';
import type {
  QualifiedRelationId,
  RelationResolveResult,
  SqlCteBinding,
  SqlDialectAdapter,
  SqlRelationBinding,
  SqlScope,
  SqlSemanticModel,
} from './types';

function cteToBinding(cte: SqlCteBinding): SqlRelationBinding {
  return {
    relation: { namespacePath: [], name: { name: cte.name, quoted: false } },
    alias: cte.name,
    aliasRange: cte.nameRange,
    sourceRange: cte.nameRange,
    sourceKind: 'cte',
  };
}

/**
 * Match a binding against a user-supplied name. If the relation carries an
 * explicit/implicit alias, only the alias (the correlation name) is matchable;
 * otherwise the base relation name matches. This mirrors SQL correlation-name
 * ambiguity: an aliased relation must be referenced by its alias.
 */
function bindingMatchesName(
  binding: SqlRelationBinding,
  name: string,
  adapter: SqlDialectAdapter,
): boolean {
  if (binding.alias && adapter.compareIdentifiers(binding.alias, name)) return true;
  return adapter.compareIdentifiers(foldSegment(binding.relation.name, adapter), name);
}

function relationMatchesQualified(
  binding: SqlRelationBinding,
  targetKey: string,
  adapter: SqlDialectAdapter,
): boolean {
  return relationKey(binding.relation, adapter) === targetKey;
}

/** Resolve a table/alias name within the scope chain (inner scopes shadow outer). */
export function resolveRelation(
  name: string,
  model: SqlSemanticModel,
  cursor?: number,
  adapter?: SqlDialectAdapter,
): RelationResolveResult {
  const dialect = adapter ?? getDialectAdapter('standard');
  const pos = cursor ?? model.cursorIntent.replacementRange.from;
  const scope = findScopeAtCursor(model.scopes, pos);
  if (!scope) return { status: 'unresolved' };

  const chain = getScopeChain(model.scopes, scope.id);
  for (const chainScope of chain) {
    // A CTE name shadows any relation with the same name in this scope.
    const cteMatches = chainScope.ctes.filter((cte) => dialect.compareIdentifiers(cte.name, name));
    if (cteMatches.length === 1) return { status: 'unique', binding: cteToBinding(cteMatches[0]!) };
    if (cteMatches.length > 1)
      return { status: 'ambiguous', candidates: cteMatches.map(cteToBinding) };

    const matches = chainScope.relations.filter((binding) =>
      bindingMatchesName(binding, name, dialect),
    );
    if (matches.length === 1) return { status: 'unique', binding: matches[0]! };
    if (matches.length > 1) return { status: 'ambiguous', candidates: matches };
  }

  return { status: 'unresolved' };
}

function collectBindingsInScopeChain(
  scopes: readonly SqlScope[],
  scopeId: string,
  adapter: SqlDialectAdapter,
): SqlRelationBinding[] {
  const chain = getScopeChain(scopes, scopeId);
  const seen = new Set<string>();
  const result: SqlRelationBinding[] = [];

  for (const scope of chain) {
    for (const cte of scope.ctes) {
      const key = `cte:${adapter.foldUnquotedIdentifier(cte.name)}`;
      if (seen.has(key)) continue;
      seen.add(key);
      result.push(cteToBinding(cte));
    }
    for (const rel of scope.relations) {
      const key = `${scope.id}:${relationKey(rel.relation, adapter)}:${rel.alias ?? ''}`;
      if (seen.has(key)) continue;
      seen.add(key);
      result.push(rel);
    }
  }

  return result;
}

/** Resolve a qualified relation id against available bindings. */
export function resolveQualifiedRelation(
  qualified: QualifiedRelationId,
  model: SqlSemanticModel,
  cursor?: number,
  adapter?: SqlDialectAdapter,
): RelationResolveResult {
  const dialect = adapter ?? getDialectAdapter('standard');
  const pos = cursor ?? model.cursorIntent.replacementRange.from;
  const scope = findScopeAtCursor(model.scopes, pos);
  if (!scope) return { status: 'unresolved' };

  const targetKey = relationKey(qualified, dialect);
  const bindings = collectBindingsInScopeChain(model.scopes, scope.id, dialect);
  const matches = bindings.filter((b) => relationMatchesQualified(b, targetKey, dialect));

  if (matches.length === 1) return { status: 'unique', binding: matches[0]! };
  if (matches.length > 1) return { status: 'ambiguous', candidates: matches };
  return { status: 'unresolved' };
}

/** Resolve qualifier.column against scope bindings and projection aliases. */
export function resolveQualifiedColumn(
  qualifier: string,
  _column: string,
  model: SqlSemanticModel,
  cursor?: number,
  adapter?: SqlDialectAdapter,
): RelationResolveResult {
  return resolveRelation(qualifier, model, cursor, adapter);
}

/** List all visible relation bindings at cursor, respecting shadowing order. */
export function listVisibleRelations(
  model: SqlSemanticModel,
  cursor?: number,
  adapter?: SqlDialectAdapter,
): readonly SqlRelationBinding[] {
  const dialect = adapter ?? getDialectAdapter('standard');
  const pos = cursor ?? model.cursorIntent.replacementRange.from;
  const scope = findScopeAtCursor(model.scopes, pos);
  if (!scope) return [];

  const chain = getScopeChain(model.scopes, scope.id);
  const byAlias = new Map<string, SqlRelationBinding>();

  // Innermost scope shadows outer; first insertion wins.
  for (const chainScope of chain) {
    for (const binding of chainScope.relations) {
      const key = binding.alias ?? relationKey(binding.relation, dialect);
      const folded = dialect.foldUnquotedIdentifier(key);
      if (!byAlias.has(folded)) byAlias.set(folded, binding);
    }
    for (const cte of chainScope.ctes) {
      const folded = dialect.foldUnquotedIdentifier(cte.name);
      if (!byAlias.has(folded)) byAlias.set(folded, cteToBinding(cte));
    }
  }

  return [...byAlias.values()];
}
