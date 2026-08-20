import { beforeEach, describe, expect, it, vi } from 'vitest';

async function loadModule(search = '') {
  vi.resetModules();
  window.history.replaceState({}, '', search || '/');
  return import('../windowKind');
}

describe('getWindowKind', () => {
  beforeEach(() => {
    vi.resetModules();
  });

  it('defaults to main when no window param', async () => {
    const { getWindowKind } = await loadModule('/');
    expect(getWindowKind()).toBe('main');
  });

  it('maps window query param to kind', async () => {
    const cases: Array<[string, string]> = [
      ['?window=new-connection', 'new-connection'],
      ['?window=connection', 'main'],
      ['?window=settings', 'main'],
      ['?window=data-sync', 'data-sync'],
      ['?window=backup', 'backup'],
      ['?window=workflow', 'main'],
      ['?window=docs', 'main'],
      ['?window=dashboard', 'main'],
    ];
    for (const [search, kind] of cases) {
      const { getWindowKind } = await loadModule(search);
      expect(getWindowKind()).toBe(kind);
    }
  });

  it('caches kind after first read', async () => {
    const { getWindowKind } = await loadModule('?window=settings');
    expect(getWindowKind()).toBe('main');
    window.history.replaceState({}, '', '?window=backup');
    expect(getWindowKind()).toBe('main');
  });
});

describe('getUrlParam', () => {
  it('reads arbitrary query params', async () => {
    const { getUrlParam } = await loadModule('?section=ai&foo=bar');
    expect(getUrlParam('section')).toBe('ai');
    expect(getUrlParam('missing')).toBeNull();
  });
});
