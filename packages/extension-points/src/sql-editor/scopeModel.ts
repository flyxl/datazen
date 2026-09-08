import { getDialectAdapter } from './dialectAdapter';
import { buildScopesForStatement } from './scope/builder';
import { detectCursorIntent } from './scope/cursorIntent';
import { meaningfulTokens } from './scope/utils';
import { scanSql } from './scanner';
import { SqlTokenKind } from './tokens';
import { buildStatementRanges, findStatementAtCursor } from './statementRanges';
import type {
  SqlReference,
  SqlScope,
  SqlSemanticDiagnostic,
  SqlSemanticModel,
  SqlStatementRange,
  SqlTextRange,
  SqlToken,
} from './types';

export function findScopeAtCursor(
  scopes: readonly SqlScope[],
  cursor: number,
): SqlScope | undefined {
  let best: SqlScope | undefined;
  let bestSize = Infinity;
  for (const scope of scopes) {
    if (cursor >= scope.range.from && cursor <= scope.range.to) {
      const size = scope.range.to - scope.range.from;
      if (size < bestSize) {
        best = scope;
        bestSize = size;
      }
    }
  }
  if (best) return best;

  // Fallback: If cursor is just past scope.range.to (e.g. whitespace, trailing dot),
  // pick the scope that starts before or at the cursor
  for (const scope of scopes) {
    if (cursor >= scope.range.from) {
      if (!best || scope.range.from >= best.range.from) {
        best = scope;
      }
    }
  }
  return best ?? scopes[0];
}

export function getScopeChain(scopes: readonly SqlScope[], scopeId: string): SqlScope[] {
  const byId = new Map(scopes.map((s) => [s.id, s]));
  const chain: SqlScope[] = [];
  let current = byId.get(scopeId);
  while (current) {
    chain.push(current);
    current = current.parentId ? byId.get(current.parentId) : undefined;
  }
  return chain;
}

export type BuildSemanticModelOptions = {
  dialectId?: string;
  statement?: SqlStatementRange;
};

export function buildSemanticModel(
  source: string,
  cursor: number,
  options: BuildSemanticModelOptions = {},
): SqlSemanticModel {
  const adapter = getDialectAdapter(options.dialectId ?? 'standard');
  const diagnostics: SqlSemanticDiagnostic[] = [];
  const { tokens, finalParenDepth } = scanSql(source);

  if (finalParenDepth !== 0) {
    diagnostics.push({
      message: 'Unbalanced parentheses',
      range: { from: 0, to: source.length },
      severity: 'warning',
    });
  }

  const ranges = buildStatementRanges(source);
  const statement = options.statement ??
    findStatementAtCursor(source, cursor, ranges, tokens) ??
    ranges[0] ?? {
      index: 0,
      from: 0,
      to: source.length,
      contentFrom: 0,
      contentTo: source.length,
      delimiterFrom: null,
      delimiterTo: null,
      firstExecutableLine: 0,
      confidence: 'degraded' as const,
    };

  const stmtRange: SqlTextRange = { from: statement.contentFrom, to: statement.contentTo };
  let scopes: SqlScope[] = [];
  try {
    scopes = buildScopesForStatement(tokens, stmtRange, adapter, source);
  } catch (err) {
    diagnostics.push({
      message: `Partial scope analysis due to parse error: ${err instanceof Error ? err.message : String(err)}`,
      range: stmtRange,
      severity: 'info',
    });
  }

  const cursorIntent = detectCursorIntent(
    source,
    cursor,
    tokens,
    scopes,
    stmtRange,
    findScopeAtCursor,
  );
  const references: SqlReference[] = extractReferences(tokens, scopes, stmtRange);

  return {
    statement,
    tokens,
    scopes,
    references,
    cursorIntent,
    diagnostics,
  };
}

const NON_COLUMN_KEYWORDS = new Set([
  'select',
  'from',
  'where',
  'having',
  'join',
  'inner',
  'left',
  'right',
  'full',
  'cross',
  'on',
  'as',
  'and',
  'or',
  'not',
  'in',
  'is',
  'null',
  'like',
  'ilike',
  'between',
  'group',
  'by',
  'order',
  'limit',
  'offset',
  'union',
  'all',
  'distinct',
  'insert',
  'into',
  'values',
  'update',
  'set',
  'delete',
  'create',
  'table',
  'view',
  'index',
  'drop',
  'alter',
  'case',
  'when',
  'then',
  'else',
  'end',
  'exists',
  'true',
  'false',
  'desc',
  'asc',
  'with',
  'primary',
  'key',
  'foreign',
  'references',
  'default',
  'check',
  'constraint',
  'unique',
]);

const OPERATOR_CHARS = new Set(['=', '<', '>', '!', '+', '-', '*', '/', '%', '|', '&', '^', '~']);

