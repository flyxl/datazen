/**
 * OS detection hook (form-split F6 prep, moved home from
 * `src/platform/desktop/os.ts`).
 *
 * The OS dimension (macos/windows/linux) is orthogonal to the form factor —
 * a web build still distinguishes mac/windows browsers — so the hook is
 * shared-facing. Only the plugin-os probe is a host capability: it rides
 * `PlatformServices.getOsPlatform` (desktop binding: the plugin; web binding
 * and the unconfigured fallback: the user agent). The F2 re-export shim to
 * the desktop zone is gone.
 */

import { useEffect, useState } from 'react';
import { getPlatformOrNull } from '../platform/contracts';

export type Platform = 'macos' | 'windows' | 'linux' | 'unknown';

let cachedPlatform: Platform | null = null;

async function detectPlatform(): Promise<Platform> {
  if (cachedPlatform) return cachedPlatform;

  // The host probe when a platform is bound (the desktop binding maps
  // plugin-os and falls back to the user agent itself, matching the original);
  // the user agent otherwise (unconfigured platform, browser form, tests).
  const probed = await getPlatformOrNull()
    ?.services.getOsPlatform()
    .catch(() => null);
  cachedPlatform = probed ?? platformFromUserAgent();
  return cachedPlatform;
}

function platformFromUserAgent(): Platform {
  const ua = navigator.userAgent.toLowerCase();
  if (ua.includes('mac')) return 'macos';
  if (ua.includes('win')) return 'windows';
  if (ua.includes('linux')) return 'linux';
  return 'unknown';
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
  return platformFromUserAgent();
}
