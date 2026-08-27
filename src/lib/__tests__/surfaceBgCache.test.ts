import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

const mockInvoke = vi.fn().mockResolvedValue(undefined);

vi.mock('@tauri-apps/api/core', () => ({
  invoke: (...args: unknown[]) => mockInvoke(...args),
}));

import {
  DEFAULT_SURFACE_DARK,
  cssColorToHex,
  isSafeCssHex,
  persistSurfaceBackground,
} from '../surfaceBgCache';

describe('cssColorToHex', () => {
  it('normalizes hex3 / hex6 / rgb', () => {
    expect(cssColorToHex('#0f172a')).toBe('#0f172a');
    expect(cssColorToHex('#ABC')).toBe('#aabbcc');
    expect(cssColorToHex('rgb(17, 34, 51)')).toBe('#112233');
    expect(cssColorToHex('rgba(15, 23, 42, 0.9)')).toBe('#0f172a');
  });

  it('rejects non-hex / non-rgb', () => {
    expect(cssColorToHex('oklch(0.5 0.1 200)')).toBeNull();
    expect(cssColorToHex('red')).toBeNull();
    expect(isSafeCssHex('#112233')).toBe(true);
    expect(isSafeCssHex('rgb(1,2,3)')).toBe(false);
  });
});

describe('persistSurfaceBackground', () => {
  beforeEach(() => {
    mockInvoke.mockClear();
    delete (window as Record<string, unknown>).__TAURI_INTERNALS__;
  });

  afterEach(() => {
    delete (window as Record<string, unknown>).__TAURI_INTERNALS__;
  });

  it('no-ops without Tauri', () => {
    persistSurfaceBackground(true, DEFAULT_SURFACE_DARK);
    expect(mockInvoke).not.toHaveBeenCalled();
  });

  it('sends sanitized hex to Rust when Tauri is present', () => {
    (window as Record<string, unknown>).__TAURI_INTERNALS__ = {};
    persistSurfaceBackground(true, 'rgb(26, 10, 46)');
    expect(mockInvoke).toHaveBeenCalledWith('set_surface_background', {
      hex: '#1a0a2e',
      dark: true,
    });
    persistSurfaceBackground(false, 'not-a-color');
    expect(mockInvoke).toHaveBeenCalledTimes(1);
  });
});
