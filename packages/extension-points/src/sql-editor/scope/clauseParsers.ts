import { foldSegment, segmentFromToken } from '../quoteHelper';
import { SqlTokenKind } from '../tokens';
import type {
  QualifiedRelationId,
  SqlCteBinding,
  SqlDialectAdapter,
  SqlProjectionAlias,
  SqlRelationBinding,
  SqlRelationSourceKind,
  SqlTextRange,
  SqlToken,
} from '../types';
import type { ScopeBuilder } from './builder';
import { TokenCursor } from './tokenCursor';
import {
  ALIAS_STOP_KEYWORDS,
  isAnyKeyword,
  isKeyword,
  JOIN_KEYWORDS,
  JOIN_MODIFIERS,
  mergeRange,
  rangeOf,
  tokenKeyword,
} from './utils';

function parseQualifiedFromTokens(
  cursor: TokenCursor,
  adapter: SqlDialectAdapter,
): { qualified: QualifiedRelationId; span: SqlTextRange } | null {
  const start = cursor.peek();
  if (!start) return null;
  const parts: string[] = [];
  let last: SqlToken | null = null;

  while (!cursor.atEnd()) {
    const token = cursor.peek();
    if (!token) break;
    const segment = segmentFromToken(token);
    if (!segment) break;
    parts.push(segment.quoted ? token.text : segment.name);
    last = token;
    cursor.advance();

    if (cursor.atEnd() || !isKeyword(cursor.peek(), '.')) break;
    cursor.advance();
  }

  if (last === null) return null;
  const text = parts.join('.');
  const qualified = adapter.parseQualifiedName(text);
  if (!qualified) return null;
  return { qualified, span: mergeRange(rangeOf(start), rangeOf(last!)) };
}

function bindingFromQualified(
  qualified: QualifiedRelationId,
  sourceRange: SqlTextRange,
  sourceKind: SqlRelationSourceKind,
  alias?: string,
  aliasRange?: SqlTextRange,
): SqlRelationBinding {
  return { relation: qualified, sourceRange, sourceKind, alias, aliasRange };
}

export function parseOptionalAlias(
  cursor: TokenCursor,
  adapter: SqlDialectAdapter,
): { alias?: string; aliasRange?: SqlTextRange } {
  if (cursor.matchKeyword('as')) {
    const token = cursor.peek();
    const segment = token ? segmentFromToken(token) : null;
    if (segment) {
      const aliasRange = rangeOf(token!);
      cursor.advance();
      return { alias: adapter.foldUnquotedIdentifier(segment.name), aliasRange };
    }
    return {};
  }
  const token = cursor.peek();
  const segment = token ? segmentFromToken(token) : null;
  if (segment && !isAnyKeyword(token, ALIAS_STOP_KEYWORDS)) {
    cursor.advance();
    return { alias: adapter.foldUnquotedIdentifier(segment.name), aliasRange: rangeOf(token!) };
  }
  return {};
}

function skipQueryUntilCloseParen(cursor: TokenCursor): void {
  let depth = 1;
  while (!cursor.atEnd() && depth > 0) {
    const token = cursor.advance();
    if (!token) break;
    if (token.kind === SqlTokenKind.OpenParen) depth += 1;
    else if (token.kind === SqlTokenKind.CloseParen) depth -= 1;
  }
}

function sourceKindForName(
  qualified: QualifiedRelationId,
  cteNames: ReadonlySet<string>,
  adapter: SqlDialectAdapter,
): SqlRelationSourceKind {
  return cteNames.has(foldSegment(qualified.name, adapter)) ? 'cte' : 'table';
}

function parseRelationEntry(
  cursor: TokenCursor,
  adapter: SqlDialectAdapter,
  cteNames: ReadonlySet<string>,
  createSubqueryScope: (innerTokens: readonly SqlToken[], innerRange: SqlTextRange) => string,
): SqlRelationBinding | null {
  const startToken = cursor.peek();
  if (!startToken) return null;

  if (startToken.kind === SqlTokenKind.OpenParen) {
    const openRange = rangeOf(startToken);
    cursor.advance();
    const innerStart = cursor.pos;
    skipQueryUntilCloseParen(cursor);
    const wasClosed = cursor.tokens[cursor.pos - 1]?.kind === SqlTokenKind.CloseParen;
    const endToken = cursor.tokens[cursor.pos - 1] ?? startToken;
    const sourceRange = mergeRange(openRange, rangeOf(endToken));
    const innerTokens = cursor.tokens.slice(innerStart, cursor.pos - (wasClosed ? 1 : 0));
    const subScopeId = createSubqueryScope(innerTokens, sourceRange);
    const aliasInfo = parseOptionalAlias(cursor, adapter);
    return bindingFromQualified(
      { namespacePath: [], name: { name: subScopeId, quoted: false } },
      sourceRange,
      'subquery',
      aliasInfo.alias,
      aliasInfo.aliasRange,
    );
  }

  const parsed = parseQualifiedFromTokens(cursor, adapter);
  if (!parsed) return null;
  const aliasInfo = parseOptionalAlias(cursor, adapter);
  return bindingFromQualified(
    parsed.qualified,
    parsed.span,
    sourceKindForName(parsed.qualified, cteNames, adapter),
    aliasInfo.alias,
    aliasInfo.aliasRange,
  );
}

