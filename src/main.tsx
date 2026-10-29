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
import { maybeCheckOnStartup } from './platform/desktop/updater';
import { getWindowKind } from './lib/windowKind';
import { assembleDesktopPlatform, showSubWindowWhenReady } from './shell/desktop/assembly';
import { IS_DESKTOP_BUILD } from './platform/env';
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
import * as cmLanguage from '@codemirror/language';
import * as cmCommands from '@codemirror/commands';
import { useSchemaStore } from './stores/schemaStore';
import { useTableDataStore } from './stores/tableDataStore';
import { useFkNavigationStore } from './stores/fkNavigationStore';
import { usePanelStore } from './stores/panelStore';
import { getCachedTableSchema } from './lib/schemaCache';

// Expose host shared modules so the dynamically-loaded PRO extension can
// resolve bare specifiers from blob URLs. The pack-ep rewrite step
// (scripts/pack-ep.mjs `rewriteEpImportsToHostGlobals`) redirects every bare
// import in the staged bundle to this table at package time — keep the key
// lists in sync or the blob-loaded bundle throws on a missing key.
// The two lists are deliberately NOT interchangeable with the extension's
// build-time externalize rule (a wide `/^@codemirror\//` regex): that wide rule
// only guarantees a new package gets checked, this table and pack-ep's
// `HOST_SHARED_MODULES` are the narrow, explicit registration gate. A regex
// here would let an unverified specifier through and silently ship a second
// copy of a module — cross-realm identity split, no error. Every shared
// module must be spelled out on both sides; `scripts/__tests__/pack-ep.test.ts`
// fails the build if the two lists drift.
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
  '@codemirror/language': cmLanguage,
  '@codemirror/commands': cmCommands,
};

// E2E test hooks: expose Zustand stores on globalThis for WebDriver tests.
// Module-level window assignments in store files get tree-shaken by Vite,
// so the stores must be registered here in the entry module.
(globalThis as any).__schemaStore = useSchemaStore;
// `__tableDataStore` is registered HERE ONLY — the store file must not register it too:
// two owners race (an imported module body evaluates before the importer's, so a second
// assignment overwrites the first one with `undefined` in a dev build).
//
// The handle is live in the two builds that need it — the webdriver build (VITE_E2E=1, set
// by scripts/e2e-tauri-build.mjs, which every E2E build path goes through) and the dev
// server (DEV=true). A production build publishes `undefined` instead: no usable handle,
// but the assignment itself survives tree-shaking, so the name stays in the artifact and
// the bridge does not silently disappear from the bundle that has to run it.
if (import.meta.env.VITE_E2E || import.meta.env.DEV) {
  (globalThis as unknown as { __tableDataStore?: typeof useTableDataStore }).__tableDataStore =
    useTableDataStore;
  // The FK navigation snapshot is the only record of "where the user was" across a
  // jump the panel unmounts for; E2E has to be able to see whether it was taken.
  (
    globalThis as unknown as { __fkNavigationStore?: typeof useFkNavigationStore }
  ).__fkNavigationStore = useFkNavigationStore;
  // Which tab is in front decides when a stored viewport is consumed, so the
  // panel store has to be readable from a spec as well.
  (globalThis as unknown as { __panelStore?: typeof usePanelStore }).__panelStore = usePanelStore;
} else {
  (globalThis as unknown as { __tableDataStore?: typeof useTableDataStore }).__tableDataStore =
    undefined;
  (
    globalThis as unknown as { __fkNavigationStore?: typeof useFkNavigationStore }
  ).__fkNavigationStore = undefined;
  (globalThis as unknown as { __panelStore?: typeof usePanelStore }).__panelStore = undefined;
}

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
    // Form is decided at COMPILE time (ruling #5) — no runtime probing.
    if (IS_DESKTOP_BUILD) {
      // Backend binding + platform port injection + chrome registration.
      assembleDesktopPlatform();
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

showSubWindowWhenReady();

requestAnimationFrame(() => {
  requestAnimationFrame(() => {
    mark('first paint (2× rAF)');
  });
});
