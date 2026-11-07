/**
 * Extract schema objects removed by DDL executed in the query editor.
 *
 * This is deliberately a small lexical scanner rather than a full SQL parser:
 * it only needs the table/column identifiers that T14 marks stale, and it must
 * never invent a target when the statement is ambiguous.
 */
export interface SqlSchemaDropTarget {
  schema?: string;
  table?: string;
  column?: string;
}

const IDENTIFIER =
  String.raw`(?:"(?:[^"]|"")*"|` +
  '`(?:[^`]|``)*`' +
  String.raw`|\[[^\]]+\]|[A-Za-z_][A-Za-z0-9_$]*)`;
const QUALIFIED = `${IDENTIFIER}(?:\\s*\\.\\s*${IDENTIFIER}){0,2}`;

const DROP_TABLE_RE = new RegExp(String.raw`\bDROP\s+TABLE\s+(?:IF\s+EXISTS\s+)?([^;]+)`, 'gi');
const ALTER_DROP_COLUMN_RE = new RegExp(
  String.raw`\bALTER\s+TABLE\s+(?:IF\s+EXISTS\s+)?(?:ONLY\s+)?(${QUALIFIED})\s+DROP\s+(?:COLUMN\s+)?(?:IF\s+EXISTS\s+)?(${IDENTIFIER})`,
  'gi',
);
const QUALIFIED_PART_RE = new RegExp(QUALIFIED, 'g');

function unquote(identifier: string): string {
  const value = identifier.trim();
  if (value.startsWith('"') && value.endsWith('"')) {
    return value.slice(1, -1).replace(/""/g, '"');
  }
  if (value.startsWith('`') && value.endsWith('`')) {
    return value.slice(1, -1).replace(/``/g, '`');
  }
  if (value.startsWith('[') && value.endsWith(']')) {
    return value.slice(1, -1);
  }
  return value;
}

function targetForQualified(qualified: string): SqlSchemaDropTarget {
  const parts = qualified
    .split('.')
    .map(unquote)
    .filter((part) => part.length > 0);
  if (parts.length === 0) return {};
  if (parts.length === 1) return { table: parts[0] };
  return { schema: parts[parts.length - 2], table: parts[parts.length - 1] };
}

function stripLeadingNoise(value: string): string {
  return value
    .replace(/\b(?:CASCADE|RESTRICT)\s*$/i, '')
    .replace(/\s+/g, ' ')
    .trim();
}

export function parseSchemaDropTargets(sql: string): SqlSchemaDropTarget[] {
  const ddlOnly = sql
    .replace(/--[^\n]*/g, ' ')
    .replace(/\/\*[\s\S]*?\*\//g, ' ')
    .replace(/'(?:''|[^'])*'/g, ' ');
  const targets: SqlSchemaDropTarget[] = [];
  const seen = new Set<string>();
  const push = (target: SqlSchemaDropTarget) => {
    if (!target.table) return;
    const key = `${target.schema ?? ''}.${target.table}.${target.column ?? ''}`.toLowerCase();
    if (seen.has(key)) return;
    seen.add(key);
    targets.push(target);
  };

  for (const match of ddlOnly.matchAll(DROP_TABLE_RE)) {
    const body = (match[1] ?? '').split(/\bCASCADE\b|\bRESTRICT\b/i)[0] ?? '';
    for (const part of body.split(',')) {
      const cleaned = stripLeadingNoise(part);
      if (!cleaned) continue;
      const qualified = cleaned.match(QUALIFIED_PART_RE)?.[0];
      if (qualified) push(targetForQualified(qualified));
    }
  }

  for (const match of ddlOnly.matchAll(ALTER_DROP_COLUMN_RE)) {
    const table = targetForQualified(match[1] ?? '');
    const column = unquote(match[2] ?? '');
    if (table.table && column) push({ ...table, column });
  }

  return targets;
}
