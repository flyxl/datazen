import { SqlTokenKind } from '../tokens';
import type { SqlCursorIntent, SqlScope, SqlTextRange, SqlToken } from '../types';
import { JOIN_MODIFIERS, JOIN_KEYWORDS, meaningfulTokens, tokenKeyword } from './utils';
import { isTableContext, extractQualifierParts } from '../completionContext';

/** All SQL keywords that cursor-intent detection cares about. */
const INTENT_KEYWORDS = new Set([
  'select',
  'from',
  'where',
  'having',
  'on',
  'set',
  'by',
  'into',
  'update',
  'table',
  'join',
  'apply',
  'lateral',
  'values',
  'insert',
  'and',
  'or',
  ...JOIN_MODIFIERS,
]);

function findLastIndex<T>(arr: readonly T[], predicate: (item: T) => boolean): number {
  for (let i = arr.length - 1; i >= 0; i--) {
    if (predicate(arr[i]!)) return i;
  }
  return -1;
}

/**
 * Walk backwards through meaningful tokens to find the last SQL keyword.
 *
 * The original implementation used `.find((t) => t.kind === Other)` which
 * stopped at the first identifier (e.g. `ord` in `SELECT ord`) instead of
 * the keyword (`select`). This caused the intent to be `unknown` and
 * completions to fall through to relation/table-name completions.
 */
function findLastIntentKeyword(meaningful: readonly SqlToken[]): string {
  for (let i = meaningful.length - 1; i >= 0; i--) {
    const t = meaningful[i];
    if (t.kind !== SqlTokenKind.Other) continue;
    const lower = t.text.toLowerCase();
    if (INTENT_KEYWORDS.has(lower)) return lower;
  }
  return '';
}

export function detectCursorIntent(
  source: string,
  cursor: number,
  tokens: readonly SqlToken[],
  scopes: readonly SqlScope[],
  stmtRange: SqlTextRange,
  findScope: (scopes: readonly SqlScope[], cursor: number) => SqlScope | undefined,
): SqlCursorIntent {
  const prefixStart = Math.max(stmtRange.from, cursor - 128);
  const prefix = source.slice(prefixStart, cursor);
  const replacementRange: SqlTextRange = { from: cursor, to: cursor };
  const qualifierParts = extractQualifierParts(prefix);

  const scope = findScope(scopes, cursor);
  const meaningful = meaningfulTokens(
    tokens.filter((t) => t.from >= stmtRange.from && t.from < cursor && t.to <= stmtRange.to),
  );
  const lastKw = findLastIntentKeyword(meaningful);

  if (/\(\s*$/.test(prefix) && /[\w"]$/.test(prefix.replace(/\(\s*$/, ''))) {
    return { kind: 'function_call', prefix, replacementRange, qualifierParts, scopeId: scope?.id };
  }
  if (qualifierParts.length > 0) {
    return {
      kind: 'qualified_column',
      prefix,
      replacementRange,
      qualifierParts,
      scopeId: scope?.id,
    };
  }
  if (lastKw === 'values' || (lastKw === 'insert' && /\bvalues\s*$/i.test(prefix))) {
    return { kind: 'insert_values', prefix, replacementRange, qualifierParts, scopeId: scope?.id };
  }
  if (lastKw === 'into' && /\(\s*[^)]*$/.test(prefix)) {
    return { kind: 'insert_columns', prefix, replacementRange, qualifierParts, scopeId: scope?.id };
  }
  if (lastKw === 'join' || JOIN_MODIFIERS.has(lastKw)) {
    const kwTokenIdx = findLastIndex(
      meaningful,
      (t) =>
        t.kind === SqlTokenKind.Other &&
        (t.text.toLowerCase() === 'join' || JOIN_MODIFIERS.has(t.text.toLowerCase())),
    );
    if (kwTokenIdx !== -1) {
      const tail = source.slice(meaningful[kwTokenIdx]!.to, cursor);
      if (!isTableContext(tail)) {
        return { kind: 'unknown', prefix, replacementRange, qualifierParts, scopeId: scope?.id };
      }
    }
    return { kind: 'join_target', prefix, replacementRange, qualifierParts, scopeId: scope?.id };
  }
  if (lastKw === 'from' || lastKw === 'into' || lastKw === 'update' || lastKw === 'table') {
    const kwTokenIdx = findLastIndex(
      meaningful,
      (t) => t.kind === SqlTokenKind.Other && t.text.toLowerCase() === lastKw,
    );
    if (kwTokenIdx !== -1) {
      const tail = source.slice(meaningful[kwTokenIdx]!.to, cursor);
      if (!isTableContext(tail)) {
        return { kind: 'unknown', prefix, replacementRange, qualifierParts, scopeId: scope?.id };
      }
    }
    return { kind: 'relation', prefix, replacementRange, qualifierParts, scopeId: scope?.id };
  }
  if (
    lastKw === 'select' ||
    lastKw === 'where' ||
    lastKw === 'having' ||
    lastKw === 'on' ||
    lastKw === 'set' ||
    lastKw === 'by' ||
    lastKw === 'and' ||
    lastKw === 'or'
  ) {
    return { kind: 'projection', prefix, replacementRange, qualifierParts, scopeId: scope?.id };
  }

  return { kind: 'unknown', prefix, replacementRange, qualifierParts, scopeId: scope?.id };
}

/** Exported for tests that assert keyword classification helpers. */
export function isJoinModifierKeyword(token: SqlToken | undefined): boolean {
  const kw = tokenKeyword(token);
  return kw !== null && JOIN_MODIFIERS.has(kw);
}

export function isJoinKeyword(token: SqlToken | undefined): boolean {
  const kw = tokenKeyword(token);
  return kw !== null && JOIN_KEYWORDS.has(kw);
}
