import { afterEach, beforeEach, describe, expect, it } from 'vitest';
import { cleanup, render, screen } from '@testing-library/react';
import { WindowChromeFallback } from '../WindowChromeFallback';
import { registerWindowChrome } from '@datazen/extension-points';

// Form-split F2: the fallback renders the chrome SLOT — register a mac-style
// stub so the drag-region assertion still exercises the real slot wiring.
let unregisterChrome: () => void = () => {};

beforeEach(() => {
  unregisterChrome = registerWindowChrome(() => <div data-tauri-drag-region />);
});

afterEach(() => {
  unregisterChrome();
  cleanup();
});

describe('WindowChromeFallback', () => {
  it('renders a macOS drag region so overlay windows can be moved before content loads', () => {
    const { container } = render(<WindowChromeFallback />);
    expect(container.querySelector('[data-testid="window-chrome-fallback"]')).toBeTruthy();
    expect(container.querySelector('[data-tauri-drag-region]')).toBeTruthy();
  });

  // The spinner here is the only thing on screen, so it is the one loading
  // indicator in the app that has to introduce itself. If the `label` prop or
  // the live-region wiring in `Spinner` regresses, these go red.
  it('announces the loading state to screen readers', () => {
    render(<WindowChromeFallback />);
    const status = screen.getByRole('status');
    expect(status.getAttribute('aria-busy')).toBe('true');
    expect(status.textContent).toBe('Loading');
  });

  it('spins a ring, not an icon', () => {
    const { container } = render(<WindowChromeFallback />);
    const status = screen.getByRole('status');
    // Ring glyph: a styled <span>, matching the original hand-written markup.
    const glyph = status.firstElementChild!;
    expect(glyph.tagName).toBe('SPAN');
    const cls = glyph.getAttribute('class') ?? '';
    expect(cls.split(' ')).toEqual(
      expect.arrayContaining(['animate-spin', 'rounded-full', 'border-2', 'border-accent']),
    );
    expect(cls).not.toContain('sr-only');
    expect(container.querySelector('svg')).toBeNull();
  });

  it('keeps the motion itself out of the accessibility tree', () => {
    const { container } = render(<WindowChromeFallback />);
    const glyph = container.querySelector('[aria-hidden="true"]');
    expect(glyph).toBeTruthy();
    // Exactly one live region, so the announcement cannot double up.
    expect(screen.getAllByRole('status')).toHaveLength(1);
  });
});
