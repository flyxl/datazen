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

async function bootstrap() {
  if ('__TAURI_INTERNALS__' in globalThis) {
    try {
      const { useSettingsStore } = await import('./stores/settingsStore');
      await useSettingsStore.getState().loadSettings();
      mark('settings loaded before first paint');
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
}

void bootstrap();

const splash = document.getElementById('splash');
if (splash) {
  splash.classList.add('hide');
  setTimeout(() => splash.remove(), 350);
}

if ('__TAURI_INTERNALS__' in globalThis) {
  import('@tauri-apps/api/window').then(({ getCurrentWindow }) => {
    const win = getCurrentWindow();
    if (win.label !== 'main') {
      void win.show().then(() => {
        win.setFocus().catch(() => {});
      });
    }
  });
}

requestAnimationFrame(() => {
  requestAnimationFrame(() => {
    mark('first paint (2× rAF)');
  });
});
