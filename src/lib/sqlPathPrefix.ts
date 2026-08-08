const IDENT = /[A-Za-z0-9_$]/;

function isIdentChar(ch: string): boolean {
  return IDENT.test(ch);
}

export function isInsideQuotes(text: string, pos: number): boolean {
  let inSingle = false;
  let inDouble = false;
  const end = Math.max(0, Math.min(pos, text.length));
  for (let i = 0; i < end; i++) {
    const ch = text[i]!;
    if (inSingle) {
      if (ch === "'") {
        if (i + 1 < text.length && text[i + 1] === "'") {
          i++;
        } else {
          inSingle = false;
        }
      }
    } else if (inDouble) {
      if (ch === '"') {
        if (i + 1 < text.length && text[i + 1] === '"') {
          i++;
        } else {
          inDouble = false;
        }
      }
    } else if (ch === "'") {
      inSingle = true;
    } else if (ch === '"') {
      inDouble = true;
    }
  }
  return inSingle || inDouble;
}

export function extractQualifiedToken(
  text: string,
  pos: number,
): { value: string; endsWithDot: boolean } | null {
  if (isInsideQuotes(text, pos)) return null;

  const len = text.length;
  let cursor = Math.max(0, Math.min(pos, len));

  if (cursor < len && !isIdentChar(text[cursor]!) && text[cursor] !== '.') {
    if (cursor > 0 && (isIdentChar(text[cursor - 1]!) || text[cursor - 1] === '.')) {
      cursor--;
    } else {
      return null;
    }
  } else if (cursor === len) {
    if (cursor === 0 || (!isIdentChar(text[cursor - 1]!) && text[cursor - 1] !== '.')) {
      return null;
    }
  }

  let start = cursor;
  while (start > 0 && isIdentChar(text[start - 1]!)) start--;

  let i = start;
  while (i > 0) {
    if (text[i - 1] === '.') {
      let k = i - 1;
      while (k > 0 && isIdentChar(text[k - 1]!)) k--;
      if (k === i - 1) break;
      i = k;
      continue;
    }
    break;
  }
  start = i;

  let end = start;
  while (end < len) {
    const identStart = end;
    while (end < len && isIdentChar(text[end]!)) end++;
    if (end === identStart) break;
    if (end < len && text[end] === '.') {
      end++;
      continue;
    }
    break;
  }

  const value = text.slice(start, end);
  if (!value || !isIdentChar(value[0]!)) return null;
  return { value, endsWithDot: value.endsWith('.') };
}

export function parseQualifiedPathParents(text: string, cursor: number): string[] {
  const pos = Math.max(0, Math.min(cursor, text.length));
  if (isInsideQuotes(text, pos)) return [];

  const token = extractQualifiedToken(text, pos);
  if (!token) return [];
  const segs = token.value.split('.').filter(Boolean);
  if (token.endsWithDot) return segs;
  if (segs.length <= 1) return [];
  return segs.slice(0, -1);
}
