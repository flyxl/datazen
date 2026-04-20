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

ReactDOM.createRoot(document.getElementById('root')!).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>
);
mark('React.render() called (Suspense shell)');

const splash = document.getElementById('splash');
if (splash) {
  splash.classList.add('hide');
  setTimeout(() => splash.remove(), 350);
}

if ('__TAURI_INTERNALS__' in globalThis) {
  import('@tauri-apps/api/window').then(({ getCurrentWindow }) => {
    const win = getCurrentWindow();
    if (win.label !== 'main') {
      void win.show();
    }
  });
}

requestAnimationFrame(() => {
  requestAnimationFrame(() => {
    mark('first paint (2× rAF)');
  });
});
