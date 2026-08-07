import type { ReactNode } from 'react';
import { useCallback, useEffect } from 'react';
import { usePlatform } from '../hooks/usePlatform';
import { useDocsMenuListener } from '../hooks/useDocsMenuListener';
import { useUiStore } from '../stores/uiStore';
import { WindowControls } from './WindowControls';

export interface TitleBarProps {
  title?: ReactNode;
  leftContent?: ReactNode;
  rightContent?: ReactNode;
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
  useDocsMenuListener();

  useEffect(() => {
    if (!isMac || !('__TAURI_INTERNALS__' in window)) return;

    let unlisten: (() => void) | undefined;

    void (async () => {
      const { listen } = await import('@tauri-apps/api/event');
      unlisten = await listen<boolean>('fullscreen-changed', (e) => {
        useUiStore.getState().setFullscreen(e.payload);
      });
    })();

    return () => { unlisten?.(); };
  }, [isMac]);

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
  const handleDragMouseDown = useCallback((e: React.MouseEvent) => {
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
      void (async () => {
        const { getCurrentWindow } = await import('@tauri-apps/api/window');
        await getCurrentWindow().startDragging();
      })();
    };

    window.addEventListener('mousemove', onMove);
    window.addEventListener('mouseup', cleanup);
  }, [isMac]);

  const leftPad = isMac && !isFullscreen ? 'pl-[78px]' : 'pl-3';
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

      <div className={`relative z-10 flex items-center ${leftPad}`}>
        {leftContent}
      </div>

      <div className="pointer-events-none flex min-w-0 flex-1 justify-center">
        {title && (
          <div className="truncate text-xs font-medium text-fg-secondary">{title}</div>
        )}
      </div>

      <div className={`relative z-10 flex items-center ${rightPad}`}>
        {rightContent}
      </div>
    </header>
  );
}
