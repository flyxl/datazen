import { mark } from './lib/startupTimer';

const htmlStart = (globalThis as Record<string, unknown>).__HTML_START__ as number | undefined;
if (htmlStart) {
  mark(`HTML → JS module (${Math.round(performance.now() - htmlStart)}ms from HTML)`);
} else {
  mark('JS module start');
}

import React from 'react';
import ReactDOM from 'react-dom/client';
mark('core imports done (React, ReactDOM)');

import App from './App';
mark('App module loaded');

import './styles/globals.css';
mark('CSS loaded');

import { hideSplash } from './lib/splash';
import { bootstrapDefaultIconResolver } from './lib/bootstrapIconResolver';
import { maybeCheckOnStartup } from './lib/updater';
import { getWindowKind } from './lib/windowKind';

bootstrapDefaultIconResolver();

async function bootstrap() {
  try {
    if ('__TAURI_INTERNALS__' in globalThis) {
      try {
        const { useSettingsStore } = await import('./stores/settingsStore');
        await useSettingsStore.getState().loadSettings();
        mark('settings loaded before first paint');
        if (getWindowKind() === 'main') {
          const { checkForUpdatesOnStartup } = useSettingsStore.getState().settings;
          void maybeCheckOnStartup(checkForUpdatesOnStartup);
        }
      } catch {
        mark('settings preload skipped (load failed)');
      }
    }

    ReactDOM.createRoot(document.getElementById('root')!).render(
      <React.StrictMode>
        <App />
      </React.StrictMode>,
    );
    mark('React.render() called (Suspense shell)');
  } finally {
    hideSplash(document.getElementById('splash'));
  }
}

void bootstrap();

/** DEV-only: expose hidden migration tool windows when productFeatures gates UI entries. */
if (import.meta.env.DEV && getWindowKind() === 'main') {
  void import('./lib/windowManager').then((wm) => {
    const dev = {
      openDataTransfer: () => wm.openDataTransferWindow(),
      openDataSync: () => wm.openDataSyncWindow(),
      openSchemaDiff: () => wm.openSchemaDiffWindow(),
    };
    (globalThis as Record<string, unknown>).__datazenDev = dev;

    const autoOpen = import.meta.env.VITE_DEV_OPEN_WINDOW;
    if (autoOpen === 'data-transfer') dev.openDataTransfer();
    else if (autoOpen === 'data-sync') dev.openDataSync();
    else if (autoOpen === 'schema-diff') dev.openSchemaDiff();
  });
}

if ('__TAURI_INTERNALS__' in globalThis) {
  import('@tauri-apps/api/window').then(({ getCurrentWindow }) => {
    const win = getCurrentWindow();
    if (win.label !== 'main') {
      // Theme + splash already applied in index.html; show only after that paint.
      requestAnimationFrame(() => {
        requestAnimationFrame(() => {
          void win
            .show()
            .then(() => {
              win.setFocus().catch(() => {});
            })
            .catch((e) => {
              console.error(`[bootstrap] failed to show window "${win.label}"`, e);
            });
        });
      });
    }
  });
}

requestAnimationFrame(() => {
  requestAnimationFrame(() => {
    mark('first paint (2× rAF)');
  });
});
