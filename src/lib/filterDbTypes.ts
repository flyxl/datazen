export function filterDbTypesByQuery<T extends { value: string; label: string }>(
  items: T[],
  query: string,
): T[] {
  const q = query.trim().toLowerCase();
  if (!q) return items;
  return items.filter(
    (item) =>
      item.label.toLowerCase().includes(q) || item.value.toLowerCase().includes(q),
  );
}
