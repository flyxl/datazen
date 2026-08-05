import type { ReactNode } from 'react';
import { usePlatform } from '../hooks/usePlatform';
import { WindowControls } from './WindowControls';

export interface TitleBarProps {
  title?: ReactNode;
  /** Extra content placed next to the window controls on the left (macOS) or left side (Windows). */
  leftContent?: ReactNode;
  /** Extra content placed on the right side (macOS) or next to controls (Windows). */
  rightContent?: ReactNode;
}

/**
 * Cross-platform title bar.
 * - macOS: uses native titleBarStyle overlay with system traffic lights;
 *          left padding reserves space for the native buttons
 * - Windows/Linux: title + left content on the left, window controls on the right
 */
export function TitleBar({ title, leftContent, rightContent }: TitleBarProps) {
  const platform = usePlatform();
  const isMac = platform === 'macos';

  return (
    <header className="relative flex h-10 min-h-[40px] shrink-0 items-center bg-titlebar">
      <div className="absolute inset-0" data-tauri-drag-region />

      {isMac ? (
        <>
          {/* Reserve space for native traffic lights (overlay mode) */}
          <div className="w-[78px] shrink-0" />
          {leftContent && (
            <div className="relative z-10 flex items-center">{leftContent}</div>
          )}
          <div className="pointer-events-none flex min-w-0 flex-1 justify-center">
            {title && (
              <div className="truncate text-xs font-medium text-fg-secondary">{title}</div>
            )}
          </div>
          {rightContent ? (
            <div className="relative z-10 pr-3">{rightContent}</div>
          ) : (
            <div className="w-[78px] shrink-0" />
          )}
        </>
      ) : (
        <>
          {/* Windows/Linux: icon + title left, controls right */}
          <div className="relative z-10 flex items-center gap-2 pl-3">
            {title && (
              <span className="truncate text-xs font-medium text-fg-secondary">{title}</span>
            )}
            {leftContent}
          </div>
          <div className="flex-1" data-tauri-drag-region />
          {rightContent && (
            <div className="relative z-10 flex items-center pr-1">{rightContent}</div>
          )}
          <WindowControls />
        </>
      )}
    </header>
  );
}
