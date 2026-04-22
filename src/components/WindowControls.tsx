import { useState } from 'react';
import { cn } from '../lib/cn';

function isTauri(): boolean {
  return '__TAURI_INTERNALS__' in window;
}

async function closeWindow() {
  if (isTauri()) {
    const { getCurrentWindow } = await import('@tauri-apps/api/window');
    await getCurrentWindow().close();
  }
}

async function minimizeWindow() {
  if (isTauri()) {
    const { getCurrentWindow } = await import('@tauri-apps/api/window');
    await getCurrentWindow().minimize();
  }
}

async function toggleMaximize() {
  if (isTauri()) {
    const { getCurrentWindow } = await import('@tauri-apps/api/window');
    await getCurrentWindow().toggleMaximize();
  }
}

function MinimizeIcon() {
  return (
    <svg width="10" height="10" viewBox="0 0 10 10" fill="currentColor">
      <rect x="1" y="4.5" width="8" height="1" />
    </svg>
  );
}

function MaximizeIcon() {
  return (
    <svg width="10" height="10" viewBox="0 0 10 10" fill="none" stroke="currentColor" strokeWidth="1">
      <rect x="1.5" y="1.5" width="7" height="7" />
    </svg>
  );
}

function CloseIcon() {
  return (
    <svg width="10" height="10" viewBox="0 0 10 10" fill="currentColor">
      <path d="M1.5 1.5L8.5 8.5M8.5 1.5L1.5 8.5" stroke="currentColor" strokeWidth="1.2" strokeLinecap="round" />
    </svg>
  );
}

export function WindowControls() {
  const [hoveredBtn, setHoveredBtn] = useState<string | null>(null);

  return (
    <div
      className="relative z-10 flex shrink-0 items-center"
      style={{ WebkitAppRegion: 'no-drag' } as React.CSSProperties}
    >
      <button
        type="button"
        onClick={() => void minimizeWindow()}
        onMouseEnter={() => setHoveredBtn('min')}
        onMouseLeave={() => setHoveredBtn(null)}
        className={cn(
          'flex h-10 w-[46px] items-center justify-center text-fg-muted transition-colors',
          hoveredBtn === 'min' && 'bg-fg/10 text-fg',
        )}
      >
        <MinimizeIcon />
      </button>
      <button
        type="button"
        onClick={() => void toggleMaximize()}
        onMouseEnter={() => setHoveredBtn('max')}
        onMouseLeave={() => setHoveredBtn(null)}
        className={cn(
          'flex h-10 w-[46px] items-center justify-center text-fg-muted transition-colors',
          hoveredBtn === 'max' && 'bg-fg/10 text-fg',
        )}
      >
        <MaximizeIcon />
      </button>
      <button
        type="button"
        onClick={() => void closeWindow()}
        onMouseEnter={() => setHoveredBtn('close')}
        onMouseLeave={() => setHoveredBtn(null)}
        className={cn(
          'flex h-10 w-[46px] items-center justify-center text-fg-muted transition-colors',
          hoveredBtn === 'close' && 'bg-red-500 text-white',
        )}
      >
        <CloseIcon />
      </button>
    </div>
  );
}