function isOperatorOrPunctuation(text: string): boolean {
  if (text.length === 0) return true;
  return [...text].every(
    (c) => OPERATOR_CHARS.has(c) || c === ',' || c === ';' || c === '(' || c === ')',
  );
}

function extractReferences(
  tokens: readonly SqlToken[],
  scopes: readonly SqlScope[],
  stmtRange: SqlTextRange,
): SqlReference[] {
  const references: SqlReference[] = [];
  const meaningful = meaningfulTokens(
    tokens.filter((t) => t.from >= stmtRange.from && t.to <= stmtRange.to),
  );

  const relationRanges = scopes.flatMap((s) => s.relations.map((r) => r.sourceRange));

  for (let idx = 0; idx < meaningful.length; idx++) {
    const t = meaningful[idx]!;
    if (
      t.kind !== SqlTokenKind.Other &&
      t.kind !== SqlTokenKind.DoubleQuoted &&
      t.kind !== SqlTokenKind.BacktickQuoted
    ) {
      continue;
    }

    if (relationRanges.some((r) => t.from >= r.from && t.to <= r.to)) {
      continue;
    }

    const prev = idx > 0 ? meaningful[idx - 1] : undefined;
    if (prev && prev.kind === SqlTokenKind.Other && prev.text.toUpperCase() === 'AS') {
      continue;
    }

    const text = t.text.trim();
    if (!text || isOperatorOrPunctuation(text)) continue;

    if (/^\d+(?:\.\d+)?$/.test(text)) continue;

    // If current token is just a trailing dot prefix (e.g. `o.` or `"o".`) and next token is an identifier,
    // skip it here — it will be captured as the qualifier of the next token.
    if (text.endsWith('.') && idx + 1 < meaningful.length) {
      const next = meaningful[idx + 1]!;
      if (
        next.kind === SqlTokenKind.Other ||
        next.kind === SqlTokenKind.DoubleQuoted ||
        next.kind === SqlTokenKind.BacktickQuoted
      ) {
        continue;
      }
    }

    const scope = scopes.find((s) => t.from >= s.range.from && t.to <= s.range.to) ?? scopes[0];
    const scopeId = scope?.id ?? 's0';

    // Check if preceded by a qualifier + dot: e.g. `o."order_no"` or `o . "order_no"`
    let leadingQualifier: string | undefined;
    let leadingFrom = t.from;

    if (prev) {
      if (prev.text.endsWith('.') && prev.text.length > 1) {
        // e.g. prev is `o.` or `"o".`
        leadingQualifier = prev.text
          .slice(0, -1)
          .replace(/^["`[]|["`\]]$/g, '')
          .trim();
        leadingFrom = prev.from;
      } else if (prev.text === '.' && idx > 1) {
        // e.g. prevPrev is `o` or `"o"`, prev is `.`
        const prevPrev = meaningful[idx - 2]!;
        if (
          prevPrev.kind === SqlTokenKind.Other ||
          prevPrev.kind === SqlTokenKind.DoubleQuoted ||
          prevPrev.kind === SqlTokenKind.BacktickQuoted
        ) {
          leadingQualifier = prevPrev.text.replace(/^["`[]|["`\]]$/g, '').trim();
          leadingFrom = prevPrev.from;
        }
      }
    }

    if (leadingQualifier) {
      const colName = text.replace(/^["`[]|["`\]]$/g, '').trim();
      if (colName && !NON_COLUMN_KEYWORDS.has(colName.toLowerCase())) {
        references.push({
          kind: 'column',
          text: colName,
          range: { from: leadingFrom, to: t.to },
          scopeId,
          qualifier: leadingQualifier,
          resolved: true,
        });
      }
    } else if (text.includes('.')) {
      const dotIdx = text.lastIndexOf('.');
      const qualifier = text
        .slice(0, dotIdx)
        .replace(/^["`[]|["`\]]$/g, '')
        .trim();
      const colName = text
        .slice(dotIdx + 1)
        .replace(/^["`[]|["`\]]$/g, '')
        .trim();
      if (colName && !NON_COLUMN_KEYWORDS.has(colName.toLowerCase())) {
        references.push({
          kind: 'column',
          text: colName,
          range: { from: t.from, to: t.to },
          scopeId,
          qualifier,
          resolved: true,
        });
      }
    } else {
      const lower = text.toLowerCase();
      if (!NON_COLUMN_KEYWORDS.has(lower)) {
        references.push({
          kind: 'column',
          text: text.replace(/^["`[]|["`\]]$/g, ''),
          range: { from: t.from, to: t.to },
          scopeId,
          resolved: true,
        });
      }
    }
  }

  return references;
}

// Re-export token helpers used by downstream modules/tests.
export { meaningfulTokens } from './scope/utils';
