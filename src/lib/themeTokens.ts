/**
 * Theme token contract shared with UI plugins (PRD §4.4).
 *
 * `THEME_TOKENS` lists the CSS custom property names a plugin may consume:
 * the full `--c-*` semantic color set (styles/themes.css) and the full
 * `--dt-*` DataTable type-color set (see src/lib/dataTypeColors.ts). Only
 * names are stored here — values are read live from computed styles so
 * snapshots always reflect the active base theme + applied theme pack.
 */
import { EXTENSION_API_VERSION } from '../types/plugin';

export const THEME_TOKENS = [
  // Backgrounds
  '--c-surface',
  '--c-surface-alt',
  '--c-surface-raised',
  '--c-surface-inset',
  // Borders
  '--c-edge',
  // Foreground / text
  '--c-fg',
  '--c-fg-secondary',
  '--c-fg-muted',
  // Accent & status
  '--c-accent',
  '--c-success',
  '--c-warning',
  '--c-danger',
  '--c-query-run',
  // Title bar
  '--c-titlebar',
  '--c-titlebar-fg',
  '--c-titlebar-fg-muted',
  '--c-titlebar-hover',
  // DataTable cell colors by SQL-ish type family
  '--dt-null',
  '--dt-bool',
  '--dt-number',
  '--dt-datetime',
  '--dt-json',
  '--dt-text',
  '--dt-binary',
] as const;

export type ThemeTokenName = (typeof THEME_TOKENS)[number];

/** Wire version of the theme snapshot payload (`theme.apply` / `host.ready`). */
export const THEME_SNAPSHOT_VERSION = EXTENSION_API_VERSION;

export interface ThemeSnapshot {
  /** Protocol version of this snapshot shape. */
  v: number;
  dark: boolean;
  tokens: Record<string, string>;
}

/** Read every contract token from the host root element's computed style. */
export function buildThemeSnapshot(): ThemeSnapshot {
  const dark = document.documentElement.classList.contains('dark');
  const style = getComputedStyle(document.documentElement);
  const tokens: Record<string, string> = {};
  for (const name of THEME_TOKENS) {
    tokens[name] = style.getPropertyValue(name).trim();
  }
  return { v: THEME_SNAPSHOT_VERSION, dark, tokens };
}
