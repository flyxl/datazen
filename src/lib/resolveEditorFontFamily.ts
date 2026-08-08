export const HOST_DEFAULT_EDITOR_FONT =
  'Menlo, Monaco, Consolas, "Courier New", monospace';

/**
 * Resolve SQL editor font: explicit user setting wins over theme `--font-editor`.
 */
export function resolveEditorFontFamily(
  userSetting: string,
  computedEditorVar: string,
  fallback: string,
): string {
  const trimmedUser = userSetting.trim();
  if (trimmedUser && trimmedUser !== HOST_DEFAULT_EDITOR_FONT) {
    return trimmedUser;
  }
  const trimmedTheme = computedEditorVar.trim();
  if (trimmedTheme) {
    return trimmedTheme;
  }
  return fallback;
}
