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

import { hideSplash, waitForStartupTask } from './lib/splash';
import { installTauriEventUnlistenRaceWorkaround } from './lib/tauriEventCompat';
import { installDocumentScrollLock } from './lib/documentScrollLock';
import { bootstrapDefaultIconResolver } from './lib/bootstrapIconResolver';
import { maybeCheckOnStartup } from './lib/updater';
import { getWindowKind } from './lib/windowKind';
import { startLocaleSync } from './lib/localeSync';
import { initProExtensions } from './extensions/generated-pro';
import * as extensionPoints from '@datazen/extension-points';
import * as jsxRuntime from 'react/jsx-runtime';
import * as reactAll from 'react';
import * as reactDomAll from 'react-dom';
import * as ui from '@datazen/ui';
import * as cmView from '@codemirror/view';
import * as cmState from '@codemirror/state';
import * as cmLint from '@codemirror/lint';
import * as cmAutocomplete from '@codemirror/autocomplete';
import { useQueryBuilderStore } from './stores/queryBuilderStore';
import { useSchemaStore } from './stores/schemaStore';
import { getCachedTableSchema } from './lib/schemaCache';

// Expose host shared modules so the dynamically-loaded PRO extension can
// resolve bare specifiers from blob URLs. The pack-ep rewrite step
// (scripts/pack-ep.mjs `rewriteEpImportsToHostGlobals`) redirects every bare
// import in the staged bundle to this table at package time — keep the key
// lists in sync or the blob-loaded bundle throws on a missing key.
(globalThis as any).__DATAZEN_HOST__ = {
  '@datazen/extension-points': extensionPoints,
  '@datazen/ui': ui,
  react: reactAll,
  'react-dom': reactDomAll,
  'react/jsx-runtime': jsxRuntime,
  '@codemirror/view': cmView,
  '@codemirror/state': cmState,
  '@codemirror/lint': cmLint,
  '@codemirror/autocomplete': cmAutocomplete,
};

// E2E test hooks: expose Zustand stores on globalThis for WebDriver tests.
// Module-level window assignments in store files get tree-shaken by Vite,
// so the stores must be registered here in the entry module.
(globalThis as any).__qbStore = useQueryBuilderStore;
(globalThis as any).__schemaStore = useSchemaStore;

// Seed the shared @datazen/ui i18n engine from the persisted language and
// keep it in sync (settings changes → setLocale → useI18n re-renders).
startLocaleSync();
extensionPoints.setTableSchemaProvider(getCachedTableSchema);

bootstrapDefaultIconResolver();
initProExtensions();
installTauriEventUnlistenRaceWorkaround();
// Keep the viewport anchored at (0,0): programmatic scrolls (WebDriver
// scrollIntoView/click, focus()) must never shift the fixed chrome.
installDocumentScrollLock();

const SETTINGS_PRELOAD_TIMEOUT_MS = 3_000;

async function bootstrap() {
  try {
    if ('__TAURI_INTERNALS__' in globalThis) {
      try {
        const settingsPreload = import('./stores/settingsStore').then(
          async ({ useSettingsStore }) => {
            await useSettingsStore.getState().loadSettings();
            mark('settings loaded before first paint');
            if (getWindowKind() === 'main') {
              const { checkForUpdatesOnStartup } = useSettingsStore.getState().settings;
              void maybeCheckOnStartup(checkForUpdatesOnStartup);
            }
          },
        );
        const preloadResult = await waitForStartupTask(
          settingsPreload,
          SETTINGS_PRELOAD_TIMEOUT_MS,
        );
        if (preloadResult === 'timed-out') {
          mark('settings preload timed out; continuing first paint');
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
  } catch (e: any) {
    console.error('[bootstrap] fatal initialization error:', e);
    // Fatal crash fallback: ensure splash is never stuck if React root fails
    hideSplash(document.getElementById('splash'));
  }
}

void bootstrap();

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
