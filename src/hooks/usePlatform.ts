import { useEffect, useState } from 'react';

export type Platform = 'macos' | 'windows' | 'linux' | 'unknown';

let cachedPlatform: Platform | null = null;

async function detectPlatform(): Promise<Platform> {
  if (cachedPlatform) return cachedPlatform;

  if ('__TAURI_INTERNALS__' in window) {
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
  const [platform, setPlatform] = useState<Platform>(cachedPlatform ?? 'macos');

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
