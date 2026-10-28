/**
 * OS detection (form-split F2, absorbed from `src/hooks/usePlatform.ts`).
 *
 * The OS dimension (macos/windows/linux) is orthogonal to the form factor —
 * a web build still distinguishes mac/windows browsers — so the hook stays
 * shared-facing through the re-export shim. Only the plugin-os probe lives
 * here, desktop-zone-only.
 */

import { useEffect, useState } from 'react';
import { IS_DESKTOP_BUILD } from './env';

export type Platform = 'macos' | 'windows' | 'linux' | 'unknown';

let cachedPlatform: Platform | null = null;

async function detectPlatform(): Promise<Platform> {
  if (cachedPlatform) return cachedPlatform;

  if (IS_DESKTOP_BUILD) {
    try {
      const { platform } = await import('@tauri-apps/plugin-os');
      const p = platform();
      if (p === 'macos') cachedPlatform = 'macos';
      else if (p === 'windows') cachedPlatform = 'windows';
      else if (p === 'linux') cachedPlatform = 'linux';
      else cachedPlatform = 'unknown';
      return cachedPlatform;
    } catch {
      // plugin-os not available, fall back to user agent
    }
  }

  const ua = navigator.userAgent.toLowerCase();
  if (ua.includes('mac')) cachedPlatform = 'macos';
  else if (ua.includes('win')) cachedPlatform = 'windows';
  else if (ua.includes('linux')) cachedPlatform = 'linux';
  else cachedPlatform = 'unknown';
  return cachedPlatform;
}

export function usePlatform(): Platform {
  // Initialize synchronously from the user agent so the first frame on
  // Windows/Linux renders the Windows titlebar instead of briefly
  // mounting the macOS `data-tauri-drag-region` overlay (which can
  // intercept clicks on WebView2 before the async detection resolves).
  const [platform, setPlatform] = useState<Platform>(getPlatformSync());

  useEffect(() => {
    void detectPlatform().then(setPlatform);
  }, []);

  return platform;
}

export function getPlatformSync(): Platform {
  if (cachedPlatform) return cachedPlatform;
  const ua = navigator.userAgent.toLowerCase();
  if (ua.includes('mac')) return 'macos';
  if (ua.includes('win')) return 'windows';
  if (ua.includes('linux')) return 'linux';
  return 'unknown';
}
