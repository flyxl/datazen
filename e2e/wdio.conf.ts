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
    await browser.pause(2000);

    // Force language to zh-CN so all Chinese selectors work
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

    // Seed a PostgreSQL connection if none exist (required by most test suites)
    const connCount = await browser.executeAsync((done: (r: number) => void) => {
      (window as any).__TAURI_INTERNALS__
        .invoke('get_connections')
        .then((conns: any[]) => done(conns.length))
        .catch(() => done(0));
    });

    if (connCount === 0) {
      const pgHost = process.env.PG_HOST || '127.0.0.1';
      const pgUser = process.env.PG_USER || 'postgres';
      const pgPassword = process.env.PG_PASSWORD || '';
      const pgDatabase = process.env.PG_DATABASE || 'postgres';
      await browser.executeAsync(
        (host: string, user: string, pw: string, db: string, done: (r: unknown) => void) => {
          const config = {
            id: 'conn_e2e_pg',
            name: '本地 PostgreSQL',
            databaseType: 'postgresql',
            host,
            port: 5432,
            username: user,
            password: pw,
            database: db,
            group: 'E2E 测试',
            colorTag: 'blue',
            sslMode: 'disable',
          };
          (window as any).__TAURI_INTERNALS__
            .invoke('save_connection', { config })
            .then(() => done(null))
            .catch((e: unknown) => done(String(e)));
        },
        pgHost,
        pgUser,
        pgPassword,
        pgDatabase,
      );
    }

    // Reload page so the new language and seeded connections take effect
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
