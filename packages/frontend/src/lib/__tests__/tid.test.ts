import { afterEach, describe, expect, it, vi } from 'vitest';
import { tid } from '../tid';

afterEach(() => {
  vi.unstubAllEnvs();
});

describe('tid', () => {
  it('returns a data-testid attribute when VITE_E2E is enabled (E2E build)', () => {
    vi.stubEnv('VITE_E2E', '1');
    expect(tid('editor-execute-button')).toEqual({
      'data-testid': 'editor-execute-button',
    });
  });

  it('returns an empty object when VITE_E2E is empty (production build)', () => {
    vi.stubEnv('VITE_E2E', '');
    expect(tid('editor-execute-button')).toEqual({});
  });

  it('returns an empty object when VITE_E2E is absent', () => {
    vi.stubEnv('VITE_E2E', undefined);
    expect(tid('anything')).toEqual({});
  });

  it('propagates the semantic id verbatim', () => {
    vi.stubEnv('VITE_E2E', 'true');
    const attrs = tid('conn-toolbar-new-query');
    expect(attrs).toEqual({ 'data-testid': 'conn-toolbar-new-query' });
    if ('data-testid' in attrs) {
      expect(attrs['data-testid']).toBe('conn-toolbar-new-query');
    }
  });
});
