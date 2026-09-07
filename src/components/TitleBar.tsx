import type { ReactNode } from 'react';
import { useCallback, useEffect } from 'react';
import { listen } from '@tauri-apps/api/event';
import { getCurrentWindow } from '@tauri-apps/api/window';
import { usePlatform } from '../hooks/usePlatform';
import { useUiStore } from '../stores/uiStore';
import { WindowControls } from './WindowControls';

export interface TitleBarProps {
  title?: ReactNode;
  leftContent?: ReactNode;
  rightContent?: ReactNode;
}

/**
 * Synchronous DOM check to detect whether the macOS window is currently
 * occupying the full native screen in Spaces mode.
 */
function isLikelyFullscreen(): boolean {
  if (typeof window === 'undefined') return false;
  if (document.fullscreenElement) return true;
  // In unit test environment, defer strictly to store state for controllability
  if (import.meta.env?.MODE === 'test') return false;
  if (typeof window.screen === 'undefined') return false;
  const availHeight = window.screen.availHeight;
  const screenHeight = window.screen.height;
  // In native macOS fullscreen space: menu bar is hidden, so availHeight equals screen height
  // and innerHeight covers almost the entire screen height.
  return (
    availHeight > 0 &&
    screenHeight > 0 &&
    availHeight === screenHeight &&
    Math.abs(window.innerHeight - screenHeight) <= 10
  );
}

/**
 * Cross-platform title bar.
 *
 * macOS: uses native titleBarStyle "Overlay" with system traffic lights.
 *        Drag handled by `data-tauri-drag-region` (safe on macOS overlay).
 *
 * Windows/Linux: frameless window (decorations: false).
 *        Drag handled by `startDragging()` JS API on mousedown.
 *        This avoids the native-level event interception that
 *        `data-tauri-drag-region` causes on Windows/WebView2.
 *        Window controls rendered in web (WindowControls component).
 */
export function TitleBar({ title, leftContent, rightContent }: TitleBarProps) {
  const platform = usePlatform();
  const isMac = platform === 'macos';
  const isFullscreen = useUiStore((s) => s.isFullscreen);

  useEffect(() => {
    if (!isMac || !('__TAURI_INTERNALS__' in window)) return;

    let unlisten: (() => void) | undefined;
    let unlistenResized: (() => void) | undefined;
    let disposed = false;

    const syncWithRetries = () => {
      void useUiStore.getState().syncFullscreen();
      // Retry after macOS fullscreen transition animation settles (approx 250ms - 500ms)
      setTimeout(() => {
        if (!disposed) void useUiStore.getState().syncFullscreen();
      }, 250);
      setTimeout(() => {
        if (!disposed) void useUiStore.getState().syncFullscreen();
      }, 500);
    };

    // Immediately sync current fullscreen state on mount with retries
    syncWithRetries();

    void listen<boolean>('fullscreen-changed', (e) => {
      if (!disposed) {
        useUiStore.getState().setFullscreen(e.payload);
      }
    }).then((fn) => {
      if (disposed) fn();
      else unlisten = fn;
    });

    void getCurrentWindow()
      .onResized(() => {
        if (!disposed) {
          syncWithRetries();
        }
      })
      .then((fn) => {
        if (disposed) fn();
        else unlistenResized = fn;
      });

    const onWindowResizeOrFocus = () => {
      if (!disposed) {
        syncWithRetries();
      }
    };

    window.addEventListener('resize', onWindowResizeOrFocus);
    window.addEventListener('focus', onWindowResizeOrFocus);
    document.addEventListener('visibilitychange', onWindowResizeOrFocus);

    return () => {
      disposed = true;
      unlisten?.();
      unlistenResized?.();
      window.removeEventListener('resize', onWindowResizeOrFocus);
      window.removeEventListener('focus', onWindowResizeOrFocus);
      document.removeEventListener('visibilitychange', onWindowResizeOrFocus);
    };
  }, [isMac]);

  // Sync immediately whenever leftContent changes (e.g. settings back button appears)
  useEffect(() => {
    if (isMac && leftContent && '__TAURI_INTERNALS__' in window) {
      void useUiStore.getState().syncFullscreen();
    }
  }, [isMac, leftContent]);

  /**
   * Windows/Linux title bar dragging.
   *
   * Drag does not start on mousedown; we wait for a small movement
   * threshold first. Calling startDragging() immediately on mousedown
   * enters a native modal drag loop on Windows, which swallows the
   * mouse-up and can leave subsequent clicks in the webview dead until
   * focus changes. Waiting for real movement keeps plain clicks intact
   * (same fix Wails applied for frameless-window drag).
   */
  const handleDragMouseDown = useCallback(
    (e: React.MouseEvent) => {
      if (isMac) return;
      if (e.button !== 0) return;
      if ((e.target as HTMLElement).closest('button, a, input, [data-no-drag]')) return;

      const startX = e.clientX;
      const startY = e.clientY;

      const cleanup = () => {
        window.removeEventListener('mousemove', onMove);
        window.removeEventListener('mouseup', cleanup);
      };

      const onMove = (ev: MouseEvent) => {
        if (Math.abs(ev.clientX - startX) < 4 && Math.abs(ev.clientY - startY) < 4) return;
        cleanup();
        void getCurrentWindow().startDragging();
      };

      window.addEventListener('mousemove', onMove);
      window.addEventListener('mouseup', cleanup);
    },
    [isMac],
  );

  const effectiveFullscreen = isFullscreen || (isMac && isLikelyFullscreen());
  const leftPad = isMac && !effectiveFullscreen ? 'pl-[78px]' : 'pl-3';
  const rightPad = isMac ? 'pr-[14px]' : 'pr-[140px]';

  return (
    <header
      className="relative flex h-10 min-h-[40px] shrink-0 items-center bg-titlebar"
      onMouseDown={!isMac ? handleDragMouseDown : undefined}
    >
      {/* macOS only: native drag region via attribute (safe with overlay titlebar) */}
      {isMac && <div className="absolute inset-0" data-tauri-drag-region />}
      {/* Windows/Linux: web window controls (absolute positioned at right) */}
      {!isMac && <WindowControls />}

      <div className={`relative z-10 flex items-center ${leftPad}`}>{leftContent}</div>

      <div className="pointer-events-none flex min-w-0 flex-1 justify-center">
        {title && <div className="truncate text-xs font-medium text-titlebar-fg">{title}</div>}
      </div>

      <div className={`relative z-10 flex items-center ${rightPad}`}>{rightContent}</div>
    </header>
  );
}
