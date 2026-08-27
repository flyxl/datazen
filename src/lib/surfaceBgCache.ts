import { invoke } from '@tauri-apps/api/core';

export const DEFAULT_SURFACE_DARK = '#0f172a';
export const DEFAULT_SURFACE_LIGHT = '#ffffff';

const SAFE_HEX = /^#([0-9a-fA-F]{3}|[0-9a-fA-F]{6})$/;

export function isSafeCssHex(value: string): boolean {
  return SAFE_HEX.test(value.trim());
}

/** Convert `rgb(r, g, b)` / `#rgb` / `#rrggbb` to `#rrggbb`, or null. */
export function cssColorToHex(value: string): string | null {
  const trimmed = value.trim();
  if (SAFE_HEX.test(trimmed)) {
    if (trimmed.length === 4) {
      const r = trimmed[1];
      const g = trimmed[2];
      const b = trimmed[3];
      return `#${r}${r}${g}${g}${b}${b}`.toLowerCase();
    }
    return trimmed.toLowerCase();
  }
  const rgb = trimmed.match(/^rgba?\(\s*(\d+)\s*,\s*(\d+)\s*,\s*(\d+)/i);
  if (!rgb) return null;
  const hex = [rgb[1], rgb[2], rgb[3]].map((n) => Number(n).toString(16).padStart(2, '0')).join('');
  return `#${hex}`;
}

/** Persist last-resolved surface to Rust `{appData}/surface-bg.json` for init-script bake. */
export function persistSurfaceBackground(isDark: boolean, cssColor: string): void {
  const hex = cssColorToHex(cssColor);
  if (!hex) return;
  if (typeof window === 'undefined' || !('__TAURI_INTERNALS__' in window)) return;
  void invoke('set_surface_background', { hex, dark: isDark }).catch(() => {
    // Cache write is best-effort; next window still has in-memory Rust state if IPC reached set().
  });
}
