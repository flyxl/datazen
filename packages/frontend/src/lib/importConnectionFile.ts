/** Whether the import dialog should require a password before importing. */
export function importFilePasswordPolicy(path: string): 'required' | 'optional' {
  const base = path.split(/[/\\]/).pop() ?? path;
  const dot = base.lastIndexOf('.');
  const ext = dot >= 0 ? base.slice(dot + 1).toLowerCase() : '';
  if (
    ext === 'datazenconnection' ||
    ext === 'datazenconnections' ||
    ext === 'tableplusconnection'
  ) {
    return 'required';
  }
  return 'optional';
}

export function importFileDisplayName(path: string): string {
  const parts = path.split(/[/\\]/);
  return parts[parts.length - 1] ?? path;
}
