export type WindowKind = 'main' | 'new-connection' | 'data-sync' | 'schema-diff' | 'backup';

let cachedKind: WindowKind | null = null;

/** Legacy sub-window kinds that now route to the unified main shell. */
const LEGACY_MAIN_ALIASES = new Set(['connection', 'workflow', 'dashboard', 'settings', 'docs']);

export function getWindowKind(): WindowKind {
  if (cachedKind) return cachedKind;

  const params = new URLSearchParams(window.location.search);
  const w = params.get('window');

  if (w === 'new-connection') cachedKind = 'new-connection';
  else if (w === 'data-sync') cachedKind = 'data-sync';
  else if (w === 'schema-diff') cachedKind = 'schema-diff';
  else if (w === 'backup') cachedKind = 'backup';
  else if (w && LEGACY_MAIN_ALIASES.has(w)) cachedKind = 'main';
  else cachedKind = 'main';

  return cachedKind;
}

export function getUrlParam(name: string): string | null {
  return new URLSearchParams(window.location.search).get(name);
}