function skipUntilJoinBoundary(cursor: TokenCursor): void {
  let parenDepth = 0;
  while (!cursor.atEnd()) {
    const token = cursor.peek();
    if (!token) break;
    if (token.kind === SqlTokenKind.OpenParen) parenDepth += 1;
    else if (token.kind === SqlTokenKind.CloseParen) {
      parenDepth -= 1;
      if (parenDepth < 0) break;
    } else if (parenDepth === 0) {
      const kw = tokenKeyword(token);
      if (
        kw &&
        (JOIN_KEYWORDS.has(kw) ||
          JOIN_MODIFIERS.has(kw) ||
          ['where', 'group', 'order', 'having', 'limit', 'offset', 'union'].includes(kw))
      ) {
        break;
      }
    }
    cursor.advance();
  }
}

export function parseFromClause(
  cursor: TokenCursor,
  adapter: SqlDialectAdapter,
  cteNames: ReadonlySet<string>,
  createSubqueryScope: (innerTokens: readonly SqlToken[], innerRange: SqlTextRange) => string,
): SqlRelationBinding[] {
  const relations: SqlRelationBinding[] = [];
  if (!cursor.matchKeyword('from')) return relations;

  while (!cursor.atEnd()) {
    const token = cursor.peek();
    if (!token) break;
    const kw = tokenKeyword(token);
    if (
      kw &&
      [
        'where',
        'group',
        'order',
        'having',
        'limit',
        'offset',
        'union',
        'intersect',
        'except',
        'fetch',
      ].includes(kw)
    ) {
      break;
    }
    if (kw === 'join' || (kw && JOIN_MODIFIERS.has(kw))) break;

    if (token.kind === SqlTokenKind.Other && token.text === ',') {
      cursor.advance();
      continue;
    }

    const binding = parseRelationEntry(cursor, adapter, cteNames, createSubqueryScope);
    if (binding) relations.push(binding);
    else break;
  }

  return relations;
}

export function parseJoinClauses(
  cursor: TokenCursor,
  adapter: SqlDialectAdapter,
  cteNames: ReadonlySet<string>,
  createSubqueryScope: (innerTokens: readonly SqlToken[], innerRange: SqlTextRange) => string,
): SqlRelationBinding[] {
  const relations: SqlRelationBinding[] = [];

  while (!cursor.atEnd()) {
    while (cursor.peek() && JOIN_MODIFIERS.has(tokenKeyword(cursor.peek()) ?? '')) {
      cursor.advance();
    }
    const token = cursor.peek();
    if (!token) break;
    const kw = tokenKeyword(token);
    if (!kw || !JOIN_KEYWORDS.has(kw)) break;
    cursor.advance();

    const binding = parseRelationEntry(cursor, adapter, cteNames, createSubqueryScope);
    if (binding) relations.push(binding);

    if (cursor.matchKeyword('on') || cursor.matchKeyword('using')) {
      skipUntilJoinBoundary(cursor);
    }
  }

  return relations;
}

export function parseSelectList(
  cursor: TokenCursor,
  adapter: SqlDialectAdapter,
): SqlProjectionAlias[] {
  const aliases: SqlProjectionAlias[] = [];
  if (!cursor.matchKeyword('select')) return aliases;

  while (!cursor.atEnd()) {
    const first = cursor.peek();
    if (!first) break;
    if (tokenKeyword(first) === 'from') break;

    const exprStart = first.from;
    const item: SqlToken[] = [];
    let depth = 0;
    let hitFrom = false;

    while (!cursor.atEnd()) {
      const t = cursor.peek()!;
      const tkw = tokenKeyword(t);
      if (depth === 0 && tkw === 'from') {
        hitFrom = true;
        break;
      }
      if (depth === 0 && t.text === ',') {
        cursor.advance();
        break;
      }
      item.push(t);
      if (t.kind === SqlTokenKind.OpenParen) depth += 1;
      else if (t.kind === SqlTokenKind.CloseParen) depth -= 1;
      cursor.advance();
    }

    if (item.length === 0) {
      if (hitFrom) break;
      // dangling comma or punctuation; avoid a spin
      if (cursor.atEnd()) break;
      cursor.advance();
      continue;
    }

    const aliasInfo = detectProjectionAlias(item, adapter);
    if (aliasInfo) {
      aliases.push({
        alias: aliasInfo.alias,
        aliasRange: aliasInfo.aliasRange,
        expressionRange: { from: exprStart, to: aliasInfo.exprTo },
      });
    }
  }

  return aliases;
}

