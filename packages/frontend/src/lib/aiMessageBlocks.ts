/**
 * Parse AI chat message content into alternating text and fenced-code segments.
 * Supports standard markdown fences: ```lang\n...\n```
 */

export interface CodeBlockSegment {
  type: 'code';
  language: string;
  code: string;
}

export interface TextSegment {
  type: 'text';
  content: string;
}

export type MessageSegment = TextSegment | CodeBlockSegment;

const FENCE_RE = /```([\w-]*)\s*\n([\s\S]*?)```/g;

/** Split message content into text and code segments, preserving order. */
export function parseMessageSegments(text: string): MessageSegment[] {
  if (!text) return [];

  const segments: MessageSegment[] = [];
  let lastIndex = 0;
  let match: RegExpExecArray | null;

  FENCE_RE.lastIndex = 0;
  while ((match = FENCE_RE.exec(text)) !== null) {
    const before = text.slice(lastIndex, match.index);
    if (before) {
      segments.push({ type: 'text', content: before });
    }
    segments.push({
      type: 'code',
      language: match[1].toLowerCase(),
      code: match[2].trimEnd(),
    });
    lastIndex = match.index + match[0].length;
  }

  const tail = text.slice(lastIndex);
  if (tail) {
    segments.push({ type: 'text', content: tail });
  }

  return segments;
}

/** Whether a fenced code block should offer "Insert to Editor". */
export function isSqlCodeBlock(language: string, code: string): boolean {
  const sqlLangs = new Set(['sql', 'mysql', 'postgresql', 'postgres', 'sqlite', 'mariadb']);
  if (sqlLangs.has(language.toLowerCase())) return true;
  return looksLikeSql(code);
}

const SQL_KEYWORDS_RE =
  /^\s*(SELECT|INSERT|UPDATE|DELETE|WITH|CREATE|ALTER|DROP|EXPLAIN|SHOW|DESCRIBE|PRAGMA|TRUNCATE|GRANT|REVOKE|BEGIN|COMMIT|ROLLBACK|SET|USE|VACUUM)\b/i;

function looksLikeSql(text: string): boolean {
  const firstLine = text.split('\n')[0].trim();
  return SQL_KEYWORDS_RE.test(firstLine);
}
