/**
 * @datazen/frontend/shell — the shell-facing slice (host-boundary follow-up F2).
 *
 * Desktop-owned shell components (TitleBar, WindowControls in
 * `apps/desktop/src/shell/desktop/`) render inside the app frame but live in
 * the app, not in this package. This subpath is the minimal shared surface
 * they may pull from: the i18n hook, the OS-dimension platform hook, the UI
 * store and the window-surface color helper.
 *
 * It deliberately does NOT re-export the store family — anything beyond
 * shell chrome belongs behind a platform port or a real feature entry, not
 * behind "whatever the title bar needed today".
 */
export { useI18n, type I18nParams } from './hooks/useI18n';
export { usePlatform, type Platform } from './hooks/usePlatform';
export { useUiStore } from './stores/uiStore';
export { cssColorToHex } from './lib/surfaceBgCache';
