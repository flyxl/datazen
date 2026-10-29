/**
 * Desktop `WindowRouter` implementation (form-split F2, absorbed from
 * `src/lib/windowManager.ts`; slimmed to the low-level machinery in the F6
 * prep — the high-level named-window helpers moved back to the shared
 * `src/lib/windowManager.ts` and reach this port implementation through
 * `requirePlatform().windows`).
 *
 * In the Tauri runtime, creates real OS windows via the Rust-side command so
 * `accept_first_mouse` is applied at the native layer. In browser dev mode,
 * opens new browser tabs with query params. The branch is resolved at
 * COMPILE time via `IS_DESKTOP_BUILD` (ruling #5).
 */

import { invoke } from '@tauri-apps/api/core';
import { IS_DESKTOP_BUILD } from '../env';
import { cssColorToHex } from '../../lib/surfaceBgCache';
import type { WindowChromePort, WindowRouter } from '../contracts';

async function focusMainWindow(): Promise<void> {
  if (!IS_DESKTOP_BUILD) return;
  const { WebviewWindow } = await loadWebviewWindowModule();
  const main = await WebviewWindow.getByLabel('main');
  if (!main) return;
  await main.show();
  await main.unminimize();
  await main.setFocus();
}

/** True when any sub-window (backup, data-sync, …) is still open. */
export async function hasOpenChildWindows(): Promise<boolean> {
  if (!IS_DESKTOP_BUILD) return false;
  const { WebviewWindow } = await import('@tauri-apps/api/webviewWindow');
  const windows = await WebviewWindow.getAll();
  return windows.some((window) => window.label !== 'main');
}

/** Sub-window HTML entry — no splash (see `window.html`). Main stays on `index.html`. */
const SUB_WINDOW_ENTRY = 'window.html';

/** Coalesce concurrent singleton opens (duplicate menu listeners / double-click). */
const singletonOpenInFlight = new Map<string, Promise<void>>();

let webviewWindowModulePromise: Promise<typeof import('@tauri-apps/api/webviewWindow')> | null =
  null;

function loadWebviewWindowModule() {
  webviewWindowModulePromise ??= import('@tauri-apps/api/webviewWindow');
  return webviewWindowModulePromise;
}

function resolveOpenerBackgroundColor(): string | undefined {
  if (typeof document === 'undefined') return undefined;
  const inline = document.documentElement.style.backgroundColor;
  const fromInline = inline ? cssColorToHex(inline) : null;
  if (fromInline) return fromInline;
  const computed = getComputedStyle(document.documentElement).backgroundColor;
  return cssColorToHex(computed) ?? undefined;
}

async function openTauriWindow(label: string, options: import('../contracts').OpenWindowOptions) {
  const qs = new URLSearchParams(options.params ?? {}).toString();
  const url = qs ? `${SUB_WINDOW_ENTRY}?${qs}` : SUB_WINDOW_ENTRY;
  const backgroundColor = resolveOpenerBackgroundColor();

  try {
    await invoke('create_sub_window', {
      options: {
        label,
        url,
        title: options.title ?? 'DataZen',
        width: options.width ?? 800,
        height: options.height ?? 640,
        minWidth: options.minWidth,
        minHeight: options.minHeight,
        center: options.center ?? true,
        ...(backgroundColor ? { backgroundColor } : {}),
      },
    });
  } catch (e) {
    console.error(`[windowManager] failed to open window "${label}"`, e);
    // A rejected invoke (permission denied, build failure) is otherwise
    // indistinguishable from "the click did nothing", and devtools are not
    // available in release builds.
    try {
      const { message } = await import('@tauri-apps/plugin-dialog');
      await message(`${label}\n\n${e instanceof Error ? e.message : String(e)}`, {
        title: 'DataZen',
        kind: 'error',
      });
    } catch {
      // dialog unavailable; the console log above is the only trace
    }
    throw e;
  }
}

function openBrowserWindow(options: import('../contracts').OpenWindowOptions) {
  const qs = new URLSearchParams(options.params ?? {}).toString();
  const url = qs ? `/window.html?${qs}` : '/window.html';
  window.open(url, '_blank', `width=${options.width ?? 800},height=${options.height ?? 640}`);
}

async function focusSingletonWindow(label: string): Promise<boolean> {
  if (!IS_DESKTOP_BUILD) return false;
  const { WebviewWindow } = await loadWebviewWindowModule();
  const existing = await WebviewWindow.getByLabel(label);
  if (!existing) return false;
  await existing.show();
  await existing.unminimize();
  await existing.setFocus();
  return true;
}

export function openSingletonWindow(
  label: string,
  options: import('../contracts').OpenWindowOptions,
) {
  const inflight = singletonOpenInFlight.get(label);
  if (inflight) {
    void inflight.catch(() => {});
    void focusSingletonWindow(label).catch(() => {});
    return;
  }

  const task = (async () => {
    if (IS_DESKTOP_BUILD) {
      if (await focusSingletonWindow(label)) return;
      await openTauriWindow(label, options);
    } else {
      openBrowserWindow(options);
    }
  })().finally(() => {
    singletonOpenInFlight.delete(label);
  });

  singletonOpenInFlight.set(label, task);
  void task.catch(() => {});
}

/** Multi-instance opener — browser tabs / future web routes. */
export function openMultiInstanceWindow(
  _label: string,
  options: import('../contracts').OpenWindowOptions,
) {
  openBrowserWindow(options);
}

// ── Window chrome port (B7: fullscreen / background / close lifecycle) ────────

async function currentWindow() {
  const { getCurrentWindow } = await import('@tauri-apps/api/window');
  return getCurrentWindow();
}

const windowChrome: WindowChromePort = {
  async isFullscreen(): Promise<boolean> {
    return currentWindow().then((win) => win.isFullscreen());
  },
  async onFullscreenChanged(cb: (fs: boolean) => void): Promise<() => void> {
    const { listen } = await import('@tauri-apps/api/event');
    return listen<boolean>('fullscreen-changed', (e) => cb(Boolean(e.payload)));
  },
  async setBackgroundRootColor(hex: string): Promise<void> {
    await (await currentWindow()).setBackgroundColor(hex);
  },
  async onCloseRequested(cb: (event: { preventDefault(): void }) => void): Promise<() => void> {
    const win = await currentWindow();
    return win.onCloseRequested((event) => cb(event));
  },
  async minimizeCurrent(): Promise<void> {
    await (await currentWindow()).minimize();
  },
  async closeCurrent(): Promise<void> {
    await (await currentWindow()).close();
  },
};

export { windowChrome };

/** Compose the desktop `WindowRouter` port implementation. */
export function createDesktopWindowRouter(): WindowRouter {
  return {
    openSingletonWindow,
    openMultiInstanceWindow,
    focusMainWindow,
    hasOpenChildWindows,
    windowChrome,
  };
}