function detectProjectionAlias(
  item: readonly SqlToken[],
  adapter: SqlDialectAdapter,
): { alias: string; aliasRange: SqlTextRange; exprTo: number } | null {
  if (item.length === 0) return null;
  const last = item[item.length - 1]!;

  // explicit `expr AS alias`
  if (item.length >= 2 && tokenKeyword(item[item.length - 2]) === 'as') {
    const seg = segmentFromToken(last);
    if (seg) {
      return {
        alias: adapter.foldUnquotedIdentifier(seg.name),
        aliasRange: rangeOf(last),
        exprTo: item[item.length - 2]!.from,
      };
    }
  }

  // implicit trailing alias: `<expr> <bare-ident>` where the ident is not the
  // end of a qualified name and not a stop keyword. A lone item is not an alias.
  const seg = segmentFromToken(last);
  if (seg && item.length >= 2 && !isAnyKeyword(last, ALIAS_STOP_KEYWORDS)) {
    const prev = item[item.length - 2]!;
    if (!isKeyword(prev, '.')) {
      return {
        alias: adapter.foldUnquotedIdentifier(seg.name),
        aliasRange: rangeOf(last),
        exprTo: last.from,
      };
    }
  }

  return null;
}

function parseCteColumnList(cursor: TokenCursor, adapter: SqlDialectAdapter): string[] | undefined {
  if (cursor.peek()?.kind !== SqlTokenKind.OpenParen) return undefined;
  cursor.advance();
  const names: string[] = [];
  while (!cursor.atEnd()) {
    const token = cursor.peek()!;
    if (token.kind === SqlTokenKind.CloseParen) {
      cursor.advance();
      break;
    }
    const segment = segmentFromToken(token);
    if (segment) {
      names.push(adapter.foldUnquotedIdentifier(segment.name));
      cursor.advance();
      if (cursor.peek()?.text === ',') cursor.advance();
      continue;
    }
    // unexpected token; make progress to avoid a spin
    cursor.advance();
  }
  return names.length > 0 ? names : undefined;
}

export function parseWithClause(
  cursor: TokenCursor,
  adapter: SqlDialectAdapter,
  scopeBuilder: ScopeBuilder,
  parentScopeId: string,
  createCteScope?: (innerTokens: readonly SqlToken[], innerRange: SqlTextRange) => string,
): SqlCteBinding[] {
  const ctes: SqlCteBinding[] = [];
  if (!cursor.matchKeyword('with')) return ctes;

  while (!cursor.atEnd()) {
    if (cursor.matchKeyword('recursive')) continue;
    const nameToken = cursor.peek();
    const nameSegment = nameToken ? segmentFromToken(nameToken) : null;
    if (!nameSegment) break;

    const nameRange = rangeOf(nameToken!);
    const name = adapter.foldUnquotedIdentifier(nameSegment.name);
    cursor.advance();

    const columnNames = parseCteColumnList(cursor, adapter);
    if (!cursor.matchKeyword('as')) break;
    if (cursor.peek()?.kind !== SqlTokenKind.OpenParen) break;
    cursor.advance();

    const queryStart = cursor.peek()?.from ?? nameRange.to;
    const innerStart = cursor.pos;
    skipQueryUntilCloseParen(cursor);
    const wasClosed = cursor.tokens[cursor.pos - 1]?.kind === SqlTokenKind.CloseParen;
    const queryEnd = cursor.pos > 0 ? cursor.tokens[cursor.pos - 1]!.to : queryStart;
    const innerTokens = cursor.tokens.slice(innerStart, cursor.pos - (wasClosed ? 1 : 0));
    const range = { from: queryStart, to: queryEnd };

    const subScopeId = createCteScope
      ? createCteScope(innerTokens, range)
      : scopeBuilder.addSubqueryScope(range, 'cte', parentScopeId);
    ctes.push({ name, nameRange, columnNames, queryScopeId: subScopeId });

    if (cursor.peek()?.text === ',') {
      cursor.advance();
      continue;
    }
    break;
  }

  return ctes;
}

export function parseInsertTarget(
  cursor: TokenCursor,
  adapter: SqlDialectAdapter,
): SqlRelationBinding | null {
  if (!cursor.matchKeyword('insert')) return null;
  cursor.matchKeyword('into');
  const parsed = parseQualifiedFromTokens(cursor, adapter);
  if (!parsed) return null;
  return bindingFromQualified(parsed.qualified, parsed.span, 'table');
}

export function parseUpdateTarget(
  cursor: TokenCursor,
  adapter: SqlDialectAdapter,
): SqlRelationBinding | null {
  if (!cursor.matchKeyword('update')) return null;
  const parsed = parseQualifiedFromTokens(cursor, adapter);
  if (!parsed) return null;
  const aliasInfo = parseOptionalAlias(cursor, adapter);
  return bindingFromQualified(
    parsed.qualified,
    parsed.span,
    'table',
    aliasInfo.alias,
    aliasInfo.aliasRange,
  );
}

export function parseDeleteTarget(
  cursor: TokenCursor,
  adapter: SqlDialectAdapter,
): SqlRelationBinding | null {
  if (!cursor.matchKeyword('delete')) return null;
  cursor.matchKeyword('from');
  const parsed = parseQualifiedFromTokens(cursor, adapter);
  if (!parsed) return null;
  const aliasInfo = parseOptionalAlias(cursor, adapter);
  return bindingFromQualified(
    parsed.qualified,
    parsed.span,
    'table',
    aliasInfo.alias,
    aliasInfo.aliasRange,
  );
}
