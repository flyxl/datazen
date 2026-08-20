import { useCallback } from 'react';
import { Minus, Square, X } from 'lucide-react';

/**
 * Web-based window controls for Windows/Linux frameless windows.
 * Renders minimize, maximize/restore, and close buttons.
 */
export function WindowControls() {
  const handleMinimize = useCallback(async () => {
    const { getCurrentWindow } = await import('@tauri-apps/api/window');
    await getCurrentWindow().minimize();
  }, []);

  const handleMaximize = useCallback(async () => {
    const { getCurrentWindow } = await import('@tauri-apps/api/window');
    const win = getCurrentWindow();
    if (await win.isMaximized()) {
      await win.unmaximize();
    } else {
      await win.maximize();
    }
  }, []);

  const handleClose = useCallback(async () => {
    const { getCurrentWindow } = await import('@tauri-apps/api/window');
    await getCurrentWindow().close();
  }, []);

  return (
    <div className="absolute right-0 top-0 z-50 flex h-10 items-stretch" data-no-drag>
      <button
        type="button"
        onClick={handleMinimize}
        className="flex w-[46px] items-center justify-center text-titlebar-fg-muted transition-colors hover:bg-titlebar-hover hover:text-titlebar-fg"
        aria-label="Minimize"
      >
        <Minus className="h-4 w-4" />
      </button>
      <button
        type="button"
        onClick={handleMaximize}
        className="flex w-[46px] items-center justify-center text-titlebar-fg-muted transition-colors hover:bg-titlebar-hover hover:text-titlebar-fg"
        aria-label="Maximize"
      >
        <Square className="h-3 w-3" />
      </button>
      <button
        type="button"
        onClick={handleClose}
        className="flex w-[46px] items-center justify-center text-titlebar-fg-muted transition-colors hover:bg-red-500/80 hover:text-white"
        aria-label="Close"
      >
        <X className="h-4 w-4" />
      </button>
    </div>
  );
}
