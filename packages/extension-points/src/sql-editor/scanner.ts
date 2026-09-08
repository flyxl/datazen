import { isQuoteOrCommentKind, SqlTokenKind } from './tokens';
import type { ScanResult, SqlToken } from './types';

export type ScanSqlOptions = {
  /** Emit identifier/operator runs (default true). Disable for boundary-only scans. */
  includeOther?: boolean;
  /** Attach slice text to each token (default true). Disable to avoid allocations. */
  includeText?: boolean;
};

function isWhitespace(code: number): boolean {
  return code === 32 || code === 9 || code === 10 || code === 13 || code === 12;
}

function isIdentChar(code: number): boolean {
  return (
    (code >= 65 && code <= 90) ||
    (code >= 97 && code <= 122) ||
    (code >= 48 && code <= 57) ||
    code === 95
  );
}

function matchDollarTag(source: string, start: number): string | null {
  if (source.charCodeAt(start) !== 36) return null;
  let end = start + 1;
  const len = source.length;
  while (end < len && isIdentChar(source.charCodeAt(end))) {
    end += 1;
  }
  if (end >= len || source.charCodeAt(end) !== 36) return null;
  return source.slice(start, end + 1);
}

function pushToken(
  tokens: SqlToken[],
  kind: SqlTokenKind,
  from: number,
  to: number,
  source: string,
  parenDepth: number,
  includeText: boolean,
): void {
  if (from >= to) return;
  tokens.push({
    kind,
    from,
    to,
    text: includeText ? source.slice(from, to) : '',
    parenDepth,
  });
}

function isOtherBreak(code: number, next: number): boolean {
  return (
    (code === 45 && next === 45) ||
    code === 35 ||
    (code === 47 && next === 42) ||
    code === 39 ||
    code === 34 ||
    code === 96 ||
    code === 91 ||
    code === 59 ||
    code === 40 ||
    code === 41 ||
    code === 44 ||
    isWhitespace(code)
  );
}

function scanOtherRun(
  source: string,
  start: number,
  parenDepth: number,
  includeText: boolean,
): { end: number; tokens: SqlToken[] } {
  const tokens: SqlToken[] = [];
  let i = start;
  const len = source.length;
  while (i < len) {
    const code = source.charCodeAt(i);
    const next = i + 1 < len ? source.charCodeAt(i + 1) : 0;
    if (isOtherBreak(code, next)) break;
    i += 1;
  }
  pushToken(tokens, SqlTokenKind.Other, start, i, source, parenDepth, includeText);
  return { end: i, tokens };
}

function skipOtherRun(source: string, start: number): number {
  let i = start;
  const len = source.length;
  while (i < len) {
    const code = source.charCodeAt(i);
    const next = i + 1 < len ? source.charCodeAt(i + 1) : 0;
    if (isOtherBreak(code, next)) break;
    i += 1;
  }
  return i;
}

/**
 * Unified lexical scanner for SQL statement boundary detection.
 * Supports ', ", `, [], --, #, block comments, PG dollar quotes, and paren depth.
 */
