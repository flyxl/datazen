export function rowToRecord(
  row: unknown[] | null,
  columns: { name: string }[],
): Record<string, unknown> | null {
  if (row === null) return null;
  const record: Record<string, unknown> = {};
  for (let i = 0; i < columns.length; i++) {
    record[columns[i].name] = row[i] ?? null;
  }
  return record;
}
