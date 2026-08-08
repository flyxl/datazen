/** Build a MongoDB find JSON command for the builtin driver query API. */

export interface MongoFindCommand {
  collection: string;
  filter: Record<string, unknown>;
  limit: number;
  database?: string;
}

export function parseMongoFilterJson(raw: string): Record<string, unknown> {
  const trimmed = raw.trim();
  if (!trimmed) return {};
  const parsed: unknown = JSON.parse(trimmed);
  if (parsed === null || typeof parsed !== 'object' || Array.isArray(parsed)) {
    throw new Error('Filter must be a JSON object');
  }
  return parsed as Record<string, unknown>;
}

export function buildMongoFindCommand(opts: {
  collection: string;
  filterText: string;
  limit: number;
  database?: string | null;
}): string {
  const filter = parseMongoFilterJson(opts.filterText);
  const cmd: MongoFindCommand = {
    collection: opts.collection,
    filter,
    limit: opts.limit,
  };
  if (opts.database) {
    cmd.database = opts.database;
  }
  return JSON.stringify(cmd, null, 2);
}

export function cellToDisplay(value: unknown): string {
  if (value === null || value === undefined) return 'NULL';
  if (typeof value === 'string') return value;
  if (typeof value === 'number' || typeof value === 'boolean') return String(value);
  try {
    return JSON.stringify(value);
  } catch {
    return String(value);
  }
}

export function rowToDocument(
  columns: string[],
  row: unknown[],
): Record<string, unknown> {
  const doc: Record<string, unknown> = {};
  columns.forEach((name, i) => {
    doc[name] = row[i] ?? null;
  });
  return doc;
}
