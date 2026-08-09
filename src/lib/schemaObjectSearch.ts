/** Match schema objects by table/view name or column name. */

export function tableMatchesObjectSearch(
  tableName: string,
  query: string,
  columns?: readonly string[] | null,
): boolean {
  const q = query.trim().toLowerCase();
  if (!q) return true;
  if (tableName.toLowerCase().includes(q)) return true;
  if (q.length < 2 || !columns?.length) return false;
  return columns.some((col) => col.toLowerCase().includes(q));
}

/** Columns that matched the query (for UI hints). */
export function matchingColumns(
  query: string,
  columns?: readonly string[] | null,
): string[] {
  const q = query.trim().toLowerCase();
  if (q.length < 2 || !columns?.length) return [];
  return columns.filter((col) => col.toLowerCase().includes(q));
}