export function scanSql(source: string, options: ScanSqlOptions = {}): ScanResult {
  const includeOther = options.includeOther ?? true;
  const includeText = options.includeText ?? true;
  const tokens: SqlToken[] = [];
  let parenDepth = 0;
  let i = 0;
  const len = source.length;

  while (i < len) {
    const code = source.charCodeAt(i);
    const next = i + 1 < len ? source.charCodeAt(i + 1) : 0;

    if (isWhitespace(code)) {
      do {
        i += 1;
      } while (i < len && isWhitespace(source.charCodeAt(i)));
      continue;
    }

    if (code === 45 && next === 45) {
      const start = i;
      i += 2;
      while (i < len && source.charCodeAt(i) !== 10) {
        i += 1;
      }
      pushToken(tokens, SqlTokenKind.LineComment, start, i, source, parenDepth, includeText);
      continue;
    }

    if (code === 35) {
      const start = i;
      i += 1;
      while (i < len && source.charCodeAt(i) !== 10) {
        i += 1;
      }
      pushToken(tokens, SqlTokenKind.LineComment, start, i, source, parenDepth, includeText);
      continue;
    }

    if (code === 47 && next === 42) {
      const start = i;
      i += 2;
      while (i + 1 < len && !(source.charCodeAt(i) === 42 && source.charCodeAt(i + 1) === 47)) {
        i += 1;
      }
      if (i + 1 < len) {
        i += 2;
      }
      pushToken(tokens, SqlTokenKind.BlockComment, start, i, source, parenDepth, includeText);
      continue;
    }

    if (code === 39) {
      const start = i;
      i += 1;
      while (i < len) {
        const c = source.charCodeAt(i);
        if (c === 39) {
          if (i + 1 < len && source.charCodeAt(i + 1) === 39) {
            i += 2;
            continue;
          }
          i += 1;
          break;
        }
        i += 1;
      }
      pushToken(tokens, SqlTokenKind.SingleQuoted, start, i, source, parenDepth, includeText);
      continue;
    }

    if (code === 34) {
      const start = i;
      i += 1;
      while (i < len) {
        const c = source.charCodeAt(i);
        if (c === 34) {
          if (i + 1 < len && source.charCodeAt(i + 1) === 34) {
            i += 2;
            continue;
          }
          i += 1;
          break;
        }
        i += 1;
      }
      pushToken(tokens, SqlTokenKind.DoubleQuoted, start, i, source, parenDepth, includeText);
      continue;
    }

    if (code === 96) {
      const start = i;
      i += 1;
      while (i < len) {
        if (source.charCodeAt(i) === 96) {
          i += 1;
          break;
        }
        i += 1;
      }
      pushToken(tokens, SqlTokenKind.BacktickQuoted, start, i, source, parenDepth, includeText);
      continue;
    }

    if (code === 91) {
      const start = i;
      i += 1;
      while (i < len) {
        const c = source.charCodeAt(i);
        if (c === 93) {
          if (i + 1 < len && source.charCodeAt(i + 1) === 93) {
            i += 2;
            continue;
          }
          i += 1;
          break;
        }
        i += 1;
      }
      pushToken(tokens, SqlTokenKind.BracketQuoted, start, i, source, parenDepth, includeText);
      continue;
    }

    if (code === 36) {
      const tag = matchDollarTag(source, i);
      if (tag) {
        const start = i;
        let j = i + tag.length;
        let closed = false;
        while (j < len) {
          if (source.charCodeAt(j) === 36) {
            const maybeClose = matchDollarTag(source, j);
            if (maybeClose === tag) {
              j += tag.length;
              closed = true;
              break;
            }
          }
          j += 1;
        }
        if (closed) {
          pushToken(tokens, SqlTokenKind.DollarQuoted, start, j, source, parenDepth, includeText);
          i = j;
          continue;
        }
      }
    }

    if (code === 59) {
      pushToken(tokens, SqlTokenKind.Semicolon, i, i + 1, source, parenDepth, includeText);
      i += 1;
      continue;
    }

    if (code === 40) {
      pushToken(tokens, SqlTokenKind.OpenParen, i, i + 1, source, parenDepth, includeText);
      parenDepth += 1;
      i += 1;
      continue;
    }

    if (code === 41) {
      pushToken(tokens, SqlTokenKind.CloseParen, i, i + 1, source, parenDepth, includeText);
      parenDepth = Math.max(0, parenDepth - 1);
      i += 1;
      continue;
    }

    if (code === 44) {
      pushToken(tokens, SqlTokenKind.Other, i, i + 1, source, parenDepth, includeText);
      i += 1;
      continue;
    }

    if (includeOther) {
      const { end, tokens: otherTokens } = scanOtherRun(source, i, parenDepth, includeText);
      tokens.push(...otherTokens);
      i = end;
    } else {
      i = skipOtherRun(source, i);
    }
  }

  return { tokens, finalParenDepth: parenDepth };
}

/** Replace semicolons inside comments and quoted literals with spaces. */
export function maskSemicolonsInLiterals(source: string): string {
  const { tokens } = scanSql(source, { includeOther: false, includeText: false });
  if (tokens.length === 0) return source;

  let result = '';
  let pos = 0;
  for (const token of tokens) {
    if (token.kind === SqlTokenKind.Semicolon) continue;
    if (!isQuoteOrCommentKind(token.kind)) continue;

    result += source.slice(pos, token.from);
    for (let j = token.from; j < token.to; j += 1) {
      result += source.charCodeAt(j) === 59 ? ' ' : source[j];
    }
    pos = token.to;
  }
  if (pos === 0) return source;
  result += source.slice(pos);
  return result;
}

/** Split SQL into raw statement slices using masked semicolon delimiters. */
export function splitSqlBySemicolon(source: string): string[] {
  const masked = maskSemicolonsInLiterals(source);
  const parts: string[] = [];
  let start = 0;
  for (let i = 0; i < masked.length; i += 1) {
    if (masked.charCodeAt(i) === 59) {
      parts.push(source.slice(start, i + 1));
      start = i + 1;
    }
  }
  if (start < source.length) {
    parts.push(source.slice(start));
  }
  return parts;
}
