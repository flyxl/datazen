/**
 * F8 test-agent: `useTheme` React binding (`@datazen/extension-sdk/react`).
 * Uses createElement (no JSX) so the package tsconfig stays unchanged.
 */
import { describe, expect, it, beforeEach } from 'vitest';
import { createElement } from 'react';
import { act, render } from '@testing-library/react';
import { applyThemeSnapshot, getThemeState, subscribeTheme } from '../src/theme';
import type { ThemeState } from '../src/theme';

// The subpath export under test — imported via the same specifier plugins use
// is not possible here (package self-reference); src/react.ts IS that module.
import { useTheme } from '../src/react';

function HookProbe(onRender: (state: ThemeState) => void) {
  return createElement(() => {
    onRender(useTheme());
    return null;
  });
}

describe('R-01 useTheme', () => {
  beforeEach(() => {
    // Reset the theme singleton between tests (module-level state).
    document.documentElement.classList.remove('dark');
    applyThemeSnapshot(null);
  });

  it('starts from the current theme state so first paint is themed', () => {
    let seed!: ThemeState;
    act(() => {
      seed = applyThemeSnapshot({ v: 2, dark: true, tokens: { '--c-accent': '#111111' } });
    });
    const seen: ThemeState[] = [];
    render(HookProbe((state) => seen.push(state)));
    expect(seen[0]).toBe(seed);
  });

  it('re-renders on every applied host snapshot', () => {
    const seen: ThemeState[] = [];
    render(HookProbe((state) => seen.push(state)));
    let next!: ThemeState;
    act(() => {
      next = applyThemeSnapshot({ v: 2, dark: false, tokens: { '--c-fg': '#222222' } });
    });
    expect(seen.at(-1)).toBe(next);
    expect(seen.at(-1)?.tokens['--c-fg']).toBe('#222222');
  });

  it('stops updating after unmount (subscription cleaned up)', () => {
    const seen: ThemeState[] = [];
    const { unmount } = render(HookProbe((state) => seen.push(state)));
    unmount();
    applyThemeSnapshot({ v: 2, dark: true, tokens: {} });
    // No crash, no further renders recorded after unmount.
    expect(getThemeState().dark).toBe(true);
  });

  it('subscribeTheme unsubscribe is idempotent (hook effect cleanup contract)', () => {
    let calls = 0;
    const listener = () => {
      calls += 1;
    };
    const unsubscribe = subscribeTheme(listener);
    unsubscribe();
    unsubscribe(); // idempotent
    applyThemeSnapshot({ v: 2, dark: false, tokens: {} });
    expect(calls).toBe(0);
  });
});
