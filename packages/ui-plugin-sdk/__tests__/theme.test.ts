import { beforeEach, describe, expect, it, vi } from 'vitest';
import {
  DEFAULT_THEME_TOKENS,
  THEME_CHANGED_EVENT,
  applyThemeSnapshot,
  getThemeState,
  startThemeListener,
  subscribeTheme,
} from '../src/theme';
import type { ThemeState } from '../src/theme';

function inlineToken(name: string): string {
  return document.documentElement.style.getPropertyValue(name);
}

function receiveFromParent(source: unknown, data: unknown): void {
  window.dispatchEvent(new MessageEvent('message', { data, source: source as Window | null }));
}

beforeEach(() => {
  // Reset the singleton's DOM footprint between tests.
  const style = document.documentElement.style;
  for (let i = style.length - 1; i >= 0; i -= 1) {
    const name = style.item(i);
    if (name.startsWith('--')) style.removeProperty(name);
  }
  document.documentElement.classList.remove('dark');
});

describe('applyThemeSnapshot', () => {
  it('writes snapshot tokens onto :root and toggles the dark class', () => {
    document.documentElement.classList.add('dark');

    const state = applyThemeSnapshot({
      v: 2,
      dark: false,
      tokens: { '--c-accent': '#123456', '--dt-json': '#abcdef' },
    });

    expect(state.v).toBe(2);
    expect(inlineToken('--c-accent')).toBe('#123456');
    expect(inlineToken('--dt-json')).toBe('#abcdef');
    expect(document.documentElement.classList.contains('dark')).toBe(false);
  });

  it('toggles the dark class on for dark snapshots', () => {
    applyThemeSnapshot({ v: 2, dark: true, tokens: {} });
    expect(document.documentElement.classList.contains('dark')).toBe(true);
  });

  it('fills omitted contract tokens from the built-in defaults (R5)', () => {
    applyThemeSnapshot({ v: 2, dark: true, tokens: { '--c-accent': '#000000' } });
    expect(inlineToken('--c-accent')).toBe('#000000');
    expect(inlineToken('--c-surface')).toBe(DEFAULT_THEME_TOKENS['--c-surface'] ?? '');
    expect(inlineToken('--dt-number')).toBe(DEFAULT_THEME_TOKENS['--dt-number'] ?? '');
  });

  it('drops malformed token entries instead of writing them', () => {
    applyThemeSnapshot({
      v: 2,
      dark: true,
      tokens: { '--c-edge': '#0f0f0f', '--c-broken': 42 } as unknown as Record<string, string>,
    });
    expect(inlineToken('--c-edge')).toBe('#0f0f0f');
    expect(inlineToken('--c-broken')).toBe('');
    expect(inlineToken('--c-fg')).toBe(DEFAULT_THEME_TOKENS['--c-fg'] ?? '');
  });

  it('dispatches datazen:theme-pack-changed on the root element with the state detail', () => {
    const events: CustomEvent<ThemeState>[] = [];
    const root = document.documentElement;
    root.addEventListener(THEME_CHANGED_EVENT, (event) => {
      events.push(event as CustomEvent<ThemeState>);
    });

    const state = applyThemeSnapshot({ v: 2, dark: true, tokens: { '--c-accent': '#ff0000' } });

    expect(events).toHaveLength(1);
    expect(events[0]?.detail).toBe(state);
    expect(events[0]?.detail?.dark).toBe(true);
    expect(events[0]?.detail?.tokens['--c-accent']).toBe('#ff0000');
  });

  it('is idempotent when the same snapshot is applied repeatedly', () => {
    const snapshot = { v: 2, dark: true, tokens: { '--c-accent': '#00ff00', '--dt-text': '#eee' } };
    const listener = vi.fn();
    subscribeTheme(listener);

    const first = applyThemeSnapshot(snapshot);
    const second = applyThemeSnapshot(snapshot);

    expect(second).toEqual(first);
    expect(inlineToken('--c-accent')).toBe('#00ff00');
    expect(inlineToken('--dt-text')).toBe('#eee');
    expect(document.documentElement.classList.contains('dark')).toBe(true);
    // DOM state above is unchanged; each re-push stays observable.
    expect(listener).toHaveBeenCalledTimes(2);
  });
});

