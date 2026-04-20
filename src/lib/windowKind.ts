export type WindowKind = 'main' | 'new-connection' | 'connection' | 'query' | 'settings';

let cachedKind: WindowKind | null = null;

export function getWindowKind(): WindowKind {
  if (cachedKind) return cachedKind;

  const params = new URLSearchParams(window.location.search);
  const w = params.get('window');

  if (w === 'new-connection') cachedKind = 'new-connection';
  else if (w === 'connection') cachedKind = 'connection';
  else if (w === 'query') cachedKind = 'query';
  else if (w === 'settings') cachedKind = 'settings';
  else cachedKind = 'main';

  return cachedKind;
}

export function getUrlParam(name: string): string | null {
  return new URLSearchParams(window.location.search).get(name);
}
