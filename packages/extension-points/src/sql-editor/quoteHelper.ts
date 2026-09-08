import { SqlTokenKind } from './tokens';
import type {
  SqlDialectAdapter,
  SqlIdentifierSegment,
  QualifiedRelationId,
  SqlToken,
} from './types';

const IDENT_START = /^[A-Za-z_]/;
const IDENT_PART = /^[A-Za-z0-9_$]*$/;

export function unquoteDouble(text: string): string | null {
  if (text.length < 2 || text[0] !== '"' || text[text.length - 1] !== '"') return null;
  return text.slice(1, -1).replace(/""/g, '"');
}

export function unquoteBacktick(text: string): string | null {
  if (text.length < 2 || text[0] !== '`' || text[text.length - 1] !== '`') return null;
  return text.slice(1, -1);
}

export function unquoteBracket(text: string): string | null {
  if (text.length < 2 || text[0] !== '[' || text[text.length - 1] !== ']') return null;
  return text.slice(1, -1).replace(/]]/g, ']');
}

export function segmentFromQuotedText(text: string): SqlIdentifierSegment | null {
  const d = unquoteDouble(text);
  if (d !== null) return { name: d, quoted: true };
  const b = unquoteBacktick(text);
  if (b !== null) return { name: b, quoted: true };
  const br = unquoteBracket(text);
  if (br !== null) return { name: br, quoted: true };
  return null;
}

export function segmentFromToken(token: SqlToken): SqlIdentifierSegment | null {
  switch (token.kind) {
    case SqlTokenKind.DoubleQuoted: {
      const name = unquoteDouble(token.text);
      return name !== null ? { name, quoted: true } : null;
    }
    case SqlTokenKind.BacktickQuoted: {
      const name = unquoteBacktick(token.text);
      return name !== null ? { name, quoted: true } : null;
    }
    case SqlTokenKind.BracketQuoted: {
      const name = unquoteBracket(token.text);
      return name !== null ? { name, quoted: true } : null;
    }
    case SqlTokenKind.Other: {
      const text = token.text;
      if (!text || !IDENT_START.test(text) || !IDENT_PART.test(text)) return null;
      return { name: text, quoted: false };
    }
    default:
      return null;
  }
}

/** Parse dot-separated qualified name from raw text (handles quoted segments). */
export function parseQualifiedNameText(text: string): QualifiedRelationId | null {
  const trimmed = text.trim();
  if (!trimmed) return null;

  const segments: SqlIdentifierSegment[] = [];
  let i = 0;
  const len = trimmed.length;

  while (i < len) {
    const ch = trimmed[i]!;
    if (ch === '.') {
      i += 1;
      continue;
    }
    if (ch === '"') {
      let j = i + 1;
      while (j < len) {
        if (trimmed[j] === '"') {
          if (j + 1 < len && trimmed[j + 1] === '"') {
            j += 2;
            continue;
          }
          j += 1;
          break;
        }
        j += 1;
      }
      const part = unquoteDouble(trimmed.slice(i, j));
      if (part === null) return null;
      segments.push({ name: part, quoted: true });
      i = j;
      continue;
    }
    if (ch === '`') {
      let j = i + 1;
      while (j < len && trimmed[j] !== '`') j += 1;
      if (j >= len) return null;
      j += 1;
      const part = unquoteBacktick(trimmed.slice(i, j));
      if (part === null) return null;
      segments.push({ name: part, quoted: true });
      i = j;
      continue;
    }
    if (ch === '[') {
      let j = i + 1;
      while (j < len) {
        if (trimmed[j] === ']') {
          if (j + 1 < len && trimmed[j + 1] === ']') {
            j += 2;
            continue;
          }
          j += 1;
          break;
        }
        j += 1;
      }
      const part = unquoteBracket(trimmed.slice(i, j));
      if (part === null) return null;
      segments.push({ name: part, quoted: true });
      i = j;
      continue;
    }
    if (!IDENT_START.test(ch)) return null;
    let j = i + 1;
    while (j < len && /[A-Za-z0-9_$]/.test(trimmed[j]!)) j += 1;
    segments.push({ name: trimmed.slice(i, j), quoted: false });
    i = j;
  }

  if (segments.length === 0) return null;
  const name = segments[segments.length - 1]!;
  return { namespacePath: segments.slice(0, -1), name };
}

export function qualifiedNameToString(id: QualifiedRelationId): string {
  const parts = [...id.namespacePath.map((s) => s.name), id.name.name];
  return parts.join('.');
}

export function foldSegment(segment: SqlIdentifierSegment, adapter: SqlDialectAdapter): string {
  return segment.quoted ? segment.name : adapter.foldUnquotedIdentifier(segment.name);
}

export function relationKey(id: QualifiedRelationId, adapter: SqlDialectAdapter): string {
  const parts = [...id.namespacePath, id.name].map((s) => foldSegment(s, adapter));
  return parts.join('.');
}
