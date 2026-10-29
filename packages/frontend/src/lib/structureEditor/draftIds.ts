export function newColumnId(): string {
  return `col_${Math.random().toString(36).slice(2, 8)}`;
}

export function newIndexId(): string {
  return `idx_${Math.random().toString(36).slice(2, 8)}`;
}
