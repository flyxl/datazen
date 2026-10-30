/**
 * Shared app bootstrap (host-boundary follow-up F2).
 *
 * Everything here used to live in `apps/desktop/src/main.tsx` behind deep
 * `@datazen/frontend/src/...` imports: the startup marks, global CSS, the
 * PRO host-module table, the E2E store bridge, locale / icon / scroll-lock /
 * PRO initialization, the settings preload and the React root. The desktop
 * entry now calls `startSharedApp()` and keeps only desktop-owned steps
 * (Tauri event compat patch, platform assembly, startup update check,
 * sub-window show handshake).
 *
 * A future web host is expected to call the same function with its own
 * `configureHost` (binding the web ports); no HTTP/Axum knowledge lives
 * here — that form is out of scope (see host-boundary-followups.md).
 */
import React from 'react';
import ReactDOM from 'react-dom/client';

import App from '../App';
import '../styles/globals.css';

import * as extensionPoints from '@datazen/extension-points';
import * as ui from '@datazen/ui';
import * as jsxRuntime from 'react/jsx-runtime';
import * as reactAll from 'react';
import * as reactDomAll from 'react-dom';
import * as cmView from '@codemirror/view';
import * as cmState from '@codemirror/state';
import * as cmLint from '@codemirror/lint';
import * as cmAutocomplete from '@codemirror/autocomplete';
import * as cmLanguage from '@codemirror/language';
import * as cmCommands from '@codemirror/commands';

import { mark } from '../lib/startupTimer';
import { hideSplash, waitForStartupTask } from '../lib/splash';
import { installDocumentScrollLock } from '../lib/documentScrollLock';
import { bootstrapDefaultIconResolver } from '../lib/bootstrapIconResolver';
import { startLocaleSync } from '../lib/localeSync';
import { getCachedTableSchema } from '../lib/schemaCache';
import { initProExtensions } from '../extensions/generated-pro';
import { useSchemaStore } from '../stores/schemaStore';
import { useTableDataStore } from '../stores/tableDataStore';
import { useFkNavigationStore } from '../stores/fkNavigationStore';
import { usePanelStore } from '../stores/panelStore';
import { IS_DESKTOP_BUILD } from '../platform/env';
import type { AppSettings } from '../types/index';

/** Host-owned startup options; see `startSharedApp`. */
export interface StartSharedAppOptions {
  /**
   * Host wiring that must run before the settings preload and the first
   * paint (desktop: backend binding, platform port injection, window chrome
   * registration — `assembleDesktopPlatform`). Runs only in desktop builds;
   * the future web host binds its own ports here.
   */
  configureHost?: () => void;
  /**
   * Called once the persisted settings preload settles — with the loaded
   * settings on success, `null` when the preload failed or timed out.
   * Desktop uses this for the startup update check (main window only);
   * the settings store itself is always seeded either way.
   */
  onSettingsPreloaded?: (settings: AppSettings | null) => void;
}

/**
 * Expose host shared modules so the dynamically-loaded PRO extension can
 * resolve bare specifiers from blob URLs. The pack-ep rewrite step
 * (scripts/pack-ep.mjs `rewriteEpImportsToHostGlobals`) redirects every bare
 * import in the staged bundle to this table at package time — keep the key
 * lists in sync or the blob-loaded bundle throws on a missing key.
 * The two lists are deliberately NOT interchangeable with the extension's
 * build-time externalize rule (a wide `/^@codemirror\//` regex): that wide rule
 * only guarantees a new package gets checked, this table and pack-ep's
 * `HOST_SHARED_MODULES` are the narrow, explicit registration gate. A regex
 * here would let an unverified specifier through and silently ship a second
 * copy of a module — cross-realm identity split, no error. Every shared
 * module must be spelled out on both sides; `scripts/__tests__/pack-ep.test.ts`
 * fails the build if the two lists drift.
 *
 * Module scope on purpose: pack-ep's `readHostGlobalTableKeys` parses this
 * literal from source (`HOST_TABLE_SOURCE`), and its parser accepts only a
 * top-level table — the closing brace must sit at column zero.
 */
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

/**
 * E2E test hooks: expose Zustand stores on globalThis for WebDriver tests.
 * Module-level window assignments in store files get tree-shaken by Vite,
 * so the stores must be registered here in the bootstrap module.
 */
function registerE2eStoreBridge(): void {
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
}

const SETTINGS_PRELOAD_TIMEOUT_MS = 3_000;

/**
 * Run the shared startup chain. Synchronous: kicks off the async bootstrap
 * (settings preload → React root) and returns, so host-owned follow-ups
 * (desktop: the sub-window show handshake) keep their pre-F2 ordering.
 */
export function startSharedApp(options: StartSharedAppOptions = {}): void {
  const htmlStart = (globalThis as Record<string, unknown>).__HTML_START__ as number | undefined;
  if (htmlStart) {
    mark(`HTML → JS module (${Math.round(performance.now() - htmlStart)}ms from HTML)`);
  } else {
    mark('JS module start');
  }
  mark('core imports done (React, ReactDOM)');
  mark('App module loaded');
  mark('CSS loaded');

  // Seed the shared @datazen/ui i18n engine from the persisted language and
  // keep it in sync (settings changes → setLocale → useI18n re-renders).
  startLocaleSync();
  extensionPoints.setTableSchemaProvider(getCachedTableSchema);

  bootstrapDefaultIconResolver();
  initProExtensions();
  // Keep the viewport anchored at (0,0): programmatic scrolls (WebDriver
  // scrollIntoView/click, focus()) must never shift the fixed chrome.
  installDocumentScrollLock();

  registerE2eStoreBridge();

  const bootstrap = async () => {
    try {
      // Form is decided at COMPILE time (ruling #5) — no runtime probing.
      if (IS_DESKTOP_BUILD) {
        // Backend binding + platform port injection + chrome registration.
        options.configureHost?.();
        try {
          const settingsPreload = import('../stores/settingsStore').then(
            async ({ useSettingsStore }) => {
              await useSettingsStore.getState().loadSettings();
              mark('settings loaded before first paint');
              return useSettingsStore.getState().settings;
            },
          );
          const preloadResult = await waitForStartupTask(
            settingsPreload,
            SETTINGS_PRELOAD_TIMEOUT_MS,
          );
          if (preloadResult === 'timed-out') {
            mark('settings preload timed out; continuing first paint');
            options.onSettingsPreloaded?.(null);
          } else {
            options.onSettingsPreloaded?.(await settingsPreload);
          }
        } catch {
          mark('settings preload skipped (load failed)');
          options.onSettingsPreloaded?.(null);
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
  };

  void bootstrap();

  requestAnimationFrame(() => {
    requestAnimationFrame(() => {
      mark('first paint (2× rAF)');
    });
  });
}
