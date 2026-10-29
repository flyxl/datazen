import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

// Form-split R6: persistence rides the HostSettingsPort — the runtime
// `__TAURI_INTERNALS__` probe is gone, so the test drives the port binding
// directly instead of faking a Tauri window.
const platformRef = vi.hoisted(() => ({ current: null as unknown }));

vi.mock('../../platform/contracts', () => ({
  getPlatformOrNull: () => platformRef.current,
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
  const persistMock = vi.fn();

  beforeEach(() => {
    persistMock.mockClear();
    platformRef.current = null;
  });

  afterEach(() => {
    platformRef.current = null;
  });

  it('no-ops when the platform is not configured (pre-assembly, bare test)', () => {
    persistSurfaceBackground(true, DEFAULT_SURFACE_DARK);
    expect(persistMock).not.toHaveBeenCalled();
  });

  it('routes the sanitized hex through the HostSettingsPort when bound', () => {
    platformRef.current = {
      hostSettings: { persistSurfaceBackground: persistMock },
    };
    persistSurfaceBackground(true, 'rgb(26, 10, 46)');
    expect(persistMock).toHaveBeenCalledWith('#1a0a2e', true);
    persistSurfaceBackground(false, 'not-a-color');
    expect(persistMock).toHaveBeenCalledTimes(1);
  });

  it('no-ops when the binding carries no host settings (web stub shape)', () => {
    platformRef.current = {};
    persistSurfaceBackground(true, DEFAULT_SURFACE_DARK);
    expect(persistMock).not.toHaveBeenCalled();
  });
});
