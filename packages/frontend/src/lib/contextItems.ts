import type { ContextItem } from '../types';

export function splitContextItems(items: ContextItem[]): {
  contextFiles: string[];
  contextTables: string[];
} {
  const contextFiles: string[] = [];
  const contextTables: string[] = [];
  for (const it of items) {
    if (it.kind === 'table') contextTables.push(it.id);
    else if (it.kind === 'file' || it.kind === 'dir') contextFiles.push(it.path ?? it.id);
  }
  return { contextFiles, contextTables };
}
