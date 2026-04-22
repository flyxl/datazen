import { useState } from 'react';
import { cn } from '../lib/cn';
import { useI18n } from '../hooks/useI18n';

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

async function toggleFullscreen() {
  if (isTauri()) {
    const { getCurrentWindow } = await import('@tauri-apps/api/window');
    const win = getCurrentWindow();
    const isFull = await win.isFullscreen();
    await win.setFullscreen(!isFull);
  }
}

async function toggleMaximize() {
  if (isTauri()) {
    const { getCurrentWindow } = await import('@tauri-apps/api/window');
    await getCurrentWindow().toggleMaximize();
  }
}

export function TrafficLights() {
  const { t } = useI18n();
  const [hovered, setHovered] = useState(false);

  return (
    <div
      className="relative z-10 flex shrink-0 items-center gap-2 px-2"
      style={{ WebkitAppRegion: 'no-drag' } as React.CSSProperties}
      onMouseEnter={() => setHovered(true)}
      onMouseLeave={() => setHovered(false)}
    >
      <button
        type="button"
        onClick={() => void closeWindow()}
        className={cn(
          'group relative h-3 w-3 rounded-full bg-[#ff5f57] transition-colors',
          'hover:bg-[#ff3b30] active:bg-[#bf4943]',
        )}
        title={t('traffic.close')}
      >
        {hovered && (
          <svg className="absolute inset-0 h-3 w-3" viewBox="0 0 12 12">
            <path d="M3.5 3.5L8.5 8.5M8.5 3.5L3.5 8.5" stroke="rgba(0,0,0,0.5)" strokeWidth="1.2" strokeLinecap="round" />
          </svg>
        )}
      </button>
      <button
        type="button"
        onClick={() => void minimizeWindow()}
        className={cn(
          'group relative h-3 w-3 rounded-full bg-[#febc2e] transition-colors',
          'hover:bg-[#f0a000] active:bg-[#bf9022]',
        )}
        title={t('traffic.minimize')}
      >
        {hovered && (
          <svg className="absolute inset-0 h-3 w-3" viewBox="0 0 12 12">
            <path d="M3 6H9" stroke="rgba(0,0,0,0.5)" strokeWidth="1.2" strokeLinecap="round" />
          </svg>
        )}
      </button>
      <button
        type="button"
        onClick={(e) => {
          if (e.altKey) {
            void toggleMaximize();
          } else {
            void toggleFullscreen();
          }
        }}
        className={cn(
          'group relative h-3 w-3 rounded-full bg-[#28c840] transition-colors',
          'hover:bg-[#1aad29] active:bg-[#1f9a30]',
        )}
        title={t('traffic.fullscreen')}
      >
        {hovered && (
          <svg className="absolute inset-0 h-3 w-3" viewBox="0 0 12 12">
            <path d="M3.5 6.5L3.5 8.5L5.5 8.5M8.5 5.5L8.5 3.5L6.5 3.5" stroke="rgba(0,0,0,0.5)" strokeWidth="1.1" strokeLinecap="round" strokeLinejoin="round" fill="none" />
          </svg>
        )}
      </button>
    </div>
  );
}
