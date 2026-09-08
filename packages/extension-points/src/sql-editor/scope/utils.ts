import { isCommentKind, SqlTokenKind } from '../tokens';
import type { SqlTextRange, SqlToken } from '../types';

export const JOIN_MODIFIERS = new Set([
  'inner',
  'left',
  'right',
  'full',
  'cross',
  'natural',
  'outer',
  'straight',
]);

export const JOIN_KEYWORDS = new Set(['join', 'apply', 'lateral']);

/** Keywords that terminate SELECT list items and must not be treated as implicit aliases. */
export const ALIAS_STOP_KEYWORDS = [
  'as',
  'from',
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
  'into',
  'join',
  'on',
  'using',
  'set',
  'values',
  'inner',
  'left',
  'right',
  'full',
  'cross',
  'natural',
  'outer',
  'straight',
  'lateral',
  'apply',
] as const;

export function rangeOf(token: SqlToken): SqlTextRange {
  return { from: token.from, to: token.to };
}

export function mergeRange(a: SqlTextRange, b: SqlTextRange): SqlTextRange {
  return { from: Math.min(a.from, b.from), to: Math.max(a.to, b.to) };
}

function isWhitespaceToken(token: SqlToken): boolean {
  return token.kind === SqlTokenKind.Whitespace;
}

export function meaningfulTokens(tokens: readonly SqlToken[]): SqlToken[] {
  return tokens.filter((t) => !isWhitespaceToken(t) && !isCommentKind(t.kind));
}

export function tokenKeyword(token: SqlToken | undefined): string | null {
  if (!token || token.kind !== SqlTokenKind.Other) return null;
  return token.text.toLowerCase();
}

export function isKeyword(token: SqlToken | undefined, keyword: string): boolean {
  return tokenKeyword(token) === keyword;
}

export function isAnyKeyword(token: SqlToken | undefined, keywords: readonly string[]): boolean {
  const kw = tokenKeyword(token);
  return kw !== null && keywords.includes(kw);
}

function isIdentChar(code: number): boolean {
  return (
    (code >= 48 && code <= 57) || // 0-9
    (code >= 65 && code <= 90) || // A-Z
    (code >= 97 && code <= 122) || // a-z
    code === 95 || // _
    code === 36 // $
  );
}

/**
 * The S2-A scanner glues dots/commas/operators into single `Other` tokens
 * (e.g. `public.users`, `cnt,`, `a.id`). The scope parsers need them as
 * separate tokens, so we expand each `Other` run into word/punctuation parts,
 * preserving offsets and paren depth. This is the single normalization used by
 * the scope model — it never re-reads source or re-scans, so statement
 * splitting stays correct.
 */
export function expandOtherTokens(tokens: readonly SqlToken[]): SqlToken[] {
  const out: SqlToken[] = [];
  for (const token of tokens) {
    if (token.kind !== SqlTokenKind.Other) {
      out.push(token);
      continue;
    }
    const text = token.text;
    let i = 0;
    while (i < text.length) {
      const isIdent = isIdentChar(text.charCodeAt(i));
      const start = i;
      while (i < text.length && isIdentChar(text.charCodeAt(i)) === isIdent) {
        i += 1;
      }
      out.push({
        kind: SqlTokenKind.Other,
        from: token.from + start,
        to: token.from + i,
        text: text.slice(start, i),
        parenDepth: token.parenDepth,
      });
    }
  }
  return out;
}
