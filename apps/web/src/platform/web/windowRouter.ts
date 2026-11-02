/**
 * Web window router (WR-E1) — no child windows, no chrome.
 *
 * Mirrors `packages/frontend/src/platform/web/stub.ts`
 * (`createWebWindowRouter`). The stub in the shared package is not publicly
 * exported (public-entry gate), so the production web assembly owns this
 * file, as the desktop form owns
 * `apps/desktop/src/platform/desktop/windowRouter.ts`.
 *
 * The browser tab is the window: `openSingletonWindow` /
 * `openMultiInstanceWindow` are no-ops (the shared App maps the `?window=`
 * URL param to a page/panel instead — the established "window capability →
 * page" form), fullscreen rides the document API, and close is
 * best-effort `window.close()`.
 */

import type { PlatformBindings } from '@datazen/frontend';

export function createWebWindowRouter(): PlatformBindings['windows'] {
  return {
    openSingletonWindow() {
      // No multi-window topology in a browser tab.
    },
    openMultiInstanceWindow() {
      // No multi-window topology in a browser tab.
    },
    async focusMainWindow() {
      window.focus();
    },
    async hasOpenChildWindows() {
      return false;
    },
    windowChrome: {
      async isFullscreen() {
        return Boolean(document.fullscreenElement);
      },
      async onFullscreenChanged(cb) {
        const handler = () => cb(Boolean(document.fullscreenElement));
        document.addEventListener('fullscreenchange', handler);
        return () => document.removeEventListener('fullscreenchange', handler);
      },
      async setBackgroundRootColor() {
        // The page background is CSS; the port is a no-op here.
      },
      async onCloseRequested() {
        // The browser owns the close button.
        return () => undefined;
      },
      async minimizeCurrent() {
        // No minimize semantics in a tab.
      },
      async closeCurrent() {
        window.close();
      },
    },
  };
}
