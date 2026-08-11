export const config: WebdriverIO.Config = {
  runner: 'local',
  specs: ['./specs/**/*.ts'],
  maxInstances: 1,
  capabilities: [{}],
  hostname: '127.0.0.1',
  port: 4445,
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
    await browser.url('tauri://localhost');
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

    // Ensure a PostgreSQL connection with env credentials exists (upsert)
    {
      const pgHost = process.env.E2E_PG_HOST || process.env.PG_HOST || '127.0.0.1';
      const pgPort = Number(process.env.E2E_PG_PORT || process.env.PG_PORT) || 5432;
      const pgUser = process.env.E2E_PG_USER || process.env.PG_USER || 'postgres';
      const pgPassword = process.env.E2E_PG_PASSWORD || process.env.PG_PASSWORD || '';
      const pgDatabase = process.env.E2E_PG_DB || process.env.PG_DATABASE || 'postgres';
      await browser.executeAsync(
        (host: string, port: number, user: string, pw: string, db: string, done: (r: unknown) => void) => {
          const config = {
            id: 'conn_e2e_pg',
            name: '本地 PostgreSQL',
            databaseType: 'postgresql',
            host,
            port,
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
        pgPort,
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
  beforeSuite: async function () {
    // Same Tauri process is reused across spec files; close leftover sub-windows
    // so Host specs do not attach to a previous MultiDb / SQLite session.
    try {
      await browser.url('tauri://localhost');
      await browser.pause(400);
      const handles = await browser.getWindowHandles();
      const main = handles[0];
      for (const h of handles) {
        if (h === main) continue;
        try {
          await browser.switchToWindow(h);
          await browser.closeWindow();
        } catch {
          /* ignore */
        }
      }
      if (main) await browser.switchToWindow(main);
    } catch {
      /* ignore */
    }
  },
};
