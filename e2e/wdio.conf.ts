import path from 'node:path';
import url from 'node:url';

const __dirname = path.dirname(url.fileURLToPath(import.meta.url));

function getAppBinaryPath(): string {
  const target = path.resolve(__dirname, '../src-tauri/target');
  if (process.platform === 'win32') {
    return path.join(target, 'debug/datazen.exe');
  }
  return path.join(target, 'debug/datazen');
}

export const config: WebdriverIO.Config = {
  runner: 'local',
  specs: ['./specs/**/*.ts'],
  maxInstances: 1,
  capabilities: [
    {
      'tauri:options': {
        application: getAppBinaryPath(),
      },
    } as WebdriverIO.Capabilities,
  ],
  hostname: '127.0.0.1',
  port: 4444,
  path: '/',
  logLevel: 'warn',
  waitforTimeout: 10000,
  connectionRetryTimeout: 30000,
  connectionRetryCount: 3,
  framework: 'mocha',
  reporters: ['spec'],
  mochaOpts: {
    ui: 'bdd',
    timeout: 60000,
  },
  before: async function () {
    // Global setup: force language to zh-CN so all Chinese selectors work
    await browser.pause(2000);
    await browser.executeAsync((done: (r: unknown) => void) => {
      (window as any).__TAURI_INTERNALS__
        .invoke('save_settings', {
          settings: {
            theme: 'dark',
            language: 'zh-CN',
            limitSelectResults: true,
            queryResultLimit: 1000,
            editorFontSize: 14,
            editorFontFamily: 'monospace',
            confirmOnDelete: true,
            autoCommit: false,
            defaultPageSize: 50,
          },
        })
        .then(() => done(null))
        .catch((e: unknown) => done(String(e)));
    });
    // Reload page so the new language takes effect
    await browser.execute(() => location.reload());
    await browser.pause(2000);
    // Expand all connection groups so items are visible
    await browser.execute(() => {
      document.querySelectorAll('[data-group-header]').forEach((el) => {
        const parent = el.closest('[data-group-name]');
        if (parent && !parent.querySelector('[data-conn-item]')) {
          (el as HTMLElement).click();
        }
      });
    });
    await browser.pause(500);
  },
};