describe('theme subscriptions', () => {
  it('notifies subscribers on every apply and stops after unsubscribe', () => {
    const listener = vi.fn();
    const unsubscribe = subscribeTheme(listener);

    applyThemeSnapshot({ v: 2, dark: true, tokens: {} });
    expect(listener).toHaveBeenCalledTimes(1);
    expect(listener.mock.calls[0]?.[0].dark).toBe(true);

    unsubscribe();
    applyThemeSnapshot({ v: 2, dark: false, tokens: {} });
    expect(listener).toHaveBeenCalledTimes(1);
  });

  it('survives subscribers that throw', () => {
    const errors: Error[] = [];
    const unsubscribe = subscribeTheme(() => {
      throw new Error('boom');
    });
    const healthy = vi.fn();
    subscribeTheme(healthy);

    expect(() => applyThemeSnapshot({ v: 2, dark: false, tokens: {} })).not.toThrow();
    expect(errors).toHaveLength(0);
    expect(healthy).toHaveBeenCalledTimes(1);
    unsubscribe();
  });

  it('getThemeState mirrors the latest applied snapshot', () => {
    const state = applyThemeSnapshot({ v: 7, dark: true, tokens: { '--c-edge': '#111' } });
    expect(getThemeState()).toBe(state);
  });
});

describe('startThemeListener', () => {
  function fakeHost(): Window {
    return { postMessage: vi.fn() } as unknown as Window;
  }

  const THEME_APPLY = (tokens: Record<string, string>) => ({
    ch: 'ui-plugin',
    type: 'theme.apply',
    target: 'host',
    payload: { v: 2, dark: true, tokens },
  });

  it('applies theme.apply envelopes arriving from the parent window', () => {
    const parent = fakeHost();
    const detach = startThemeListener({ parentWindow: parent });

    receiveFromParent(parent, THEME_APPLY({ '--c-accent': '#010101' }));

    expect(inlineToken('--c-accent')).toBe('#010101');
    expect(document.documentElement.classList.contains('dark')).toBe(true);
    detach();
  });

  it('ignores messages from other sources or with foreign shapes', () => {
    const parent = fakeHost();
    const detach = startThemeListener({ parentWindow: parent });

    receiveFromParent({}, THEME_APPLY({ '--c-accent': '#bad' }));
    receiveFromParent(null, THEME_APPLY({ '--c-accent': '#bad' }));
    receiveFromParent(parent, { ...THEME_APPLY({ '--c-accent': '#bad' }), ch: 'other' });
    receiveFromParent(parent, { ...THEME_APPLY({ '--c-accent': '#bad' }), type: 'theme.remove' });
    receiveFromParent(parent, 'garbage');

    expect(inlineToken('--c-accent')).toBe('');
    detach();
  });

  it('stops applying after detach', () => {
    const parent = fakeHost();
    const detach = startThemeListener({ parentWindow: parent });
    detach();

    receiveFromParent(parent, THEME_APPLY({ '--c-accent': '#deadbe' }));

    expect(inlineToken('--c-accent')).toBe('');
  });

  it('re-applies subsequent pushes so live theme switches reach the page', () => {
    const parent = fakeHost();
    const detach = startThemeListener({ parentWindow: parent });

    receiveFromParent(parent, THEME_APPLY({ '--c-accent': '#111111' }));
    receiveFromParent(parent, {
      ...THEME_APPLY({ '--c-accent': '#222222' }),
      payload: { v: 2, dark: false, tokens: { '--c-accent': '#222222' } },
    });

    expect(inlineToken('--c-accent')).toBe('#222222');
    expect(document.documentElement.classList.contains('dark')).toBe(false);
    detach();
  });
});
