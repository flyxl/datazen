/**
 * Optional React binding for the theme module.
 *
 * Import the hook from the `@datazen/ui-plugin-sdk/react` subpath so bundles
 * without React never link against it (react is an optional peer dependency).
 * Plain-JS pages get the same state via `subscribeTheme()` / the
 * `datazen:theme-pack-changed` DOM event.
 */
import { useEffect, useState } from 'react';
import { getThemeState, subscribeTheme } from './theme';
import type { ThemeState } from './theme';

/** Result of {@link useTheme}: the latest applied host theme snapshot. */
export type UseThemeResult = ThemeState;

/**
 * Subscribe a component to the host theme snapshot (`dark` flag plus the
 * `--c-*` / `--dt-*` token map). Re-renders whenever the host pushes a new
 * `theme.apply`; starts from the current state so first paint is themed.
 */
export function useTheme(): UseThemeResult {
  const [state, setState] = useState<UseThemeResult>(getThemeState);
  useEffect(() => subscribeTheme(setState), []);
  return state;
}
